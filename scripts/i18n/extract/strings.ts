// The part of `i18n:extract` that never converts: strings in TypeScript code. A `toast.error("...")` or a `label: "..."`
// in a `.ts` module or a component's `<script>` is user-facing text the Svelte ratchet does not see and a codemod cannot
// place safely (it needs the right call site, a reactive scope and often a computed variable). They are listed with
// file:line and the reason, so the agent migrating the file by hand sees them. A heuristic, on purpose: false positives
// cost one look, and the list says "heuristic".
import { lineOf } from "../util.ts";
import type { Refusal } from "./svelte.ts";

const CALL = /\b(?:toast(?:\.\w+)?|alert|confirm|notify)\(\s*(["'`])((?:\\.|(?!\1)[^\\\n])*)\1/g;
const PROP = /\b(?:label|title|description|placeholder|hint|tooltip|message|text|heading|subtitle|caption)\s*:\s*(["'`])((?:\\.|(?!\1)[^\\\n])*)\1/g;
const PROSE = /\p{L}{2}/u;

/** Strings in code a person may read, as refusals: `ts-module` for a `.ts` file, `script-string` inside a component. */
export function scriptStrings(source: string, reason: "ts-module" | "script-string"): Refusal[] {
	const lines = source.split("\n");
	const out: Refusal[] = [];
	for (const re of [CALL, PROP]) {
		for (const m of source.matchAll(re)) {
			const line = lineOf(source, m.index ?? 0);
			const text = m[2];
			const here = lines[line - 1] ?? "";
			if (/^\s*(?:\/\/|\/\*|\*)/.test(here) || /i18n-ignore/.test(here) || /i18n-ignore/.test(lines[line - 2] ?? "")) continue;
			if (!PROSE.test(text) || !(/\s/.test(text.trim()) || /^\p{Lu}/u.test(text))) continue;
			out.push({
				line,
				reason,
				text: text.trim().slice(0, 80),
				covers: 0,
				detail: `${m[1] === "`" && text.includes("${") ? "template literal: build it as a message with variables. " : ""}heuristic: if a person reads this text, add a message (bun run i18n:add) and call m.area_name() where it runs; if not, add // i18n-ignore`,
			});
		}
	}
	return out.sort((a, b) => a.line - b.line);
}
