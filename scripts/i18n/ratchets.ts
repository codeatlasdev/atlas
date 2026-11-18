// The ratchets: scanners of existing offenders whose per-file count may only go down (i18n/hardcoded-baseline.jsonl).
// Each one is a definition (where it looks, what a scan returns); `runRatchet` is what the checkers call and
// `scanRatchet` what `i18n:fix` uses to shrink the baseline.
import { existsSync } from "node:fs";
import { join } from "node:path";
import { type Found, type Lang, type RatchetText, type Violation, parseBaseline, ratchet, readBaseline, staleEntries } from "./baseline.ts";
import { showAt } from "../lib/base.ts";
import { PLATFORM, SHORTCUT, lineViolations, scanLines } from "./hardcoded/lines.ts";
import { RUST_DIRS, isTestFile, scanRust, scanRustSource } from "./hardcoded/rust.ts";
import { SVELTE_ROOTS, scanSvelte, scanSvelteSource } from "./hardcoded/svelte.ts";
import { BASELINE_FILE, type Ctx, type Finding } from "./types.ts";
import { docFiles, hitMessage, lintMarkdown, loadRules } from "./writing.ts";
import { readText } from "./util.ts";

export interface RatchetDef extends RatchetText {
	/** Surface name -> directories (or single files) it scans; `--surface <name>` selects one. */
	scopes: { surface: string; dirs: string[] }[];
	scan(ctx: Ctx, dirs: string[]): { found: Found; unparsable?: { file: string; message: string }[] };
	/** The offenders of one file's text with the same detector as `scan` (what a file looked like at the diff base);
	 * undefined when the text cannot be scanned. */
	scanText(ctx: Ctx, file: string, text: string): Violation[] | undefined;
}

const SOURCE_EXTS = [".svelte", ".ts", ".astro", ".html", ".rs"];
const NON_RUST_EXTS = SOURCE_EXTS.filter((e) => e !== ".rs");

export const SVELTE: RatchetDef = {
	lang: "svelte",
	rule: "hardcoded",
	noun: "hardcoded user-facing text",
	hint: "use a message key, or `i18n-ignore` if it is not for users",
	scopes: SVELTE_ROOTS.map((r) => ({ surface: r.surface, dirs: [r.dir] })),
	scan: (ctx, dirs) => scanSvelte(ctx.root, dirs),
	scanText(_ctx, file, text) {
		if (!file.endsWith(".svelte")) return [];
		try {
			return scanSvelteSource(text);
		} catch {
			return undefined;
		}
	},
};

export const RUST: RatchetDef = {
	lang: "rust",
	rule: "hardcoded",
	noun: "hardcoded user-facing text",
	hint: "use a message key, or `// i18n-ignore` if it is not for users",
	scopes: [{ surface: "rust", dirs: RUST_DIRS }],
	scan: (ctx, dirs) => ({ found: scanRust(ctx.root, dirs) }),
	scanText: (_ctx, file, text) => (file.endsWith(".rs") && !isTestFile(file) ? scanRustSource(text) : []),
};

export const SHORTCUTS: RatchetDef = {
	lang: "shortcut",
	rule: "shortcut",
	noun: "shortcut written into user-facing source",
	hint: "a shortcut is a {key} placeholder filled by keys.ts kbd()/label() (Rust: the shortcut helper)",
	scopes: [
		{ surface: "desktop", dirs: ["apps/desktop/src"] },
		{ surface: "site", dirs: ["site/src"] },
		{ surface: "rust", dirs: RUST_DIRS },
	],
	scan: (ctx, dirs) => ({ found: scanLines(ctx.root, dirs, SOURCE_EXTS, SHORTCUT) }),
	scanText: (_ctx, file, text) => lineViolations(file, text, SOURCE_EXTS, SHORTCUT),
};

export const PLATFORM_SNIFFING: RatchetDef = {
	lang: "platform",
	rule: "platform-check",
	noun: "ad hoc platform check",
	hint: "the platform is read once, in apps/desktop/src/lib/keys.ts (IS_MAC, IS_WIN); use that or the os the host passes",
	scopes: [
		{ surface: "desktop", dirs: ["apps/desktop/src"] },
		{ surface: "site", dirs: ["site/src"] },
	],
	scan: (ctx, dirs) => ({ found: scanLines(ctx.root, dirs, NON_RUST_EXTS, PLATFORM) }),
	scanText: (_ctx, file, text) => lineViolations(file, text, NON_RUST_EXTS, PLATFORM),
};

/** Existing prose (English docs, README, skills, site content, CHANGELOG): error-level hits are counted, not failed. */
export const WRITING: RatchetDef = {
	lang: "writing",
	rule: "writing",
	noun: "flagged writing",
	hint: "rewrite it (i18n/style/README.md), or add `<!-- writing-lint: allow RULE_ID: reason -->` above it",
	scopes: [{ surface: "writing", dirs: ["docs", "skills", "site/src/content", "README.md", "CHANGELOG.md"] }],
	scan(ctx) {
		const found: Found = new Map();
		const { existing } = docFiles(ctx.root, Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default));
		for (const file of existing) {
			const out = this.scanText(ctx, file, readText(join(ctx.root, file))) ?? [];
			if (out.length) found.set(file, out);
		}
		return { found };
	},
	scanText(ctx, _file, text) {
		return lintMarkdown(loadRules(ctx.root).rules, text, "en")
			.hits.filter((h) => h.rule.severity === "error")
			.map((h) => ({ line: h.line, text: h.text, detail: hitMessage(h), rule: `writing-${h.rule.id}` }));
	},
};

export const RATCHETS: RatchetDef[] = [SVELTE, RUST, SHORTCUTS, PLATFORM_SNIFFING, WRITING];

/** What `def` finds in the scopes of `surface` (every scope when undefined) that exist in the repo. */
export function scanRatchet(ctx: Ctx, def: RatchetDef, surface?: string) {
	const found: Found = new Map();
	const prefixes: string[] = [];
	const unparsable: { file: string; message: string }[] = [];
	for (const scope of def.scopes.filter((s) => !surface || s.surface === surface)) {
		const dirs = scope.dirs.filter((d) => existsSync(join(ctx.root, d)));
		if (dirs.length === 0) continue;
		prefixes.push(...dirs);
		const r = def.scan(ctx, dirs);
		for (const [f, v] of r.found) found.set(f, v);
		unparsable.push(...(r.unparsable ?? []));
	}
	return { found, prefixes, unparsable };
}

/** The offenders `head` has that the file did not have at the base: per distinct offender, as many of the last
 * occurrences as the count grew by (so moving or reflowing old code adds nothing, one more copy of a string does). */
function addedOffenders(head: Violation[], base: Violation[], def: RatchetDef): Violation[] {
	const key = (v: Violation) => `${v.rule ?? def.rule}\u0000${v.text}`;
	const had = new Map<string, number>();
	for (const v of base) had.set(key(v), (had.get(key(v)) ?? 0) + 1);
	const groups = new Map<string, Violation[]>();
	for (const v of head) groups.set(key(v), [...(groups.get(key(v)) ?? []), v]);
	const added: Violation[] = [];
	for (const [k, list] of groups) added.push(...list.slice(list.length - Math.max(0, list.length - (had.get(k) ?? 0))));
	return added.sort((a, b) => a.line - b.line);
}

/** Diff mode: only files the change touches, only offenders it adds. */
function newOffenders(ctx: Ctx, def: RatchetDef, found: Found): Finding[] {
	const base = ctx.base;
	if (!base) return [];
	const out: Finding[] = [];
	for (const [file, head] of [...found].sort(([a], [b]) => (a < b ? -1 : 1))) {
		if (!base.changed.has(file)) continue;
		const was = base.changed.get(file);
		const before = was === undefined ? undefined : showAt(ctx.root, base.mb, was);
		const old = before === undefined ? [] : (def.scanText(ctx, file, before) ?? []);
		for (const v of addedOffenders(head, old, def)) {
			out.push({
				rule: v.rule ?? def.rule,
				severity: "error",
				file,
				line: v.line,
				message: `${def.noun} ${v.detail ?? JSON.stringify(v.text)} is new in this change (${head.length} in this file, ${old.length} at ${base.ref}); ${def.hint}`,
			});
		}
	}
	return out;
}

/** The baseline file is the shrinking floor of the ratchet: a change that raises a count is snapshotting offenders
 * instead of fixing them. Skipped when the base had no baseline (or no such section): that is its introduction. */
function baselineGrowth(ctx: Ctx, lang: Lang): Finding[] {
	const base = ctx.base;
	const text = base ? showAt(ctx.root, base.mb, BASELINE_FILE) : undefined;
	if (!base || text === undefined) return [];
	let before: Record<string, number> | undefined;
	try {
		before = parseBaseline(text)[lang];
	} catch {
		return [];
	}
	if (!before) return [];
	return Object.entries(readBaseline(ctx.root)[lang] ?? {})
		.filter(([file, n]) => n > (before?.[file] ?? 0))
		.map(([file, n]) => ({
			rule: "baseline-grew",
			severity: "error" as const,
			file: BASELINE_FILE,
			key: file,
			message: `this change raises the ${lang} baseline of ${file} from ${before?.[file] ?? 0} to ${n}; the baseline only shrinks: fix the offenders (or mark them i18n-ignore), do not snapshot them`,
		}));
}

/** Ratchet findings of `def` for the run's `--surface`: against the diff base when there is one, else the baseline. */
export function runRatchet(ctx: Ctx, def: RatchetDef): Finding[] {
	const { found, prefixes, unparsable } = scanRatchet(ctx, def, ctx.surface);
	if (prefixes.length === 0) return [];
	const unreadable = unparsable.map((u): Finding => ({ rule: def.rule, severity: "error", file: u.file, message: `cannot parse for the ${def.lang} scan: ${u.message}` }));
	if (!ctx.base) return [...unreadable, ...ratchet(ctx.root, def, found, prefixes)];
	return [...unreadable, ...newOffenders(ctx, def, found), ...baselineGrowth(ctx, def.lang), ...staleEntries(ctx.root, def, found, prefixes)];
}
