import type { DiffComment } from "#lib/stores/workspace.svelte.ts";
import type { DiffHunk, FileDiff } from "#lib/api.ts";

const ESCAPES: Record<string, string> = {
	"\n": "\\n",
	"\t": "\\t",
	"\r": "\\r",
	"\x07": "\\a",
	"\b": "\\b",
	"\f": "\\f",
	"\v": "\\v",
	'"': '\\"',
	"\\": "\\\\",
};

/**
 * `a/<path>` or `b/<path>` as a patch header writes it. `git apply` reads a plain name as it is (a letter outside
 * ASCII included, which is why `core.quotepath` is no concern here), but a `"`, a backslash or a control character
 * needs git's own C-quoting around the whole token: `"a/tab\there.txt"`.
 */
function patchName(prefix: "a/" | "b/", path: string): string {
	if (!/["\\\x00-\x1f\x7f]/.test(path)) return prefix + path;
	const body = [...path]
		.map(
			(c) =>
				ESCAPES[c] ?? (c < " " || c === "\x7f" ? `\\${c.charCodeAt(0).toString(8).padStart(3, "0")}` : c),
		)
		.join("");
	return `"${prefix}${body}"`;
}

/** Single-hunk patch for `git apply --cached [-R]`. */
export function hunkPatch(f: FileDiff, h: DiffHunk): string {
	const a = patchName("a/", f.old_path ?? f.path);
	const b = patchName("b/", f.path);
	const from = f.status === "added" || f.status === "untracked" ? "/dev/null" : a;
	const to = f.status === "deleted" ? "/dev/null" : b;
	const body = h.lines
		.map((l) => (l.kind === "add" ? "+" : l.kind === "del" ? "-" : " ") + l.text)
		.join("\n");
	return `diff --git ${a} ${b}\n--- ${from}\n+++ ${to}\n${h.header}\n${body}\n`;
}

/** The text an agent receives for a batch of review notes. */
export function reviewPrompt(notes: DiffComment[]): string {
	const lines = notes.map((n) => `- ${n.path}:${n.line} (${n.side}): ${n.text.replace(/\s*\n\s*/g, " ")}`);
	return `Review notes:\n${lines.join("\n")}\n`;
}
