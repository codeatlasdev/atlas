// What a surface's source references. Two uses: a key none of the surfaces consuming it reference is dead weight
// (`unused`), and a reference to a key that is not in the tree is a bug the compiler cannot see (`undefined-key`).
//   Paraglide surfaces: `m.key(`, `m["key"]`, `messages_key`, named imports of the generated messages
//   Rust surfaces:      `t!("key")` and `t_in!(tag, "key")`; and any "key" string literal counts as a use (ids passed to helpers, computed ids)
// Dynamic access cannot be seen; annotate it where it happens: `// i18n-keys: settings_tab_*, other_key`.
// A line (or the one above) with `i18n-ignore` is skipped by the undefined-key check.
import { dirname } from "node:path";
import { bracketHead, memberHead, messageBindings } from "./messageModule.ts";
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

/** `// i18n-keys: settings_tab_*, other_key`: the keys a dynamic access may reach; group 1 is the list. */
export const ANNOTATION = /i18n-keys:\s*([^\n]*?)(?:-->|\*\/|$)/gm;

function annotations(text: string, patterns: RegExp[]) {
	for (const m of text.matchAll(ANNOTATION)) {
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
			const { spaces, named } = messageBindings(text);
			const call = (key: string, index: number) => {
				out.names.add(key);
				const line = lineOf(text, index);
				if (spaces.length > 0 && KEY_PATTERN.test(key) && !ignored(lines, line)) out.calls.push({ key, file, line });
			};
			// `m` counts as the namespace even where it is not imported (names only): a use keeps a key alive.
			for (const space of new Set(["m", ...spaces])) {
				for (const m of text.matchAll(new RegExp(`${memberHead(space)}([A-Za-z_]\\w*)`, "g"))) call(m[1], m.index ?? 0);
				for (const m of text.matchAll(new RegExp(`${bracketHead(space)}([^"'\`]+)["'\`]\\s*\\]`, "g"))) call(m[1], m.index ?? 0);
			}
			for (const m of text.matchAll(/\bmessages_([A-Za-z_]\w*)/g)) out.names.add(m[1]);
			for (const n of named) out.names.add(n.imported);
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
			for (const m of text.matchAll(/\b(?:t!\(|t_in!\([^,"]+,)\s*"([^"]+)"/g)) {
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
