/**
 * What Settings ▸ Workspaces shows under "Worktree folder": where a new worktree is created and what its branch is
 * called, from the same three settings the app reads. Mirrors `crates/atlas-desktop/src/worktree/create.rs` and
 * `naming.rs` (`slug`); the git user name is only known to git, so it is shown as a placeholder.
 */

const DEFAULT_DIR = "~/.atlas/worktrees";
const EXAMPLE_REPO = "atlas";
const EXAMPLE_NAME = "fix-login";

export interface WorktreePrefs {
	/** `workspaceDir` as typed; empty means the default folder. */
	dir: string;
	/** `nestWorkspaces` is not "false". */
	nest: boolean;
	/** `branchPrefix`: `git-username`, `custom` or `none`; anything else behaves like `git-username`. */
	prefix: string;
	/** `branchPrefixCustom`. */
	custom: string;
}

/** `naming::slug`: lowercase ASCII letters and digits, any other run becomes one `-`, trimmed, at most 40 chars. */
export function slug(s: string): string {
	const dashed = s
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
	return dashed.slice(0, 40).replace(/-+$/, "");
}

export function worktreePreview(p: WorktreePrefs): { path: string[]; branch: string } {
	const dir = p.dir.trim().replace(/\/+$/, "") || DEFAULT_DIR;
	const prefix = p.prefix === "none" ? "" : p.prefix === "custom" ? slug(p.custom) : "<your-git-name>";
	return {
		path: p.nest ? [dir, EXAMPLE_REPO, EXAMPLE_NAME] : [dir, EXAMPLE_NAME],
		branch: prefix ? `${prefix}/${EXAMPLE_NAME}` : EXAMPLE_NAME,
	};
}
