// bun run i18n:extract <path-or-glob>... [--area <area>] [--dry-run] [--json] [--plan]
// Moves the user-facing text of Svelte components into the catalog and replaces it with the message call the app
// already uses (`m.area_key()`): text nodes, `title`/`placeholder`/`aria-label`/`alt`/`label`... attributes (kit
// components included). English only: `en.json` and `meta.json` of the area get the keys (with a description derived from
// the component and the element), `common*` messages are reused when the English is the same (never duplicated), and
// pt-BR is left to `bun run i18n:translate` or the migrating agent (this tool never invents a translation).
//
// What it will not touch is listed with file:line and a reason in the report (`--json`): plurals, text joined to an
// expression or split by inline markup, template literals with logic, string literals inside `{...}`, shortcuts (kbd()),
// URLs, identifier-like tokens, text that fails the catalog rules, and every string in `.ts` modules and `<script>` blocks.
// A `.rs` path gets a worklist and is never rewritten (extract/rust.ts says why). `--dry-run` writes nothing.
// `--plan` prints the migration batches (i18n/MIGRATION.md) from the hardcoded baseline.
import { existsSync, statSync, writeFileSync } from "node:fs";
import { isAbsolute, join, relative } from "node:path";
import { areaPath, CommandError, readObject, writeObject } from "./edit.ts";
import { sourceEntryIssues } from "./engine.ts";
import { type PlanBatch, buildPlan, formatPlan } from "./extract/plan.ts";
import { areaOf, stemOf, surfaceOf } from "./extract/areas.ts";
import { type RustItem, rustWorklist } from "./extract/rust.ts";
import { scriptStrings } from "./extract/strings.ts";
import { type Analysis, type Candidate, type Refusal, analyzeSvelte, applySvelte, importLine } from "./extract/svelte.ts";
import { fix } from "./fix.ts";
import { type MetaEntry } from "./model.ts";
import { AREA_PATTERN, areaOfKey, consumers, isCommon, loadSource, makeEntry, writeAreaFile, AREA_FILE } from "./source.ts";
import { type Ctx, MESSAGES_DIR, loadCtx } from "./types.ts";
import { readText, rel, walk } from "./util.ts";
import { isTestFile } from "./hardcoded/rust.ts";
import { lintText, loadRules, uiProse } from "./writing.ts";

export interface Options {
	targets: string[];
	/** Put every string in this area (default: the area the file belongs to, extract/areas.ts). */
	area?: string;
	dryRun?: boolean;
}

export interface Converted {
	line: number;
	kind: "text" | "attr";
	attr?: string;
	english: string;
	key: string;
	/** `new` key written, `common` the shared message reused, `area` a message of the area with the same English reused. */
	via: "new" | "common" | "area";
	/** Strings of the hardcoded ratchet this accounts for. */
	covers: number;
}

export interface FileReport {
	file: string;
	type: "svelte" | "rust" | "script";
	area?: string;
	converted: Converted[];
	refused: Refusal[];
	/** Rust only: the sites, classified. */
	worklist?: RustItem[];
}

export interface AreaTotals {
	files: number;
	/** Strings of the ratchet converted / refused (what the baseline counts). */
	converted: number;
	refused: number;
	/** New catalog keys. */
	keys: number;
}

export interface Report {
	dryRun: boolean;
	files: FileReport[];
	summary: {
		converted: number;
		refused: number;
		keys: number;
		reused: { common: number; area: number };
		byArea: Record<string, AreaTotals>;
		/** Refusal entries (not strings) by reason, `.ts` and `<script>` heuristics included. */
		byReason: Record<string, number>;
		rust?: Record<string, number>;
	};
	areasCreated: string[];
	problems: string[];
	written: string[];
}

const EXTS = [".svelte", ".rs", ".ts", ".astro"];
const WANTED = (p: string) => EXTS.some((e) => p.endsWith(e)) && !/\.(?:test|spec)\.ts$|\.d\.ts$|[\\/](?:node_modules|paraglide|\.svelte-kit|target)[\\/]/.test(p);

/** The files a path, directory or glob names, repo-relative and sorted. */
export function resolveTargets(root: string, targets: string[]): string[] {
	const out = new Set<string>();
	for (const raw of targets) {
		const t = isAbsolute(raw) ? relative(root, raw) : raw;
		if (t.includes("*")) {
			for (const f of new Bun.Glob(t).scanSync({ cwd: root })) if (WANTED(`/${f}`)) out.add(f.replace(/\\/g, "/"));
			continue;
		}
		const p = join(root, t);
		if (!existsSync(p)) throw new CommandError(`${t}: no such file or directory (paths are relative to the repo root, globs use *)`);
		if (statSync(p).isDirectory()) for (const f of walk(p, WANTED)) out.add(rel(root, f));
		else out.add(t.replace(/\\/g, "/"));
	}
	return [...out].filter((f) => !(f.endsWith(".rs") && isTestFile(f))).sort();
}

const ROLE: Record<string, string> = {
	button: "Button label",
	Button: "Button label",
	IconButton: "Button label",
	h1: "Heading",
	h2: "Heading",
	h3: "Heading",
	h4: "Heading",
	h5: "Heading",
	h6: "Heading",
	label: "Field label",
	legend: "Group title",
	summary: "Disclosure title",
	option: "Option of a list",
	th: "Table column heading",
	td: "Table cell",
	li: "List item",
	p: "Paragraph",
	a: "Link text",
	title: "Page title",
};
const ATTR_ROLE: Record<string, string> = {
	"aria-label": "Accessible name (aria-label, read by screen readers)",
	"aria-description": "Accessible description (aria-description, read by screen readers)",
	title: "Tooltip (title attribute)",
	placeholder: "Placeholder of the field",
	alt: "Alt text of the image",
	label: "label prop",
	tooltip: "Tooltip",
	description: "description prop (help line)",
	message: "message prop",
};

/** A description a translator can use: what kind of text, where, and what each variable is. Derived, so reviewers refine it. */
function describe(file: string, c: Candidate): string {
	const component = file.split("/").pop()?.replace(/\.[a-z]+$/, "") ?? file;
	const owner = c.tag ? (/^[A-Z]/.test(c.tag) ? `<${c.tag}> component` : `<${c.tag}> element`) : "the template";
	const role = c.kind === "attr" ? `${ATTR_ROLE[c.attr ?? ""] ?? c.attr} of a ${owner}` : `${ROLE[c.tag] ?? "Text"} in ${/^[A-Z]/.test(c.tag) ? "the" : "a"} ${owner}`;
	const vars = c.vars.length ? ` ${c.vars.map((v) => `{${v.name}} is ${v.expr}`).join("; ")}.` : "";
	return `${role}, in ${component} (${file}).${vars} Extracted by i18n:extract: refine this description on review.`;
}

const slugOf = (english: string) =>
	english
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, " ")
		.trim()
		.split(" ")
		.filter(Boolean)
		.slice(0, 5)
		.join("_") || "text";

const plain = (t: string) => t.trim().toLowerCase().replace(/[.…]$/, "").trim();

interface Pending {
	english: string;
	description: string;
}

/** Run the codemod (or the worklist) over `o.targets`; writes unless `o.dryRun`. Throws a CommandError that names the fix. */
export function extract(ctx: Ctx, o: Options): Report {
	const root = ctx.root;
	const source = loadSource(ctx);
	const files = resolveTargets(root, o.targets);
	if (files.length === 0) throw new CommandError("no .svelte, .rs, .ts or .astro file matches: pass a path, a directory or a glob (apps/desktop/src/lib/settings/*.svelte)");
	const base = ctx.locales.default;
	const config = source.config;
	const areaNames = new Set(source.areas.keys());
	const created = new Map<string, string>();
	const { rules } = loadRules(root);

	const common = new Map<string, string>();
	const commonPlain = new Map<string, string>();
	const allKeys = new Set<string>();
	for (const a of source.areas.values()) {
		for (const [key, e] of a.entries.get(base) ?? []) {
			allKeys.add(key);
			if (isCommon(a.name) && typeof e.value === "string") {
				common.set(e.value.trim(), key);
				commonPlain.set(plain(e.value), key);
			}
		}
	}
	const sameAreaIndex = (area: string) => {
		const m = new Map<string, string>();
		const a = source.areas.get(area);
		for (const [key, e] of a?.entries.get(base) ?? []) {
			const meta = a?.meta.get(key);
			if (typeof e.value === "string" && !meta?.distinct && !meta?.platform) m.set(e.value.trim(), key);
		}
		return m;
	};

	const areaIndex = new Map<string, Map<string, string>>();
	const pending = new Map<string, Map<string, Pending>>();
	const ensureArea = (area: string, surface: string | undefined) => {
		if (!AREA_PATTERN.test(area)) throw new CommandError(`"${area}" is not an area name: snake_case (settings_capture)`);
		if (isCommon(area)) throw new CommandError(`${area} holds shared terms (Cancel, Trash), not a screen's text: add a term to it with bun run i18n:add, and extract into the area of the screen`);
		if (!surface) throw new CommandError(`cannot tell which surface consumes the strings of this file: only apps/desktop/src, site/src and crates are known`);
		if (created.has(area)) {
			if (created.get(area) !== surface) throw new CommandError(`area ${area} is new in this run for ${created.get(area)}, and ${surface} files cannot share it: extract them with a different --area`);
		} else if (areaNames.has(area)) {
			if (!consumers(config, area).includes(surface)) throw new CommandError(`area ${area} is not consumed by ${surface} (it is by ${consumers(config, area).join(", ") || "no surface"}): pick an area whose area.json names ${surface}, or leave --area off`);
		} else if (!created.has(area)) {
			const clash = [...allKeys].find((k) => k.startsWith(`${area}_`) && areaOfKey(k, [...areaNames]) !== area);
			if (clash) throw new CommandError(`new area ${area} would take over the key ${clash} of another area (the longest area prefix owns a key): pick another name`);
			created.set(area, surface);
			areaNames.add(area);
		}
		if (!areaIndex.has(area)) areaIndex.set(area, sameAreaIndex(area));
		if (!pending.has(area)) pending.set(area, new Map());
	};

	const report: Report = {
		dryRun: o.dryRun === true,
		files: [],
		summary: { converted: 0, refused: 0, keys: 0, reused: { common: 0, area: 0 }, byArea: {}, byReason: {}, rust: undefined },
		areasCreated: [],
		problems: [],
		written: [],
	};
	const edits: { file: string; text: string }[] = [];

	const allocate = (area: string, stem: string, english: string): string => {
		const areaWords = new Set(area.split("_"));
		const stemWords = stem.split("_").filter((w) => w && !areaWords.has(w));
		let key = [area, ...stemWords, slugOf(english)].join("_");
		if (key.length > 64) key = key.slice(0, 64).replace(/_[^_]*$/, "");
		let n = 1;
		let candidate = key;
		while (allKeys.has(candidate)) candidate = `${key}_${++n}`;
		allKeys.add(candidate);
		return candidate;
	};

	for (const file of files) {
		const text = readText(join(root, file));
		const fr: FileReport = { file, type: file.endsWith(".svelte") ? "svelte" : file.endsWith(".rs") ? "rust" : "script", converted: [], refused: [] };
		report.files.push(fr);
		if (fr.type === "rust") {
			const area = o.area ?? areaOf(file) ?? "cmd_misc";
			fr.area = area;
			fr.worklist = rustWorklist(file, text, area);
			continue;
		}
		if (fr.type === "script") {
			fr.refused = scriptStrings(text, "ts-module");
			continue;
		}
		const surface = surfaceOf(file);
		if (surface !== "desktop" && surface !== "site") throw new CommandError(`${file}: Svelte components outside apps/desktop/src and site/src are not covered`);
		const area = o.area ?? areaOf(file);
		if (!area) throw new CommandError(`${file}: no area rule covers it (extract/areas.ts): pass --area <area>`);
		ensureArea(area, surface);
		fr.area = area;
		const analysis: Analysis = analyzeSvelte(text);
		fr.refused.push(...analysis.refused, ...scriptStrings(text, "script-string"));
		const stem = stemOf(file);
		const used: { c: Candidate; key: string }[] = [];
		for (const c of analysis.candidates) {
			const english = c.english;
			const exact = common.get(english);
			if (exact) {
				used.push({ c, key: exact });
				fr.converted.push({ line: c.line, kind: c.kind, attr: c.attr, english, key: exact, via: "common", covers: c.covers });
				continue;
			}
			const near = commonPlain.get(plain(english));
			if (near) {
				fr.refused.push({ line: c.line, reason: "near-common", text: english, covers: c.covers, detail: `differs from ${near} only in case or a final . or …: use ${near} if the same word is meant, or add the key by hand with meta "distinct" saying why` });
				continue;
			}
			const idx = areaIndex.get(area) as Map<string, string>;
			const known = idx.get(english);
			if (known) {
				used.push({ c, key: known });
				fr.converted.push({ line: c.line, kind: c.kind, attr: c.attr, english, key: known, via: "area", covers: c.covers });
				continue;
			}
			const description = describe(file, c);
			const key = allocate(area, stem, english);
			const entry = makeEntry({ key, area, file: `i18n/messages/${area}/en.json`, value: english, lookup: source.lookup(base), meta: { description } satisfies MetaEntry });
			const bad = sourceEntryIssues(entry, [...areaNames]).filter((i) => i.severity === "error").map((i) => `${i.rule}: ${i.message}`);
			bad.push(...lintText(rules, uiProse(english), base, "ui").filter((h) => h.rule.severity === "error").map((h) => `writing-${h.rule.id}: "${h.text}"${h.rule.suggest ? ` (${h.rule.suggest})` : ""}`));
			if (bad.length) {
				allKeys.delete(key);
				fr.refused.push({ line: c.line, reason: "catalog-rule", text: english, covers: c.covers, detail: `${bad.join("; ")}: fix the English by hand, then add it with bun run i18n:add` });
				continue;
			}
			idx.set(english, key);
			(pending.get(area) as Map<string, Pending>).set(key, { english, description });
			used.push({ c, key });
			fr.converted.push({ line: c.line, kind: c.kind, attr: c.attr, english, key, via: "new", covers: c.covers });
		}
		if (used.length) edits.push({ file, text: applySvelte(text, analysis, used, importLine(surface, file)) });
	}

	// per-area budget: the catalog fails `budget` past it, so refuse before writing
	const { keys: maxKeys, bytes: maxBytes } = config.budget;
	for (const [area, items] of pending) {
		const had = source.areas.get(area)?.entries.get(base);
		const keys = (had?.size ?? 0) + items.size;
		const bytes = [...(had?.values() ?? [])].reduce((n, e) => n + e.raw.length + e.key.length + 8, 0) + [...items].reduce((n, [k, p]) => n + k.length + p.english.length + 8, 0);
		if (keys > maxKeys || bytes > maxBytes) report.problems.push(`area ${area} would have ${keys} keys, about ${bytes} bytes (budget ${maxKeys} keys, ${maxBytes} bytes): run on fewer files or pass a narrower --area (settings_capture, not settings), or add the area to extract/areas.ts`);
	}

	for (const fr of report.files) {
		const t = (fr.area ? (report.summary.byArea[fr.area] ??= { files: 0, converted: 0, refused: 0, keys: 0 }) : undefined) as AreaTotals | undefined;
		if (t && fr.type === "svelte") t.files++;
		for (const c of fr.converted) {
			report.summary.converted += c.covers;
			if (t) t.converted += c.covers;
			if (c.via !== "new") report.summary.reused[c.via]++;
		}
		for (const r of fr.refused) {
			report.summary.refused += r.covers;
			if (t) t.refused += r.covers;
			report.summary.byReason[r.reason] = (report.summary.byReason[r.reason] ?? 0) + 1;
		}
		for (const w of fr.worklist ?? []) {
			const rust = (report.summary.rust ??= {});
			rust[w.kind] = (rust[w.kind] ?? 0) + 1;
		}
	}
	for (const [area, items] of pending) {
		report.summary.keys += items.size;
		(report.summary.byArea[area] ??= { files: 0, converted: 0, refused: 0, keys: 0 }).keys = items.size;
	}
	report.areasCreated = [...created.keys()];
	if (o.dryRun) return report;
	if (report.problems.length) throw new CommandError(`nothing written: ${report.problems.join("; ")}`);

	for (const [area, items] of pending) {
		if (items.size === 0) continue;
		const en = readObject(root, areaPath(area, "en"));
		const meta = readObject(root, areaPath(area, "meta"));
		for (const [key, p] of items) {
			en[key] = p.english;
			meta[key] = p.description;
		}
		writeObject(root, areaPath(area, "en"), en);
		writeObject(root, areaPath(area, "meta"), meta);
		report.written.push(areaPath(area, "en"), areaPath(area, "meta"));
	}
	if (created.size) {
		for (const [area, surface] of created) {
			writeAreaFile(root, area, { surface });
			report.written.push(`${MESSAGES_DIR}/${area}/${AREA_FILE}`);
		}
	}
	for (const e of edits) {
		writeFileSync(join(root, e.file), e.text);
		report.written.push(e.file);
	}
	fix(loadCtx(root));
	return report;
}

function printReport(r: Report) {
	for (const f of r.files) {
		const strings = f.converted.reduce((n, c) => n + c.covers, 0);
		if (f.type === "rust") {
			const k: Record<string, number> = {};
			for (const w of f.worklist ?? []) k[w.kind] = (k[w.kind] ?? 0) + 1;
			console.log(`${f.file} (${f.area}): ${(f.worklist ?? []).length} site(s) ${JSON.stringify(k)}`);
			for (const w of f.worklist ?? []) console.log(`  ${f.file}:${w.line} ${w.kind} ${w.macro}!("${w.format.slice(0, 70)}")${w.reasons.length ? ` [${w.reasons.map((x) => x.split(":")[0]).join(", ")}]` : ""}`);
			continue;
		}
		console.log(`${f.file}${f.area ? ` (${f.area})` : ""}: ${strings} converted, ${f.refused.length} left`);
		for (const c of f.converted) console.log(`  ${f.file}:${c.line} ${c.via} ${c.key} "${c.english.slice(0, 60)}"`);
		for (const x of f.refused) console.log(`  ${f.file}:${x.line} ${x.reason} "${x.text.slice(0, 60)}"${x.detail ? `: ${x.detail}` : ""}`);
	}
	const s = r.summary;
	console.log(`\n${r.dryRun ? "dry run: nothing written. " : ""}${s.converted} string(s) ${r.dryRun ? "would be " : ""}converted into ${s.keys} new key(s) (${s.reused.common} common, ${s.reused.area} same-area reused), ${s.refused} left by hand.`);
	for (const p of r.problems) console.log(`problem: ${p}`);
	if (!r.dryRun && r.written.length) console.log(`pt-BR is required: bun run i18n:translate --locale pt-BR, then bun run i18n:fix and bun run i18n:check`);
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const flag = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const json = args.includes("--json");
	const root = flag("--root") ?? join(import.meta.dir, "..", "..");
	const valueFlags = ["--area", "--root"];
	const targets = args.filter((a, i) => !a.startsWith("--") && !valueFlags.includes(args[i - 1] ?? ""));
	try {
		if (args.includes("--plan")) {
			const plan: PlanBatch[] = buildPlan(root);
			console.log(json ? JSON.stringify(plan, null, 2) : formatPlan(plan));
		} else {
			if (targets.length === 0) throw new CommandError("usage: bun run i18n:extract <path-or-glob>... [--area <area>] [--dry-run] [--json]   (or --plan)");
			const report = extract(loadCtx(root), { targets, area: flag("--area"), dryRun: args.includes("--dry-run") });
			if (json) console.log(JSON.stringify(report, null, 2));
			else printReport(report);
		}
	} catch (e) {
		if (!(e instanceof CommandError)) throw e;
		if (json) console.error(JSON.stringify({ error: e.message }));
		else console.error(`i18n:extract: ${e.message}`);
		process.exit(1);
	}
}
