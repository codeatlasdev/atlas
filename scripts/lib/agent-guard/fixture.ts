// Test fixture: a real repository with a primary checkout and two linked worktrees, in a temp dir.
import { mkdirSync, mkdtempSync, realpathSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const GIT_ENV = { GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null", GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@codeatlas.com.br", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@codeatlas.com.br" };

export function git(cwd: string, ...args: string[]): string {
	const r = Bun.spawnSync(["git", ...args], { cwd, env: { ...process.env, ...GIT_ENV } });
	if (r.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr.toString()}`);
	return r.stdout.toString().trim();
}

export interface Repo {
	base: string;
	/** The primary checkout, on branch `main`. */
	main: string;
	/** Linked worktrees on `feat/a` and `feat/b`. */
	a: string;
	b: string;
}

export function makeRepo(): Repo {
	// `.native`: on Windows only it expands 8.3 names (RUNNER~1), which git and the guard report in full.
	const base = realpathSync.native(mkdtempSync(join(tmpdir(), "guard-")));
	const main = join(base, "atlas");
	mkdirSync(join(main, "sub"), { recursive: true });
	git(main, "init", "-q", "-b", "main");
	git(main, "commit", "-q", "--allow-empty", "-m", "base");
	const a = join(base, "atlas-wt-a");
	const b = join(base, "atlas-wt-b");
	git(main, "worktree", "add", "-q", "-b", "feat/a", a);
	git(main, "worktree", "add", "-q", "-b", "feat/b", b);
	return { base, main, a, b };
}

/** A directory symlink (a junction on Windows, which needs no privilege). */
export function dirLink(target: string, link: string): void {
	symlinkSync(target, link, "junction");
}
