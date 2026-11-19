// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeAll, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { classifyGitError } from "./gitErrors.ts";
import { m } from "#lib/i18n/m.ts";

// Incident (#505): in a folder that is not a git repository (~/.omp) the app raised an error toast with the whole
// output of `git diff --no-color -U3 …`: "warning: Not a git repository. Use --no-index to compare two paths outside a
// working tree" followed by git's usage text. Every input here is what the REAL git prints, produced in throwaway
// folders with the global config isolated, in the shape hostd reports it (`git <args>: <stderr>`).
let root;
let globalConfig;
const decoder = new TextDecoder();
const env = () => ({
	PATH: process.env.PATH,
	HOME: root,
	GIT_CONFIG_GLOBAL: globalConfig,
	GIT_CONFIG_NOSYSTEM: "1",
	GIT_TERMINAL_PROMPT: "0",
	LC_ALL: "C",
});
const git = (dir, args) => {
	const p = Bun.spawnSync(["git", "-C", dir, ...args], { env: env(), stdin: "ignore" });
	return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
};
/** The error as the app shows it: `worktree/exec.rs::git_on` reports stderr; the commit box shows `stderr || stdout`. */
const hostError = (dir, args) => {
	const r = git(dir, args);
	expect(r.code).not.toBe(0);
	return `git ${args.join(" ")}: ${r.stderr.trim() || r.stdout.trim()}`;
};

let n = 0;
const folder = () => {
	const d = join(root, `d${n++}`);
	mkdirSync(d);
	return d;
};
const repo = (commits = true) => {
	const d = folder();
	git(d, ["init", "-q", "-b", "main"]);
	if (commits) git(d, ["commit", "-q", "--allow-empty", "-m", "init"]);
	return d;
};

beforeAll(() => {
	root = mkdtempSync(join(tmpdir(), "atlas-giterr-"));
	globalConfig = join(root, "global.gitconfig");
	writeFileSync(globalConfig, "[user]\n\tname = Ada\n\temail = ada@example.com\n");
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

const REVIEW_DIFF = [
	"diff",
	"--no-color",
	"-U3",
	"--no-ext-diff",
	"-M",
	"--src-prefix=a/",
	"--dst-prefix=b/",
];

test("git diff outside a repository (#505) is one sentence, and the usage text stays in the details", () => {
	const raw = hostError(folder(), REVIEW_DIFF);
	expect(raw).toContain("usage: git diff --no-index"); // the incident: this is what reached the toast
	const p = classifyGitError(raw);
	expect(p.kind).toBe("not-repo");
	expect(p.message).toBe(m.git_errors_not_repo());
	expect(p.details).toContain("Not a git repository. Use --no-index");
	expect(p.details).not.toContain("git diff --no-color"); // the host's argument prefix is dropped
});

test("git status outside a repository is the same sentence (the other wording git uses)", () => {
	const raw = hostError(folder(), ["status", "--porcelain=v2", "-z", "--branch"]);
	expect(raw).toContain("fatal: not a git repository");
	expect(classifyGitError(raw)).toMatchObject({
		kind: "not-repo",
		message: m.git_errors_not_repo(),
	});
});

/** The catalog sentence of each kind a case below expects. */
const MESSAGES = {
	"not-repo": () => m.git_errors_not_repo(),
	"no-commits": () => m.git_errors_no_commits(),
	"no-remote": () => m.git_errors_no_remote(),
	"no-upstream": () => m.git_errors_no_upstream(),
	"remote-access": () => m.git_errors_remote_access(),
	rejected: () => m.git_errors_rejected(),
	conflict: () => m.git_errors_conflict(),
	"local-changes": () => m.git_errors_local_changes(),
	"nothing-to-commit": () => m.git_errors_nothing_to_commit(),
	locked: () => m.git_errors_locked(),
};

const CASES = [
	["log in a repository without commits", () => [repo(false), ["log"]], "no-commits"],
	["diff HEAD in a repository without commits", () => [repo(false), ["diff", "HEAD"]], "no-commits"],
	["push with no remote", () => [repo(), ["push"]], "no-remote"],
	[
		"push of a branch that has no upstream",
		() => {
			const d = repo();
			const bare = folder();
			git(bare, ["init", "-q", "--bare"]);
			git(d, ["remote", "add", "origin", bare]);
			return [d, ["push"]];
		},
		"no-upstream",
	],
	["pull with no tracking branch", () => [repo(), ["pull", "--rebase"]], "no-upstream"],
	[
		"push to a remote that does not exist",
		() => {
			const d = repo();
			git(d, ["remote", "add", "origin", join(root, "nowhere")]);
			return [d, ["push", "origin", "main"]];
		},
		"remote-access",
	],
	[
		"push of a commit-less branch to a remote",
		() => {
			const d = repo(false);
			const bare = folder();
			git(bare, ["init", "-q", "--bare"]);
			git(d, ["remote", "add", "origin", bare]);
			return [d, ["push", "origin", "main"]];
		},
		"no-commits",
	],
	[
		"push that the remote has moved past",
		() => {
			const d = repo();
			const bare = folder();
			git(bare, ["init", "-q", "--bare"]);
			git(d, ["remote", "add", "origin", bare]);
			git(d, ["push", "-q", "-u", "origin", "main"]);
			const other = join(root, `clone${n++}`);
			git(root, ["clone", "-q", "-b", "main", bare, other]);
			git(other, ["commit", "-q", "--allow-empty", "-m", "elsewhere"]);
			git(other, ["push", "-q"]);
			git(d, ["commit", "-q", "--allow-empty", "-m", "here"]);
			return [d, ["push"]];
		},
		"rejected",
	],
	[
		"merge that conflicts",
		() => {
			const d = repo();
			writeFileSync(join(d, "f.txt"), "base\n");
			git(d, ["add", "."]);
			git(d, ["commit", "-q", "-m", "base"]);
			git(d, ["checkout", "-q", "-b", "other"]);
			writeFileSync(join(d, "f.txt"), "other\n");
			git(d, ["commit", "-q", "-am", "other"]);
			git(d, ["checkout", "-q", "main"]);
			writeFileSync(join(d, "f.txt"), "main\n");
			git(d, ["commit", "-q", "-am", "main"]);
			return [d, ["merge", "other"]];
		},
		"conflict",
	],
	[
		"checkout that would overwrite local changes",
		() => {
			const d = repo();
			writeFileSync(join(d, "f.txt"), "base\n");
			git(d, ["add", "."]);
			git(d, ["commit", "-q", "-m", "base"]);
			git(d, ["checkout", "-q", "-b", "other"]);
			writeFileSync(join(d, "f.txt"), "other\n");
			git(d, ["commit", "-q", "-am", "other"]);
			git(d, ["checkout", "-q", "main"]);
			writeFileSync(join(d, "f.txt"), "uncommitted\n");
			return [d, ["checkout", "other"]];
		},
		"local-changes",
	],
	["commit with nothing to commit", () => [repo(), ["commit", "-m", "x"]], "nothing-to-commit"],
	[
		"add while another git process holds the index lock",
		() => {
			const d = repo();
			writeFileSync(join(d, ".git", "index.lock"), "");
			writeFileSync(join(d, "f.txt"), "x\n");
			return [d, ["add", "."]];
		},
		"locked",
	],
];
for (const [name, make, kind] of CASES) {
	test(`${name} is classified as ${kind}, never as raw text`, () => {
		const [dir, args] = make();
		const p = classifyGitError(hostError(dir, args));
		expect(p.kind).toBe(kind);
		expect(p.message).not.toContain("\n");
		expect(p.message).toMatch(/^[A-Z].*\.$/);
		expect(p.message).toBe(MESSAGES[kind]());
		expect(p.details).not.toBe("");
	});
}

test("an error git has no sentence for says so, keeps its first line, drops the usage block, and keeps the rest as details", () => {
	const raw = hostError(repo(), ["commit", "--bogus-flag"]);
	expect(raw).toContain("usage: git commit");
	const p = classifyGitError(raw);
	expect(p.kind).toBe("other");
	expect(p.message).toBe(m.git_errors_other_line({ line: "unknown option `bogus-flag'" }));
	expect(p.details).toContain("usage: git commit");
});

test("a one-line unknown error has nothing to hide behind Details", () => {
	expect(classifyGitError("hook says no")).toEqual({
		kind: "other",
		message: m.git_errors_other_line({ line: "hook says no" }),
		details: "",
	});
});

test("no kind's message is ever longer than a sentence, whatever git printed", () => {
	const long = `git foo: error: ${"x".repeat(900)}\nusage: git foo\n\n    more usage`;
	const p = classifyGitError(long);
	expect(p.message.length).toBeLessThanOrEqual(260);
	expect(p.message).not.toContain("usage");
});

test("empty output still gives a message", () => {
	expect(classifyGitError("").message).toBe(m.git_errors_other());
});
