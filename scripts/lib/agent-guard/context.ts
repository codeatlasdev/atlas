// What the guard needs to know about the machine: which git checkout the session is in, which other checkouts of the
// same repository exist, and the maintainer's override. `decide.ts` only talks to this interface, so its rules are
// table-tested with plain objects; `realCtx` fills it from git and the environment.
import { spawnSync } from "node:child_process";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { physical } from "./paths";

export interface Tree {
	/** Top level of the checkout the session runs in. */
	root: string;
	/** The session runs in the repository's primary checkout (the one that owns `.git/`), not a linked worktree. */
	primary: boolean;
	/** Top levels of every checkout of this repository, the primary one and the session's own included. */
	all: string[];
	/** The primary checkout (the first entry of `git worktree list`). */
	main: string;
	/** `git config atlas.agentWrites deny`: the maintainer asked to keep agents out of the primary checkout. */
	denyPrimary: boolean;
}

export interface Ctx {
	/** Where the session started. */
	cwd: string;
	home: string;
	/** Directories agents may always write: temp dirs and the harnesses' own state. */
	scratch: string[];
	/** `ATLAS_MAINTAINER_OK` actions the maintainer put in this process's environment. */
	override: ReadonlySet<string>;
	/** `undefined` outside a git repository: nothing to guard. */
	tree(): Tree | undefined;
	/** Real location of `p` (relative to `base`, default the session cwd). */
	physical(p: string, base?: string): string;
	/** Branch checked out in the checkout containing `dir`. */
	branch(dir: string): string | undefined;
}

/** The environment variable a maintainer sets, before launching the agent, to lift a guard. Agents never set it. */
export const OVERRIDE_ENV = "ATLAS_MAINTAINER_OK";

function git(cwd: string, args: string[]): { code: number; out: string } {
	const r = spawnSync("git", args, {
		cwd,
		encoding: "utf8",
		env: { ...process.env, GIT_OPTIONAL_LOCKS: "0", LC_ALL: "C" },
	});
	return { code: r.status ?? 1, out: (r.stdout ?? "").trim() };
}

function readTree(cwd: string, home: string): Tree | undefined {
	const rp = git(cwd, ["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir", "--absolute-git-dir"]);
	if (rp.code !== 0) return undefined;
	const [top, common, gitDir] = rp.out.split("\n");
	if (!top || !common || !gitDir) return undefined;
	const root = physical(top, cwd, home);
	const all = git(cwd, ["worktree", "list", "--porcelain"])
		.out.split("\n")
		.filter((l) => l.startsWith("worktree "))
		.map((l) => physical(l.slice("worktree ".length), cwd, home));
	if (!all.includes(root)) all.push(root);
	return {
		root,
		primary: physical(common, cwd, home) === physical(gitDir, cwd, home),
		all,
		main: all[0] ?? root,
		denyPrimary: git(cwd, ["config", "--get", "atlas.agentWrites"]).out === "deny",
	};
}

/** `ATLAS_MAINTAINER_OK=push,merge` or `all`. */
export function parseOverride(raw: string | undefined): Set<string> {
	return new Set((raw ?? "").split(/[\s,]+/).filter(Boolean));
}

export function realCtx(cwd: string, env: Record<string, string | undefined> = process.env): Ctx {
	const home = env.HOME ?? env.USERPROFILE ?? homedir();
	const scratch = [tmpdir(), "/tmp", "/var/tmp", ...[".claude", ".codex", ".omp", ".atlas", ".agents"].map((d) => join(home, d))].map(
		(d) => physical(d, cwd, home),
	);
	let tree: Tree | undefined | null = null;
	return {
		cwd: physical(cwd, cwd, home),
		home,
		scratch,
		override: parseOverride(env[OVERRIDE_ENV]),
		tree: () => (tree === null ? (tree = readTree(cwd, home)) : tree),
		physical: (p, base) => physical(p, base ?? cwd, home),
		branch: (dir) => git(dir, ["symbolic-ref", "--short", "-q", "HEAD"]).out || undefined,
	};
}

/** Is `ancestor` reachable from `descendant`? An object this clone does not have counts as "no". */
export function isAncestor(cwd: string, ancestor: string, descendant: string): boolean {
	return git(cwd, ["merge-base", "--is-ancestor", ancestor, descendant]).code === 0;
}
