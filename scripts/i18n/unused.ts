// What a surface's source references. Two uses: a key none of the surfaces consuming it reference is dead weight
// (`unused`), and a reference to a key that is not in the tree is a bug the compiler cannot see (`undefined-key`).
//   Paraglide surfaces: `m.key(`, `m["key"]`, `messages_key`, named imports of the generated messages
//   Rust surfaces:      `t!("key")`; and any "key" string literal counts as a use (ids passed to helpers, computed ids)
// Dynamic access cannot be seen; annotate it where it happens: `// i18n-keys: settings_tab_*, other_key`.
// A line (or the one above) with `i18n-ignore` is skipped by the undefined-key check.
import { dirname } from "node:path";
import { isTestFile, withoutTestTail } from "./hardcoded/rust.ts";
import { KEY_PATTERN, type SurfaceDef } from "./source.ts";
import { globToRegExp, lineOf, readText, rel, walk } from "./util.ts";

export interface Reference {
	key: string;
	file: string;
	line: number;
}

export interface Refs {
	/** Every use of a key, call or literal: what keeps a key alive. */
	names: Set<string>;
	patterns: RegExp[];
	/** Calls that name a message (`m.key(`, `t!("key")`): what must exist. */
	calls: Reference[];
}

const WEB = /\.(?:svelte|ts|tsx|js|jsx|mjs|astro)$/;
/** `import * as m from ".../paraglide/messages"` or `import { m } from ...`: the file's `m` is the messages module. */
const USES_M = /import\s+(?:\*\s+as\s+m|\{[^}]*\bm\b[^}]*\})\s+from\s+["'][^"']*paraglide\/messages/;

function annotations(text: string, patterns: RegExp[]) {
	for (const m of text.matchAll(/i18n-keys:\s*([^\n]*?)(?:-->|\*\/|$)/gm)) {
		for (const g of m[1].split(/[\s,]+/).filter(Boolean)) patterns.push(globToRegExp(g));
	}
}

const ignored = (lines: string[], line: number) => /i18n-ignore/.test(lines[line - 1] ?? "") || /i18n-ignore/.test(lines[line - 2] ?? "");

/** What the sources of `def` reference, or undefined for a surface kind with no source scanner (arb). */
export function scanRefs(root: string, def: SurfaceDef): Refs | undefined {
	const out: Refs = { names: new Set(), patterns: [], calls: [] };
	if (def.paraglide) {
		for (const path of walk(`${root}/${dirname(def.paraglide)}/src`, (p) => WEB.test(p) && !/[\\/]paraglide[\\/]/.test(p))) {
			const text = readText(path);
			const file = rel(root, path);
			const lines = text.split("\n");
			const usesM = USES_M.test(text);
			const call = (key: string, index: number) => {
				out.names.add(key);
				const line = lineOf(text, index);
				if (usesM && KEY_PATTERN.test(key) && !ignored(lines, line)) out.calls.push({ key, file, line });
			};
			for (const m of text.matchAll(/\bm\.([A-Za-z_]\w*)/g)) call(m[1], m.index ?? 0);
			for (const m of text.matchAll(/\bm\[\s*["'`]([^"'`]+)["'`]\s*\]/g)) call(m[1], m.index ?? 0);
			for (const m of text.matchAll(/\bmessages_([A-Za-z_]\w*)/g)) out.names.add(m[1]);
			for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*["'][^"']*paraglide\/messages[^"']*["']/g)) {
				for (const part of m[1].split(",")) {
					const name = part.trim().split(/\s+as\s+/)[0];
					if (name && name !== "m") out.names.add(name);
				}
			}
			annotations(text, out.patterns);
		}
		return out;
	}
	if (def.rust) {
		for (const path of walk(`${root}/${def.rust}`, (p) => p.endsWith(".rs"))) {
			const raw = readText(path);
			const file = rel(root, path);
			for (const m of raw.matchAll(/"([a-z][a-z0-9_]*)"/g)) out.names.add(m[1]);
			annotations(raw, out.patterns);
			if (isTestFile(file)) continue;
			const text = withoutTestTail(raw);
			const lines = text.split("\n");
			for (const m of text.matchAll(/\bt!\(\s*"([^"]+)"/g)) {
				const line = lineOf(text, m.index ?? 0);
				const before = (lines[line - 1] ?? "").slice(0, (m.index ?? 0) - text.lastIndexOf("\n", (m.index ?? 0) - 1) - 1);
				if (!before.includes("//") && !ignored(lines, line)) out.calls.push({ key: m[1], file, line });
			}
		}
		return out;
	}
	return undefined;
}

export const isUsed = (refs: Refs, key: string) => refs.names.has(key) || refs.patterns.some((p) => p.test(key));
