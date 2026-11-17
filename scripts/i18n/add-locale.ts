// bun run i18n:add-locale <tag> [--owner @user] [--tier community|required]
// Adds a language in one PR by editing only: i18n/locales.json, .github/CODEOWNERS (two lines), an empty
// `<tag>.json` in every area of i18n/messages, every project.inlang `locales` list, the `SUPPORTED` list of
// crates/atlas-core/src/lang.rs, CFBundleLocalizations of crates/atlas-desktop/Info.plist; then it runs the generators (they pick the language up). It prints what is left to
// do. A new language starts as `community`: missing keys warn and the UI falls back to English until a native speaker
// owns it.
import { existsSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { generateAll } from "./generate.ts";
import { loadSource } from "./source.ts";
import { LOCALES_FILE, loadCtx, loadLocales, type LocalesConfig } from "./types.ts";
import { readText } from "./util.ts";

export interface AddLocaleOptions {
	owner?: string;
	tier?: "community" | "required";
}

const TAG = /^[a-z]{2,3}(?:-[A-Z][a-z]{3})?(?:-(?:[A-Z]{2}|\d{3}))?$/;
const CODEOWNERS = ".github/CODEOWNERS";
const LANG_RS = "crates/atlas-core/src/lang.rs";
const PLIST = "crates/atlas-desktop/Info.plist";

/** i18n/locales.json in the layout it is kept in: one locale per line. */
export function serializeLocales(c: LocalesConfig): string {
	const lines = Object.entries(c.locales).map(([tag, i]) => {
		const owners = i.owners?.length ? `, "owners": [${i.owners.map((o) => JSON.stringify(o)).join(", ")}]` : "";
		return `\t\t${JSON.stringify(tag)}: { "tier": ${JSON.stringify(i.tier)}${owners} }`;
	});
	return `{\n\t"default": ${JSON.stringify(c.default)},\n\t"locales": {\n${lines.join(",\n")}\n\t}\n}\n`;
}

/** `text` with `tag` added to the JSON array `"locales": [...]`, keeping its single- or multi-line layout. */
export function addToLocalesArray(text: string, tag: string): string {
	return text.replace(/("locales"\s*:\s*\[)([^\]]*)(\])/, (_m, open: string, body: string, close: string) => {
		if (body.includes(JSON.stringify(tag))) return `${open}${body}${close}`;
		const trimmed = body.replace(/\s+$/, "");
		const indent = /\n([ \t]*)"/.exec(body)?.[1];
		if (indent !== undefined) return `${open}${trimmed},\n${indent}${JSON.stringify(tag)}${body.slice(trimmed.length)}${close}`;
		return `${open}${trimmed ? `${trimmed}, ` : ""}${JSON.stringify(tag)}${close}`;
	});
}

export function addLocale(root: string, tag: string, opts: AddLocaleOptions = {}): string[] {
	if (!TAG.test(tag)) throw new Error(`"${tag}" is not a canonical BCP-47 tag (language[-Script][-REGION], e.g. es, pt-BR, zh-Hans-CN)`);
	const owner = opts.owner ?? "@codeatlasdev/atlas-maintainers";
	if (!/^@[\w.-]+(?:\/[\w.-]+)?$/.test(owner)) throw new Error(`--owner must be a GitHub @user or @org/team, got "${owner}"`);
	const locales = loadLocales(root);
	if (locales.locales[tag]) throw new Error(`${tag} is already in ${LOCALES_FILE}`);
	const log: string[] = [];

	locales.locales[tag] = { tier: opts.tier ?? "community", owners: [owner] };
	writeFileSync(join(root, LOCALES_FILE), serializeLocales(locales));
	log.push(`${LOCALES_FILE}: added ${tag} (${opts.tier ?? "community"}, ${owner})`);

	const owners = join(root, CODEOWNERS);
	if (existsSync(owners)) {
		const lines = readText(owners).split("\n");
		const block = [`**/${tag}/**`, `/i18n/messages/*/${tag}.json`].map((p) => `${p.padEnd(28)} ${owner}`);
		let at = lines.map((l) => /^\/i18n\/messages\/\*\/\S+\.json\s/.test(l)).lastIndexOf(true);
		if (at < 0) at = lines.map((l) => /^\*\*\/[\w-]+\/\*\*\s/.test(l)).lastIndexOf(true);
		if (at < 0) at = lines.length - (lines.at(-1) === "" ? 2 : 1);
		lines.splice(at + 1, 0, ...block);
		writeFileSync(owners, lines.join("\n"));
		log.push(`${CODEOWNERS}: added 2 lines for ${tag}`);
	} else log.push(`${CODEOWNERS}: not found, skipped`);

	const ctx = loadCtx(root);
	const source = loadSource(ctx);
	for (const area of source.areas.values()) {
		const file = join(root, area.dir, `${tag}.json`);
		if (!existsSync(file)) writeFileSync(file, "{}\n");
	}
	log.push(`i18n/messages/*/${tag}.json: created empty for ${source.areas.size} area(s)`);
	for (const [name, def] of Object.entries(source.config.surfaces)) {
		if (def.paraglide) {
			const settings = join(root, dirname(def.paraglide), "project.inlang", "settings.json");
			if (existsSync(settings)) {
				writeFileSync(settings, addToLocalesArray(readText(settings), tag));
				log.push(`${settings.slice(root.length + 1)}: added ${tag}`);
			} else log.push(`${name}: no project.inlang/settings.json next to ${def.paraglide}, add ${tag} to its locales by hand`);
		}
	}

	const lang = join(root, LANG_RS);
	if (existsSync(lang)) {
		const src = readText(lang);
		const next = src.replace(/(pub const SUPPORTED: &\[&str\] = &\[)([^\]]*)(\];)/, (_m, a: string, body: string, c: string) => `${a}${body.trim() ? `${body.trim()}, ` : ""}"${tag}"${c}`);
		if (next === src) log.push(`${LANG_RS}: no \`pub const SUPPORTED: &[&str] = &[...]\` line found, add "${tag}" by hand`);
		else {
			writeFileSync(lang, next);
			log.push(`${LANG_RS}: added "${tag}" to SUPPORTED`);
		}
	} else log.push(`${LANG_RS}: not found (atlas_core::lang has not landed in this tree), add "${tag}" to SUPPORTED when it does`);

	const plist = join(root, PLIST);
	if (existsSync(plist)) {
		const src = readText(plist);
		const next = src.replace(/(<key>CFBundleLocalizations<\/key>\s*<array>)([\s\S]*?)(\s*<\/array>)/, (_m, a: string, body: string, c: string) => {
			const indent = /\n([ \t]*)<string>/.exec(body)?.[1] ?? "\t\t";
			return `${a}${body.replace(/\s+$/, "")}\n${indent}<string>${tag}</string>${c}`;
		});
		if (next === src) log.push(`${PLIST}: no CFBundleLocalizations array found, add <string>${tag}</string> by hand`);
		else {
			writeFileSync(plist, next);
			log.push(`${PLIST}: added ${tag} to CFBundleLocalizations`);
		}
	} else log.push(`${PLIST}: not found, skipped`);

	for (const g of generateAll(loadCtx(root))) log.push(`generated ${g}`);

	log.push(
		"",
		"Next:",
		`  1. bun run i18n:translate --locale ${tag}        (needs ANTHROPIC_API_KEY or I18N_BASE_URL; fills i18n/messages/*/${tag}.json)`,
		`  2. put the writing guide of ${tag} in i18n/style/${tag}.md and its fixed terms in i18n/glossary.yaml`,
		`  3. docs: translate docs/<page>.md into docs/${tag}/<page>.md (front matter: title, source, source_sha)`,
		"  4. bun run i18n:fix && bun run i18n:check",
		`  5. a native speaker of ${tag} reviews the PR; promote the tier in ${LOCALES_FILE} to "required" only when ${owner} owns it`,
	);
	return log;
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const tag = args.find((a, i) => !a.startsWith("--") && !["--owner", "--tier", "--root"].includes(args[i - 1] ?? ""));
	const tier = opt("--tier");
	if (!tag || (tier !== undefined && tier !== "community" && tier !== "required")) {
		console.error("usage: bun run i18n:add-locale <tag> [--owner @user] [--tier community|required]");
		process.exit(2);
	}
	try {
		for (const l of addLocale(opt("--root") ?? join(import.meta.dir, "..", ".."), tag, { owner: opt("--owner"), tier })) console.log(l);
	} catch (e) {
		console.error(`i18n:add-locale: ${(e as Error).message}`);
		process.exit(1);
	}
}
