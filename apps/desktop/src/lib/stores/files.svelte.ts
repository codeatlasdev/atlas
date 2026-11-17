import { listen } from "@tauri-apps/api/event";
import { git } from "#lib/api.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { requestGoto } from "#lib/symbols.ts";

/** Open (or focus) a file tab in the worktree. `path` is relative to the worktree root; `line`/`col` (1-based) place the caret. */
export function openFile(
	worktreeId: string,
	path: string,
	line?: number,
	col?: number,
	end?: { line: number; col: number },
) {
	if (line) requestGoto(`${worktreeId}:${path}`, line, col, end?.line, end?.col);
	ws.reveal(worktreeId, (t) => t.kind === "editor" && t.path === path, {
		kind: "editor",
		label: path.split("/").pop() ?? path,
		path,
	});
}

/** Open a file in the pane to the side of the focused one, keeping focus on the pane you came from. */
export function openFileToSide(worktreeId: string, path: string) {
	const from = ws.focusedGroup(worktreeId).id;
	const side = ws.sideGroup(worktreeId);
	const existing = ws.tabsOf(worktreeId).find((t) => t.kind === "editor" && t.path === path);
	if (existing) ws.activate(existing.id);
	else ws.openTab(worktreeId, { kind: "editor", label: path.split("/").pop() ?? path, path }, side);
	if (!existing) ws.focusGroup(from);
}

export function openDiff(worktreeId: string) {
	ws.reveal(worktreeId, (t) => t.kind === "diff", { kind: "diff", label: "Changes" });
}

/** The backstop for a folder nobody watches; a watched one is refetched as soon as it changes (`touchesFiles`). */
const FILES_TTL_MS = 30_000;
const cache: Record<string, { at: number; files: string[] }> = {};
let listening: Promise<unknown> | undefined;

/** Does an `fs_event` batch change the files under `root`? Siblings that share its prefix and `.git` noise do not. */
export function touchesFiles(root: string, paths: string[] | undefined): boolean {
	if (!root) return false;
	const base = root.replace(/\/+$/, "") + "/";
	return (paths ?? []).some((p) => p.startsWith(base) && !p.startsWith(`${base}.git/`));
}

/** One listener on the host's event stream (the one the Explorer's and Source Control's watchers feed). */
function dropOnChange() {
	listening ??= listen<{ ev?: string; paths?: string[] }>("host://event", (e) => {
		if (e.payload?.ev !== "fs_event") return;
		for (const id of Object.keys(cache)) {
			if (touchesFiles(ws.worktree(id)?.path ?? "", e.payload.paths)) delete cache[id];
		}
	});
}

/**
 * The files of a worktree for Quick Open and `@` mentions: `git ls-files`, or a bounded walk for a folder with no git
 * (`git_files` decides). Cached until the watcher reports a change under the folder, or the TTL.
 */
export async function worktreeFiles(worktreeId: string): Promise<string[]> {
	dropOnChange();
	const c = cache[worktreeId];
	if (c && Date.now() - c.at < FILES_TTL_MS) return c.files;
	const files = await git.files(worktreeId).catch(() => []);
	cache[worktreeId] = { at: Date.now(), files };
	return files;
}
