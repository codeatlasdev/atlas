// The ratchets: scanners of existing offenders whose per-file count may only go down (i18n/hardcoded-baseline.json).
// Each one is a definition (where it looks, what a scan returns); `runRatchet` is what the checkers call and
// `scanRatchet` what `i18n:fix` uses to shrink the baseline.
import { existsSync } from "node:fs";
import { join } from "node:path";
import { type Found, type RatchetText, type Violation, ratchet } from "./baseline.ts";
import { PLATFORM, SHORTCUT, scanLines } from "./hardcoded/lines.ts";
import { RUST_DIRS, scanRust } from "./hardcoded/rust.ts";
import { SVELTE_ROOTS, scanSvelte } from "./hardcoded/svelte.ts";
import type { Ctx, Finding } from "./types.ts";
import { docFiles, hitMessage, lintMarkdown, loadRules } from "./writing.ts";
import { readText } from "./util.ts";

export interface RatchetDef extends RatchetText {
	/** Surface name -> directories (or single files) it scans; `--surface <name>` selects one. */
	scopes: { surface: string; dirs: string[] }[];
	scan(ctx: Ctx, dirs: string[]): { found: Found; unparsable?: { file: string; message: string }[] };
}

const SOURCE_EXTS = [".svelte", ".ts", ".astro", ".html", ".rs"];

export const SVELTE: RatchetDef = {
	lang: "svelte",
	rule: "hardcoded",
	noun: "hardcoded user-facing text",
	hint: "use a message key, or `i18n-ignore` if it is not for users",
	scopes: SVELTE_ROOTS.map((r) => ({ surface: r.surface, dirs: [r.dir] })),
	scan: (ctx, dirs) => scanSvelte(ctx.root, dirs),
};

export const RUST: RatchetDef = {
	lang: "rust",
	rule: "hardcoded",
	noun: "hardcoded user-facing text",
	hint: "use a message key, or `// i18n-ignore` if it is not for users",
	scopes: [{ surface: "rust", dirs: RUST_DIRS }],
	scan: (ctx, dirs) => ({ found: scanRust(ctx.root, dirs) }),
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
	scan: (ctx, dirs) => ({ found: scanLines(ctx.root, dirs, SOURCE_EXTS.filter((e) => e !== ".rs"), PLATFORM) }),
};

/** Existing prose (English docs, README, skills, site content, CHANGELOG): error-level hits are counted, not failed. */
export const WRITING: RatchetDef = {
	lang: "writing",
	rule: "writing",
	noun: "flagged writing",
	hint: "rewrite it (i18n/style/README.md), or add `<!-- writing-lint: allow RULE_ID: reason -->` above it",
	scopes: [{ surface: "writing", dirs: ["docs", "skills", "site/src/content", "README.md", "CHANGELOG.md"] }],
	scan(ctx) {
		const { rules } = loadRules(ctx.root);
		const found: Found = new Map();
		const { existing } = docFiles(ctx.root, Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default));
		for (const file of existing) {
			const out: Violation[] = lintMarkdown(rules, readText(join(ctx.root, file)), "en")
				.hits.filter((h) => h.rule.severity === "error")
				.map((h) => ({ line: h.line, text: h.text, detail: hitMessage(h), rule: `writing-${h.rule.id}` }));
			if (out.length) found.set(file, out);
		}
		return { found };
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

/** Ratchet findings of `def` for the run's `--surface`. */
export function runRatchet(ctx: Ctx, def: RatchetDef): Finding[] {
	const { found, prefixes, unparsable } = scanRatchet(ctx, def, ctx.surface);
	if (prefixes.length === 0) return [];
	return [
		...unparsable.map((u): Finding => ({ rule: def.rule, severity: "error", file: u.file, message: `cannot parse for the ${def.lang} scan: ${u.message}` })),
		...ratchet(ctx.root, def, found, prefixes),
	];
}
