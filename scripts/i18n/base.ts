// The diff base of the ratchets. A PR is judged on what it changes, not against a snapshot that lags main: for every
// file the change touches, the offenders at the merge-base are counted with the same detector and only the ones the
// change adds fail. Files the change does not touch are not looked at, so main adding screens never needs a
// re-snapshot and an old file's offenders never fail someone else's PR.
//
// The base is `I18N_BASE` (a ref or sha: CI sets it to `HEAD^1` on a pull request, the main tip its merge ref was
// built on, and to the commit a push replaced), else `origin/main`, else
// `main`; the merge-base of that and HEAD is what the working tree is compared with (committed, staged, unstaged and
// untracked changes alike). With no usable base (not a git checkout, shallow clone, no such ref) there is no diff base
// and the committed baseline (i18n/hardcoded-baseline.json) is compared instead. Fetch before checking locally:
// `git fetch origin main`; a stale origin/main makes main's newer files look like part of your change.
export interface DiffBase {
	/** The ref the merge-base was taken against. */
	ref: string;
	/** The merge-base commit. */
	mb: string;
	/** Path now -> its path at the merge-base (undefined: the file is new). Deleted files are not listed. */
	changed: Map<string, string | undefined>;
}

/** Output lines of a git command: LF or CRLF, no empty ones. */
export const lines = (out: string) => out.split(/\r?\n/).filter(Boolean);

/** `git diff --name-status -M` plus `git ls-files --others` output as the changed-file map of a DiffBase. */
export function parseChanged(nameStatus: string, untracked: string): Map<string, string | undefined> {
	const changed = new Map<string, string | undefined>();
	for (const line of lines(nameStatus)) {
		const [status, a, b] = line.split("\t");
		if (!status || status.startsWith("D")) continue;
		if (status.startsWith("R") || status.startsWith("C")) changed.set(b, a);
		else changed.set(a, status === "A" ? undefined : a);
	}
	for (const f of lines(untracked)) changed.set(f, undefined);
	return changed;
}

/** git with an argument array (no shell), never throwing: a missing git or a failing command is `ok: false`. */
function git(root: string, args: string[]): { ok: boolean; out: string } {
	try {
		const r = Bun.spawnSync(["git", "-c", "core.quotepath=false", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
		return { ok: r.exitCode === 0, out: r.stdout.toString() };
	} catch {
		return { ok: false, out: "" };
	}
}

/** The diff base of the repository at `root`, or undefined when there is none (the root is not the top of a git work
 * tree, or no candidate ref shares history with HEAD). `I18N_BASE` may be a ref or a sha; an unknown one (a new branch
 * push has an all-zero `before`) is skipped like any other candidate.
 *
 * In CI a pull request is checked out as the merge of its branch into the current main: pass `I18N_BASE=HEAD^1`
 * (resolved to a sha), the exact main tip GitHub merged with. The base sha recorded in the event would make main
 * code merged after the PR branched look like the PR's. */
export function resolveBase(root: string, env: Record<string, string | undefined> = process.env): DiffBase | undefined {
	// Empty at the top of the work tree, `sub/` inside it, a failure outside: no paths to compare, so no 8.3 short
	// names, drive letters or symlinks to get wrong on Windows.
	const prefix = git(root, ["rev-parse", "--show-prefix"]);
	if (!prefix.ok || prefix.out.trim() !== "") return undefined;
	for (const ref of [env.I18N_BASE?.trim(), "origin/main", "main"]) {
		if (!ref) continue;
		const mb = git(root, ["merge-base", ref, "HEAD"]);
		if (!mb.ok) continue;
		const base = mb.out.trim();
		const diff = git(root, ["diff", "--name-status", "-M", "--no-color", base]);
		const others = git(root, ["ls-files", "--others", "--exclude-standard"]);
		if (!diff.ok || !others.ok) continue;
		return { ref, mb: base, changed: parseChanged(diff.out, others.out) };
	}
	return undefined;
}

/** `path` as it was at `commit`, or undefined when it did not exist there. */
export function showAt(root: string, commit: string, path: string): string | undefined {
	const r = git(root, ["show", `${commit}:${path}`]);
	return r.ok ? r.out : undefined;
}
