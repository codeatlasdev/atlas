// The diff base of the ratchets. A PR is judged on what it changes, not against a snapshot that lags main: for every
// file the change touches, the offenders at the merge-base are counted with the same detector and only the ones the
// change adds fail. Files the change does not touch are not looked at, so main adding screens never needs a
// re-snapshot and an old file's offenders never fail someone else's PR.
//
// The base is `I18N_BASE` (a ref or sha: CI sets it to the PR base or the pushed-from commit), else `origin/main`, else
// `main`; the merge-base of that and HEAD is what the working tree is compared with (committed, staged, unstaged and
// untracked changes alike). With no usable base (not a git checkout, shallow clone, no such ref) there is no diff base
// and the committed baseline (i18n/hardcoded-baseline.json) is compared instead. Fetch before checking locally:
// `git fetch origin main`; a stale origin/main makes main's newer files look like part of your change.
import { realpathSync } from "node:fs";

export interface DiffBase {
	/** The ref the merge-base was taken against. */
	ref: string;
	/** The merge-base commit. */
	mb: string;
	/** Path now -> its path at the merge-base (undefined: the file is new). Deleted files are not listed. */
	changed: Map<string, string | undefined>;
}

function git(root: string, args: string[]): { ok: boolean; out: string } {
	const r = Bun.spawnSync(["git", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
	return { ok: r.exitCode === 0, out: r.stdout.toString() };
}

/** The diff base of the repository at `root`, or undefined when there is none (the root is not a git work tree
 * of its own, or no candidate ref shares history with HEAD). */
export function resolveBase(root: string, env: Record<string, string | undefined> = process.env): DiffBase | undefined {
	const top = git(root, ["rev-parse", "--show-toplevel"]);
	if (!top.ok || realpathSync(top.out.trim()) !== realpathSync(root)) return undefined;
	for (const ref of [env.I18N_BASE?.trim(), "origin/main", "main"]) {
		if (!ref) continue;
		const mb = git(root, ["merge-base", ref, "HEAD"]);
		if (!mb.ok) continue;
		const base = mb.out.trim();
		const changed = new Map<string, string | undefined>();
		for (const line of git(root, ["diff", "--name-status", "-M", "--no-color", base]).out.split("\n")) {
			const [status, a, b] = line.split("\t");
			if (!status || status.startsWith("D")) continue;
			if (status.startsWith("R") || status.startsWith("C")) changed.set(b, a);
			else changed.set(a, status === "A" ? undefined : a);
		}
		for (const f of git(root, ["ls-files", "--others", "--exclude-standard"]).out.split("\n")) if (f) changed.set(f, undefined);
		return { ref, mb: base, changed };
	}
	return undefined;
}

/** `path` as it was at `commit`, or undefined when it did not exist there. */
export function showAt(root: string, commit: string, path: string): string | undefined {
	const r = git(root, ["show", `${commit}:${path}`]);
	return r.ok ? r.out : undefined;
}
