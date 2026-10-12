import type { DiffComment } from "#lib/stores/workspace.svelte.ts";
import type { DiffHunk, FileDiff } from "#lib/api.ts";

/** Single-hunk patch for `git apply --cached [-R]`. */
export function hunkPatch(f: FileDiff, h: DiffHunk): string {
	const from = f.status === "added" || f.status === "untracked" ? "/dev/null" : `a/${f.old_path ?? f.path}`;
	const to = f.status === "deleted" ? "/dev/null" : `b/${f.path}`;
	const body = h.lines
		.map((l) => (l.kind === "add" ? "+" : l.kind === "del" ? "-" : " ") + l.text)
		.join("\n");
	return `diff --git a/${f.old_path ?? f.path} b/${f.path}\n--- ${from}\n+++ ${to}\n${h.header}\n${body}\n`;
}

/** The text an agent receives for a batch of review notes. */
export function reviewPrompt(notes: DiffComment[]): string {
	const lines = notes.map((n) => `- ${n.path}:${n.line} (${n.side}): ${n.text.replace(/\s*\n\s*/g, " ")}`);
	return `Review notes:\n${lines.join("\n")}\n`;
}
