import { listen } from "@tauri-apps/api/event";
import { git } from "#lib/api.ts";
import { createGitStatusFeed } from "./gitStatusFeed.ts";

/** The app's git status per worktree: debounced, single-flight, shared by Source Control and the Explorer (#307). */
export const gitStatus = createGitStatusFeed({
	fetch: git.status,
	listen,
	delay: 250,
	maxWait: 2000,
	now: () => Date.now(),
	setTimeout: (fn, ms) => window.setTimeout(fn, ms),
	clearTimeout: (id) => window.clearTimeout(id as number),
});
