import { git } from "#lib/api.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { requestGoto } from "#lib/symbols.ts";

/** Open (or focus) a file tab in the worktree. `path` is relative to the worktree root; `line`/`col` (1-based) place the caret. */
export function openFile(worktreeId: string, path: string, line?: number, col?: number) {
	if (line) requestGoto(`${worktreeId}:${path}`, line, col);
	ws.reveal(worktreeId, (t) => t.kind === "editor" && t.path === path, {
		kind: "editor",
		label: path.split("/").pop() ?? path,
		path,
	});
}

export function openDiff(worktreeId: string) {
	ws.reveal(worktreeId, (t) => t.kind === "diff", { kind: "diff", label: "Changes" });
}

const FILES_TTL_MS = 5_000;
const cache: Record<string, { at: number; files: string[] }> = {};

/** `git ls-files` for Quick Open, cached for a few seconds. */
export async function worktreeFiles(worktreeId: string): Promise<string[]> {
	const c = cache[worktreeId];
	if (c && Date.now() - c.at < FILES_TTL_MS) return c.files;
	const files = await git.files(worktreeId).catch(() => []);
	cache[worktreeId] = { at: Date.now(), files };
	return files;
}
