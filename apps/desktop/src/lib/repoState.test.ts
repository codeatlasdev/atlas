// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeAll, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
	classifyGhView,
	describeState,
	ghAuth,
	isOtherForge,
	parseDefaultBranch,
	parseRemotes,
	probeRepo,
	remoteHost,
	repoState,
} from "./repoState.ts";

// Incident (#506): the Pull request tab knew two states, "a PR" and "no PR", so a folder that is not a repository, a
// repository with no remote and a branch that was never pushed all said "No pull request for this branch" with a
// Create button that could only fail. git facts below come from REAL git in throwaway folders; gh answers are real
// output recorded from gh itself (src/lib/__fixtures__/gh/*.json, each names the command and the situation).
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
/** A `Run` for the folder `dir`, like `host.exec(dir, argv)`; a missing program rejects as hostd does. */
const runIn = (dir) => async (argv) => {
	const p = Bun.spawnSync(argv, { cwd: dir, env: env(), stdin: "ignore" });
	return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
};
const git = (dir, args) => runIn(dir)(["git", ...args]);

let n = 0;
const folder = () => {
	const d = join(root, `d${n++}`);
	mkdirSync(d);
	return d;
};
async function repo({ commits = true, remote } = {}) {
	const d = folder();
	await git(d, ["init", "-q", "-b", "main"]);
	if (commits) await git(d, ["commit", "-q", "--allow-empty", "-m", "init"]);
	if (remote) await git(d, ["remote", "add", "origin", remote]);
	return d;
}

beforeAll(() => {
	root = mkdtempSync(join(tmpdir(), "atlas-repostate-"));
	globalConfig = join(root, "global.gitconfig");
	writeFileSync(globalConfig, "[user]\n\tname = Ada\n\temail = ada@example.com\n");
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

const fixture = (name) =>
	JSON.parse(readFileSync(join(import.meta.dir, "__fixtures__", "gh", `${name}.json`), "utf8")).result;

// ---- git facts ----

test("probe: a folder that is not a repository is one process and says so", async () => {
	let calls = 0;
	const d = folder();
	const run = runIn(d);
	const p = await probeRepo(async (argv) => (calls++, run(argv)));
	expect(p).toEqual({ notRepo: true, hasCommits: false, remotes: [], defaultBranch: null });
	expect(calls).toBe(1);
});

test("probe: a repository with no commits and no remote", async () => {
	expect(await probeRepo(runIn(await repo({ commits: false })))).toEqual({
		notRepo: false,
		hasCommits: false,
		remotes: [],
		defaultBranch: null,
	});
});

test("probe: commits, a remote, and the default branch the remote names", async () => {
	const d = await repo({ remote: "https://github.com/codeatlasdev/atlas.git" });
	await git(d, ["update-ref", "refs/remotes/origin/main", "HEAD"]);
	await git(d, ["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
	expect(await probeRepo(runIn(d))).toEqual({
		notRepo: false,
		hasCommits: true,
		remotes: [{ name: "origin", url: "https://github.com/codeatlasdev/atlas.git" }],
		defaultBranch: "main",
	});
});

test("probe: a remote that is not on GitHub is reported as it is", async () => {
	const d = await repo({ remote: "git@gitlab.com:x/y.git" });
	const p = await probeRepo(runIn(d));
	expect(p.remotes).toEqual([{ name: "origin", url: "git@gitlab.com:x/y.git" }]);
	expect(repoState({ probe: p, auth: "ok", branch: "main", upstream: null, lookup: null })).toEqual({
		kind: "remote-not-github",
		remote: p.remotes[0],
	});
});

test("probe: an error that is not 'not a repository' is thrown, not swallowed as an empty repo", async () => {
	await expect(
		probeRepo(async () => ({ code: 128, stdout: "", stderr: "fatal: unsafe repository" })),
	).rejects.toThrow("unsafe repository");
});

test("remotes: one entry per name, from the fetch line, whatever the push URL is", () => {
	const out =
		"origin\thttps://github.com/a/b.git (fetch)\norigin\tgit@github.com:a/b.git (push)\nup\thttps://github.com/c/d.git (fetch)\nup\thttps://github.com/c/d.git (push)\n";
	expect(parseRemotes(out)).toEqual([
		{ name: "origin", url: "https://github.com/a/b.git" },
		{ name: "up", url: "https://github.com/c/d.git" },
	]);
	expect(parseRemotes("")).toEqual([]);
});

test("remote hosts: https, ssh://, scp-like, with user and port", () => {
	expect(remoteHost("https://github.com/a/b.git")).toBe("github.com");
	expect(remoteHost("https://user:tok@gitlab.com/a/b")).toBe("gitlab.com");
	expect(remoteHost("ssh://git@git.corp.example:2222/a/b")).toBe("git.corp.example");
	expect(remoteHost("git@GitHub.com:a/b.git")).toBe("github.com");
	expect(remoteHost("/srv/git/b.git")).toBe("");
	expect(isOtherForge("git@gitlab.com:a/b.git")).toBe(true);
	expect(isOtherForge("https://bitbucket.org/a/b")).toBe(true);
	expect(isOtherForge("https://github.com/a/b")).toBe(false);
	// An unknown host may be GitHub Enterprise: only gh can tell.
	expect(isOtherForge("https://git.corp.example/a/b")).toBe(false);
});

test("origin/HEAD short name gives the default branch, a branch with slashes keeps them", () => {
	expect(parseDefaultBranch("origin/main\n")).toBe("main");
	expect(parseDefaultBranch("origin/release/1.5")).toBe("release/1.5");
	expect(parseDefaultBranch("")).toBeNull();
});

// ---- gh answers, recorded ----

test("gh pr view of a real PR parses into the PrView the tab renders", () => {
	const l = classifyGhView(fixture("pr-view-open"));
	expect(l.kind).toBe("pr");
	expect(l.pr).toMatchObject({
		number: 497,
		state: "MERGED",
		reviewDecision: "REVIEW_REQUIRED",
		url: "https://github.com/codeatlasdev/atlas/pull/497",
	});
	expect(l.pr.statusCheckRollup.length).toBeGreaterThan(0);
});

test("gh pr view: every recorded failure maps to its state", () => {
	expect(classifyGhView(fixture("view-no-pr")).kind).toBe("none");
	expect(classifyGhView(fixture("view-not-github")).kind).toBe("not-github");
	expect(classifyGhView(fixture("view-ghe-host")).kind).toBe("not-github");
	expect(classifyGhView(fixture("view-signed-out")).kind).toBe("signed-out");
	// No remote at all is git's to report (probe); gh's wording must not read as a PR failure to retry.
	expect(classifyGhView(fixture("view-no-remote"))).toEqual({
		kind: "error",
		message: "no git remotes found",
	});
});

test("gh auth: signed in, signed out (recorded), not installed (rejection), host down (unknown)", async () => {
	expect(await ghAuth(async () => ({ code: 0, stdout: "", stderr: "" }))).toBe("ok");
	expect(await ghAuth(async () => fixture("auth-signed-out"))).toBe("signed-out");
	// hostd's wording for a program that does not exist: Rust's io::Error, Unix and Windows.
	expect(
		await ghAuth(async () => {
			throw new Error("gh: No such file or directory (os error 2)");
		}),
	).toBe("not-installed");
	expect(
		await ghAuth(async () => {
			throw "gh: The system cannot find the file specified. (os error 2)";
		}),
	).toBe("not-installed");
	expect(
		await ghAuth(async () => {
			throw new Error("hostd is not running");
		}),
	).toBe("unknown");
});

// ---- the model ----

const probe = (o = {}) => ({
	notRepo: false,
	hasCommits: true,
	remotes: [{ name: "origin", url: "https://github.com/codeatlasdev/atlas.git" }],
	defaultBranch: "main",
	...o,
});
const facts = (o = {}) => ({
	probe: probe(),
	auth: "ok",
	branch: "feat",
	upstream: null,
	lookup: { kind: "none" },
	...o,
});

test("nothing is decided before the first probe", () => {
	expect(repoState(facts({ probe: null }))).toBeNull();
});

test("each state, in the order the user has to fix them", () => {
	const kind = (o) => repoState(facts(o))?.kind;
	expect(kind({ probe: probe({ notRepo: true, hasCommits: false, remotes: [] }) })).toBe("not-repo");
	expect(kind({ probe: probe({ hasCommits: false, remotes: [] }) })).toBe("no-commits");
	expect(kind({ probe: probe({ remotes: [] }) })).toBe("no-remote");
	expect(kind({ probe: probe({ remotes: [{ name: "origin", url: "https://gitlab.com/a/b" }] }) })).toBe(
		"remote-not-github",
	);
	expect(kind({ auth: "not-installed", lookup: null })).toBe("gh-missing");
	expect(kind({ auth: "signed-out", lookup: null })).toBe("gh-signed-out");
	expect(kind({ lookup: { kind: "signed-out" } })).toBe("gh-signed-out");
	expect(kind({ lookup: { kind: "not-github" } })).toBe("remote-not-github"); // an unknown host gh does not know
	expect(kind({ lookup: { kind: "error", message: "boom" } })).toBe("gh-error");
	expect(kind({ branch: "main" })).toBe("default-branch");
	expect(kind({ branch: "feat", upstream: null })).toBe("not-pushed");
	expect(kind({ branch: "feat", upstream: "origin/feat" })).toBe("ready");
	expect(kind({ lookup: { kind: "pr", pr: { number: 1 } } })).toBe("pr");
});

test("an empty repository is 'no commits' even with a remote: that is the first thing to fix", () => {
	expect(repoState(facts({ probe: probe({ hasCommits: false }) })).kind).toBe("no-commits");
});

test("without origin/HEAD, main and master still count as the default branch; a feature branch does not", () => {
	const noHead = probe({ defaultBranch: null });
	expect(repoState(facts({ probe: noHead, branch: "master" })).kind).toBe("default-branch");
	expect(repoState(facts({ probe: noHead, branch: "feat" })).kind).toBe("not-pushed");
	// A repository whose default branch is `trunk`: `main` is an ordinary branch there.
	expect(repoState(facts({ probe: probe({ defaultBranch: "trunk" }), branch: "main" })).kind).toBe(
		"not-pushed",
	);
	expect(repoState(facts({ probe: probe({ defaultBranch: "trunk" }), branch: "trunk" })).kind).toBe(
		"default-branch",
	);
});

test("not pushed names the remote to push to: origin when there is one, else the first", () => {
	const two = probe({
		remotes: [
			{ name: "fork", url: "https://github.com/me/atlas" },
			{ name: "origin", url: "https://github.com/codeatlasdev/atlas" },
		],
	});
	expect(repoState(facts({ probe: two })).remote).toBe("origin");
	const one = probe({ remotes: [{ name: "fork", url: "https://github.com/me/atlas" }] });
	expect(repoState(facts({ probe: one })).remote).toBe("fork");
});

test("while gh has not been asked yet the tab keeps waiting instead of offering Create", () => {
	expect(repoState(facts({ auth: "unknown", lookup: null }))).toBeNull();
	expect(repoState(facts({ lookup: null }))).toBeNull();
});

test("every state that is not a PR says why and offers its next step (only 'no commits' sends you to Source Control)", () => {
	const states = [
		{ kind: "no-commits" },
		{ kind: "no-remote" },
		{ kind: "remote-not-github", remote: { name: "origin", url: "https://gitlab.com/a/b" } },
		{ kind: "remote-not-github", remote: null },
		{ kind: "gh-missing" },
		{ kind: "gh-signed-out" },
		{ kind: "gh-error", message: "boom" },
		{ kind: "default-branch", branch: "main" },
		{ kind: "not-pushed", branch: "feat", remote: "origin" },
		{ kind: "ready", branch: "feat" },
	];
	const actions = states.map((s) => describeState(s).action?.id ?? null);
	expect(actions).toEqual([
		null,
		"add-remote",
		"add-remote",
		"add-remote",
		"install-gh",
		"sign-in",
		"retry",
		"create-branch",
		"push",
		"create-pr",
	]);
	expect(describeState(states[2]).description).toContain("https://gitlab.com/a/b");
	expect(describeState(states[7]).title).toContain("main");
	for (const s of states.filter((x) => x.kind !== "gh-error")) {
		const t = describeState(s);
		expect(t.title.length).toBeGreaterThan(5);
		expect(t.description.length).toBeGreaterThan(10);
	}
	// 'Create pull request' is offered in exactly one state.
	expect(states.filter((s) => describeState(s).action?.id === "create-pr")).toHaveLength(1);
});
