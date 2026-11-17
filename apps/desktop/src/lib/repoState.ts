/**
 * One model of "where is this worktree on its way to a pull request" (#506). The Pull request tab used to know two
 * things, "there is a PR" and "there is not", so a folder that is not a repository, a repository with no remote and a
 * branch that was never pushed all read "No pull request for this branch" with a Create button that could only fail.
 *
 * `repoState` derives the state from facts the app already has or gets in one go:
 *  - git: `probeRepo` (`git remote -v`, `rev-parse HEAD`, `symbolic-ref origin/HEAD`) runs with the PR lookup, which
 *    already refreshes every minute while the tab is focused; branch and upstream come from the shared git status
 *    feed Source Control polls;
 *  - gh: `gh auth status` and `gh pr view` (their real exit codes and messages are classified here).
 *
 * Pure: no Svelte, no IPC; the runner is passed in so the tests drive real git and real recorded `gh` output.
 */
import type { PrView } from "#lib/stores/pr.svelte.ts";

export interface Out {
	code: number | null;
	stdout: string;
	stderr: string;
}
/** Runs `argv` in the worktree; rejects when the program cannot be started (gh is not installed). */
export type Run = (argv: string[]) => Promise<Out>;

export interface Remote {
	name: string;
	url: string;
}

export interface RepoProbe {
	notRepo: boolean;
	hasCommits: boolean;
	remotes: Remote[];
	/** `main` for `origin/HEAD` = `origin/main`; null when the remote has not told (no `origin/HEAD`). */
	defaultBranch: string | null;
}

export type GhAuth = "ok" | "signed-out" | "not-installed" | "unknown";

/** What `gh pr view` said about the branch. */
export type GhLookup =
	| { kind: "pr"; pr: PrView }
	| { kind: "none" }
	| { kind: "not-github" }
	| { kind: "signed-out" }
	| { kind: "error"; message: string };

export type RepoState =
	| { kind: "not-repo" }
	| { kind: "no-commits" }
	| { kind: "no-remote" }
	| { kind: "remote-not-github"; remote: Remote | null }
	| { kind: "gh-missing" }
	| { kind: "gh-signed-out" }
	| { kind: "gh-error"; message: string }
	| { kind: "default-branch"; branch: string }
	| { kind: "not-pushed"; branch: string; remote: string }
	| { kind: "ready"; branch: string | null }
	| { kind: "pr"; pr: PrView };

/** `git remote -v`: one fetch line per remote (a remote can have a different push URL; the fetch one is its address). */
export function parseRemotes(out: string): Remote[] {
	const seen = new Map<string, Remote>();
	for (const line of out.split("\n")) {
		const m = line.match(/^(\S+)\t(\S+) \(fetch\)$/);
		if (m && !seen.has(m[1])) seen.set(m[1], { name: m[1], url: m[2] });
	}
	return [...seen.values()];
}

/** Host of an https/ssh/scp-like git URL (`git@github.com:o/r.git`, `ssh://git@host:22/o/r`), lower case; "" if none. */
export function remoteHost(url: string): string {
	const m =
		url.match(/^[a-z+.-]+:\/\/(?:[^@/]*@)?([^/:]+)/i) ?? url.match(/^(?:[^@/\s]+@)?([^:/\s]+):(?!\/\/)/);
	return (m?.[1] ?? "").toLowerCase();
}

/** Forges that are certainly not GitHub (and not a GitHub Enterprise host); gh decides for every other host. */
const OTHER_FORGE = /(^|\.)(gitlab|bitbucket|codeberg|gitea|sr\.ht|azure|visualstudio)\b/;
export const isOtherForge = (url: string): boolean => OTHER_FORGE.test(remoteHost(url));

/** `origin/HEAD` as `refs/remotes/origin/HEAD` short name (`origin/main`) → `main`. */
export function parseDefaultBranch(out: string): string | null {
	const short = out.trim();
	const slash = short.indexOf("/");
	return short && slash > 0 ? short.slice(slash + 1) : null;
}

export async function probeRepo(run: Run): Promise<RepoProbe> {
	// `remote -v` fails exactly when the folder is not a repository: one process answers the common empty case.
	const remote = await run(["git", "remote", "-v"]);
	if (remote.code !== 0) {
		if (/not a git repository/i.test(remote.stderr)) {
			return { notRepo: true, hasCommits: false, remotes: [], defaultBranch: null };
		}
		throw new Error(remote.stderr.trim() || "git remote failed");
	}
	const [head, origin] = await Promise.all([
		run(["git", "rev-parse", "--verify", "-q", "HEAD"]),
		run(["git", "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]),
	]);
	return {
		notRepo: false,
		hasCommits: head.code === 0,
		remotes: parseRemotes(remote.stdout),
		defaultBranch: origin.code === 0 ? parseDefaultBranch(origin.stdout) : null,
	};
}

/** hostd's answer when the program does not exist: `gh: No such file or directory (os error 2)` (Windows: "cannot find the file"). */
const NO_PROGRAM = /os error 2\b|no such file|cannot find the (?:file|path)|program not found/i;

/** `gh auth status`; a missing binary is a rejection from the runner, not an exit code. Any other rejection (hostd down) is unknown. */
export async function ghAuth(run: Run): Promise<GhAuth> {
	try {
		const r = await run(["gh", "auth", "status"]);
		return r.code === 0 ? "ok" : "signed-out";
	} catch (e) {
		return NO_PROGRAM.test(e instanceof Error ? e.message : String(e)) ? "not-installed" : "unknown";
	}
}

/** `gh pr view --json …` exit code and message → what it says about the branch. */
export function classifyGhView(r: Out): GhLookup {
	if (r.code === 0) return { kind: "pr", pr: JSON.parse(r.stdout) as PrView };
	const text = (r.stderr || r.stdout).trim();
	if (/no pull requests found/i.test(text)) return { kind: "none" };
	if (/known GitHub host/i.test(text)) return { kind: "not-github" };
	// Exit code 4 is gh's "authentication required"; signed out it asks for GH_TOKEN, as it would in a script.
	if (r.code === 4 || /not logged in|gh auth login|GH_TOKEN|HTTP 401|bad credentials/i.test(text)) {
		return { kind: "signed-out" };
	}
	return { kind: "error", message: text || "gh pr view failed" };
}

export interface RepoFacts {
	/** Null until the first probe is back. */
	probe: RepoProbe | null;
	auth: GhAuth;
	/** From the git status feed: the current branch (null while detached or unknown) and its upstream. */
	branch: string | null;
	upstream: string | null;
	/** Null when gh was not asked (nothing to ask about, or gh not usable). */
	lookup: GhLookup | null;
}

const FALLBACK_DEFAULTS = ["main", "master"];

/** The state, or null while there is nothing to go on yet. Precedence is the order the user must fix things in. */
export function repoState(f: RepoFacts): RepoState | null {
	const p = f.probe;
	if (!p) return null;
	if (p.notRepo) return { kind: "not-repo" };
	if (!p.hasCommits) return { kind: "no-commits" };
	if (!p.remotes.length) return { kind: "no-remote" };
	const other = p.remotes.find((r) => isOtherForge(r.url));
	if (f.lookup?.kind === "not-github" || (other && p.remotes.every((r) => isOtherForge(r.url)))) {
		return { kind: "remote-not-github", remote: other ?? p.remotes[0] };
	}
	if (f.auth === "not-installed") return { kind: "gh-missing" };
	if (f.auth === "signed-out" || f.lookup?.kind === "signed-out") return { kind: "gh-signed-out" };
	const l = f.lookup;
	if (l?.kind === "pr") return { kind: "pr", pr: l.pr };
	if (l?.kind === "error") return { kind: "gh-error", message: l.message };
	if (f.auth !== "ok" || !l) return null;
	const branch = f.branch;
	if (branch && (branch === p.defaultBranch || (!p.defaultBranch && FALLBACK_DEFAULTS.includes(branch)))) {
		return { kind: "default-branch", branch };
	}
	if (branch && !f.upstream) {
		return {
			kind: "not-pushed",
			branch,
			remote: p.remotes.find((r) => r.name === "origin")?.name ?? p.remotes[0].name,
		};
	}
	return { kind: "ready", branch };
}

export type RepoAction =
	"add-remote" | "install-gh" | "sign-in" | "create-branch" | "push" | "create-pr" | "retry";

export interface StateText {
	title: string;
	description: string;
	/** The one next step; null when the only step is to commit in Source Control. */
	action: { id: RepoAction; label: string } | null;
}

/** Why there is no pull request to show, and the next step. Every state but `pr` and `not-repo` (its own panel). */
export function describeState(s: Exclude<RepoState, { kind: "pr" } | { kind: "not-repo" }>): StateText {
	switch (s.kind) {
		case "no-commits":
			return {
				title: "No commits yet",
				description:
					"A pull request is made of commits. Commit your changes in Source Control, then come back.",
				action: null,
			};
		case "no-remote":
			return {
				title: "This repository has no remote",
				description: "Add the address of the repository on GitHub, then push your branch to it.",
				action: { id: "add-remote", label: "Add a remote" },
			};
		case "remote-not-github":
			return {
				title: "The remote is not on GitHub",
				description: s.remote
					? `${s.remote.name} points to ${s.remote.url}. Pull requests here need a GitHub remote.`
					: "Pull requests here need a GitHub remote.",
				action: { id: "add-remote", label: "Add a remote" },
			};
		case "gh-missing":
			return {
				title: "GitHub CLI is not installed",
				description: "Atlas uses the GitHub CLI (gh) to read and create pull requests.",
				action: { id: "install-gh", label: "Install GitHub CLI" },
			};
		case "gh-signed-out":
			return {
				title: "Sign in to GitHub",
				description: "The GitHub CLI is not signed in. Sign in once and pull requests show up here.",
				action: { id: "sign-in", label: "Sign in to GitHub" },
			};
		case "gh-error":
			return {
				title: "GitHub did not answer",
				description: s.message,
				action: { id: "retry", label: "Try again" },
			};
		case "default-branch":
			return {
				title: `You are on ${s.branch}, the default branch`,
				description: "A pull request merges one branch into another. Create a branch for your work first.",
				action: { id: "create-branch", label: "Create branch" },
			};
		case "not-pushed":
			return {
				title: `${s.branch} is not pushed yet`,
				description: `GitHub does not know this branch. Push it to ${s.remote}, then open a pull request.`,
				action: { id: "push", label: "Push branch" },
			};
		case "ready":
			return {
				title: "No pull request for this branch",
				description: "Open one to get this branch reviewed and merged.",
				action: { id: "create-pr", label: "Create pull request" },
			};
	}
}
