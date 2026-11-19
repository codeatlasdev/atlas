// The diff base: what "changed" is compared with. One resolver for check:changed, the CI plan job and anything else
// that judges a change instead of a snapshot (the i18n ratchets import `resolveBase`, `showAt` and `DiffBase`).
//
//   explicit        `--base <ref>` / ATLAS_BASE (a ref or a sha). A bad one throws: a typo must not fall back to something else.
//   pull_request    CI checks a pull request out as the merge of its branch into the CURRENT main: the base is that
//                   merge's first parent (HEAD^1), the main tip it was built on. The base sha in the event would blame
//                   the pull request for main code merged after it branched.
//   push            the commit the push replaced (`before` in the event file; all zeros for a new branch: no base).
//   origin/main     locally: merge-base with origin/main, else with main. `git fetch origin main` first: a stale
//                   origin/main makes main's newer files look like part of your change.
//
// In CI (GITHUB_ACTIONS=true) nothing else applies: workflow_dispatch and schedule have no base, and no base means
// "everything changed", never "nothing changed". A merge-base is the commit the diff starts at; the diff is that commit
// against the working tree, so committed, staged, unstaged and untracked changes all count.
//
// During an uncommitted merge (MERGE_HEAD exists) HEAD is still the pre-merge tip while the work tree already holds the
// other side. The base is then taken as git will once the merge commit exists: `merge-base <ref> HEAD MERGE_HEAD` (the
// merge base of <ref> and a hypothetical merge of both), so what the merge brings from main is not blamed on the branch.
// A rebase stopped on a conflict needs nothing: its HEAD already sits on the new base.
//
// The root is only ever the cwd of git, never compared with a path git printed (Windows 8.3 short names, drive letters
// and symlinks make those differ for the same directory).
import { readFileSync } from "node:fs";

export type BaseMode = "explicit" | "pull_request" | "push" | "origin/main" | "main";

export interface DiffBase {
	/** The ref (or sha) the merge-base was taken against. */
	ref: string;
	mode: BaseMode;
	/** The merge-base commit. */
	mb: string;
	/** Path now -> its path at the merge-base (undefined: the file is new). Deleted files are not listed. */
	changed: Map<string, string | undefined>;
	/** Every path the change touches, sorted: new and modified files, deleted files and the old side of renames. */
	touched: string[];
}

/** Output lines of a git command: LF or CRLF, no empty ones. */
export const lines = (out: string) => out.split(/\r?\n/).filter(Boolean);

/** `git diff --name-status -M` plus `git ls-files --others` output as the changed-file map and the touched list. */
export function parseChanged(
	nameStatus: string,
	untracked: string,
): { changed: Map<string, string | undefined>; touched: string[] } {
	const changed = new Map<string, string | undefined>();
	const touched = new Set<string>();
	for (const line of lines(nameStatus)) {
		const [status, a, b] = line.split("\t");
		if (!status || !a) continue;
		touched.add(a);
		if (status.startsWith("D")) continue;
		if (status.startsWith("R") || status.startsWith("C")) {
			if (!b) continue;
			touched.add(b);
			changed.set(b, a);
		} else changed.set(a, status === "A" ? undefined : a);
	}
	for (const f of lines(untracked)) {
		changed.set(f, undefined);
		touched.add(f);
	}
	return { changed, touched: [...touched].sort() };
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

const ZEROS = /^0+$/;

interface Candidate {
	mode: BaseMode;
	ref: string;
}

/** Where the base may come from, best first, for this environment. Pure: reads the event file, runs `rev-parse` only
 * to tell a merge commit from a plain one. */
function candidates(root: string, env: Record<string, string | undefined>, explicit?: string): Candidate[] {
	const out: Candidate[] = [];
	const given = (explicit ?? env.ATLAS_BASE)?.trim();
	if (given && !ZEROS.test(given)) out.push({ mode: "explicit", ref: given });
	if (env.GITHUB_ACTIONS === "true") {
		const event = env.GITHUB_EVENT_NAME;
		if (event === "pull_request") {
			const merge = git(root, ["rev-parse", "--verify", "--quiet", "HEAD^2"]);
			const first = git(root, ["rev-parse", "--verify", "--quiet", "HEAD^1"]);
			if (merge.ok && first.ok) out.push({ mode: "pull_request", ref: first.out.trim() });
		} else if (event === "push" && env.GITHUB_EVENT_PATH) {
			try {
				const before = String(JSON.parse(readFileSync(env.GITHUB_EVENT_PATH, "utf8")).before ?? "").trim();
				if (before && !ZEROS.test(before)) out.push({ mode: "push", ref: before });
			} catch {
				// no readable event: no base
			}
		}
		return out;
	}
	out.push({ mode: "origin/main", ref: "origin/main" }, { mode: "main", ref: "main" });
	return out;
}

/** The diff base of the repository at `root`, or undefined when there is none (not the top of a git work tree, shallow
 * history, no candidate shares history with HEAD, a CI event without a base): callers treat that as "everything
 * changed". An explicit base that git cannot resolve throws. */
export function resolveBase(
	root: string,
	env: Record<string, string | undefined> = process.env,
	explicit?: string,
): DiffBase | undefined {
	// Empty at the top of the work tree, `sub/` inside it, a failure outside: no paths to compare.
	const prefix = git(root, ["rev-parse", "--show-prefix"]);
	if (!prefix.ok || prefix.out.trim() !== "") return undefined;
	const merging = git(root, ["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]);
	const tips = merging.ok ? ["HEAD", merging.out.trim()] : ["HEAD"];
	for (const { mode, ref } of candidates(root, env, explicit)) {
		const mb = git(root, ["merge-base", ref, ...tips]);
		if (!mb.ok) {
			if (mode === "explicit") throw new Error(`base \`${ref}\` is not a commit that shares history with HEAD (git merge-base failed)`);
			continue;
		}
		const base = mb.out.trim();
		const diff = git(root, ["diff", "--name-status", "-M", "--no-color", base]);
		const others = git(root, ["ls-files", "--others", "--exclude-standard"]);
		if (!diff.ok || !others.ok) continue;
		return { ref, mode, mb: base, ...parseChanged(diff.out, others.out) };
	}
	return undefined;
}

/** `path` as it was at `commit`, or undefined when it did not exist there. */
export function showAt(root: string, commit: string, path: string): string | undefined {
	const r = git(root, ["show", `${commit}:${path}`]);
	return r.ok ? r.out : undefined;
}
