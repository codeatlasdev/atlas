// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, afterEach, beforeAll, beforeEach, expect, mock, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { m } from "#lib/i18n/m.ts";

// Incident (#506): the Pull request tab of a folder that is not a repository said only "No pull request for this
// branch." with a Create pull request button. The real tab and stores run against REAL git in throwaway folders; the
// IPC mock only does what hostd's `exec` does (run the program in the folder). `gh` answers are the outputs recorded
// from the real gh (src/lib/__fixtures__/gh), chosen per scenario.
let root;
let globalConfig;
let dir = "";
/** What `gh` does in this scenario: `auth` and `view` are recorded results, or `missing` (no such program). */
let gh = {};
let ghCalls = [];
const decoder = new TextDecoder();
const b64 = (s) => Buffer.from(s).toString("base64");

const env = () => ({
	PATH: process.env.PATH,
	HOME: root,
	GIT_CONFIG_GLOBAL: globalConfig,
	GIT_CONFIG_NOSYSTEM: "1",
	GIT_TERMINAL_PROMPT: "0",
	LC_ALL: "C",
});
const git = (args, cwd = dir) => {
	const p = Bun.spawnSync(["git", ...args], { cwd, env: env(), stdin: "ignore" });
	return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
};
const fixture = (name) =>
	JSON.parse(readFileSync(join(import.meta.dir, "..", "__fixtures__", "gh", `${name}.json`), "utf8")).result;

function exec({ cwd, argv }) {
	let r;
	if (argv[0] === "gh") {
		ghCalls.push(argv.slice(1, 3).join(" "));
		const key = argv[1] === "auth" ? "auth" : argv[2] === "checks" ? "checks" : "view";
		if (gh[key] === "missing") throw "gh: No such file or directory (os error 2)";
		r = gh[key] ?? { code: 1, stdout: "", stderr: `unexpected gh ${argv.slice(1).join(" ")}` };
	} else {
		const p = Bun.spawnSync(argv, { cwd, env: env(), stdin: "ignore" });
		r = { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
	}
	return { code: r.code, stdout: b64(r.stdout), stderr: b64(r.stderr), timed_out: false };
}

mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd, args) => {
		if (cmd === "host_call" && args.cmd === "exec") return exec(args.args);
		if (cmd === "git_run") {
			const p = Bun.spawnSync(["git", "-C", dir, ...args.args], { env: env(), stdin: "ignore" });
			return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
		}
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
// Other suites replace these two for the whole run; this one needs the real modules over the IPC above.
const realApi = await import("../api.ts?real");
mock.module("#lib/api.ts", () => realApi);
const realHost = await import("../host.ts?real");
mock.module("#lib/host.ts", () => realHost);

// The status feed over real git: what Source Control and the Explorer share, minus Tauri's event listener.
const { createGitStatusFeed } = await import("#lib/stores/gitStatusFeed.ts");
const feed = createGitStatusFeed({
	fetch: async () => {
		const branch = git(["branch", "--show-current"]).stdout.trim() || null;
		const up = git(["rev-parse", "--abbrev-ref", "@{u}"]);
		return {
			branch,
			upstream: up.code === 0 ? up.stdout.trim() : null,
			ab: null,
			base: "HEAD",
			vs_base: null,
			entries: [],
			operation: null,
		};
	},
	listen: async () => () => {},
	delay: 20,
	maxWait: 200,
	now: () => Date.now(),
	setTimeout: (fn, ms) => window.setTimeout(fn, ms),
	clearTimeout: (id) => window.clearTimeout(id),
});
mock.module("#lib/stores/gitStatus.ts", () => ({ gitStatus: feed }));
// The branch picker pulls the palette in: only that it is asked to open matters here.
let picker = 0;
mock.module("#lib/stores/branch.svelte.ts", () => ({
	openBranchPicker: async () => void picker++,
	branchPicker: {},
	branchItems: () => [],
	createBranchItems: () => [],
}));

await (
	await import("../../../test/uiKit.ts")
).useRealKit(
	"Button",
	"Spinner",
	"EmptyState",
	"StatusChip",
	"TextArea",
	"DropdownMenu",
	"DropdownMenu/DropdownMenuItem",
);
const { toast } = await import("@codeatlasdev/ui");
const { flushSync, mount, unmount } = await import("svelte");
const { ws } = await import("#lib/stores/workspace.svelte.ts");
const { pr } = await import("#lib/stores/pr.svelte.ts");
const { default: PrTab } = await import("./PrTab.svelte");

beforeAll(() => {
	root = mkdtempSync(join(tmpdir(), "atlas-prtab-"));
	globalConfig = join(root, "global.gitconfig");
	writeFileSync(globalConfig, "[user]\n\tname = Ada\n\temail = ada@example.com\n");
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

let n = 0;
let done;
let toasts = [];
toast.error = (message) => toasts.push(message);
beforeEach(() => {
	toasts = [];
	ghCalls = [];
	picker = 0;
	pr.createFor = null;
	gh = { auth: { code: 0, stdout: "", stderr: "" }, view: fixture("view-no-pr") };
	dir = join(root, `d${n++}`);
	mkdirSync(dir);
	ws.doc.worktrees = [{ id: "wt", repoId: "r", path: dir, name: "w", isMain: true }];
});
afterEach(() => {
	done?.();
	done = undefined;
	document.body.innerHTML = "";
});

const text = () => document.body.textContent.replace(/\s+/g, " ");
const button = (label) =>
	[...document.body.querySelectorAll("button")].find((b) => b.textContent.trim() === label);
const until = async (cond, what) => {
	for (let i = 0; i < 400; i++) {
		flushSync();
		if (cond()) return;
		await new Promise((r) => setTimeout(r, 10));
	}
	throw new Error(`timed out waiting for ${what}\n${text()}`);
};
async function open() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(PrTab, { target, props: { worktreeId: "wt", focused: true } }));
	done = () => unmount(app);
}
const repo = ({ commits = true, remote, branch = "main" } = {}) => {
	git(["init", "-q", "-b", branch]);
	if (commits) git(["commit", "-q", "--allow-empty", "-m", "init"]);
	if (remote) git(["remote", "add", "origin", remote]);
};
const GITHUB = "https://github.com/codeatlasdev/atlas.git";
/** Everything the tab must NOT offer when there can be no pull request. */
const noCreate = () => {
	expect(button("Create pull request")).toBeUndefined();
	expect(text()).not.toContain("No pull request for this branch");
};

test("a folder that is not a repository says so and offers Initialize repository, not Create pull request", async () => {
	await open();
	await until(() => text().includes("This folder is not a git repository"), "the not-a-repository state");
	noCreate();
	expect(button("Initialize repository")).toBeDefined();
	expect(ghCalls).toEqual([]); // git already explains it: gh is not asked
	expect(toasts).toEqual([]);

	button("Initialize repository").click();
	await until(() => text().includes("No commits yet"), "the next state after git init");
	expect(git(["rev-parse", "--git-dir"]).code).toBe(0);
});

test("a repository with no commits says so; there is nothing to create yet", async () => {
	repo({ commits: false, remote: GITHUB });
	await open();
	await until(() => text().includes("No commits yet"), "the state");
	noCreate();
	expect(ghCalls).toEqual([]);
});

test("a repository with no remote offers Add a remote, and adding one moves on to the next state", async () => {
	repo();
	await open();
	await until(() => text().includes("This repository has no remote"), "the state");
	noCreate();
	expect(ghCalls).toEqual([]);
	// The dialog is the app's own prompt; answer it as the user would.
	const { ask } = await import("#lib/stores/ask.svelte.ts");
	button("Add a remote").click();
	await until(() => ask.current, "the address prompt");
	ask.current.resolve("https://github.com/codeatlasdev/atlas.git");
	ask.current = null;
	await until(
		() => git(["remote", "-v"]).stdout.includes("github.com/codeatlasdev/atlas.git"),
		"git remote add",
	);
	await until(
		() => text().includes("is the default branch") || text().includes("You are on main"),
		"the next state",
	);
});

test("a remote that is not on GitHub says which one and asks gh nothing", async () => {
	repo({ remote: "git@gitlab.com:team/app.git" });
	await open();
	await until(() => text().includes("The remote is not on GitHub"), "the state");
	expect(text()).toContain("git@gitlab.com:team/app.git");
	noCreate();
	expect(ghCalls).toEqual([]);
});

test("a host gh does not know (GitHub Enterprise, or not GitHub) is told apart by gh itself", async () => {
	repo({ remote: "https://git.corp.example/team/app.git" });
	gh.view = fixture("view-ghe-host");
	await open();
	await until(() => text().includes("The remote is not on GitHub"), "the state");
	noCreate();
});

test("gh not installed offers Install GitHub CLI", async () => {
	repo({ remote: GITHUB });
	gh.auth = "missing";
	await open();
	await until(() => text().includes("GitHub CLI is not installed"), "the state");
	expect(button("Install GitHub CLI")).toBeDefined();
	noCreate();
});

test("gh not signed in offers Sign in to GitHub (gh auth login in a terminal)", async () => {
	repo({ remote: GITHUB });
	gh.auth = fixture("auth-signed-out");
	let opened;
	const real = ws.newTerminal;
	ws.newTerminal = async (id, o) => void (opened = { id, ...o });
	try {
		await open();
		await until(() => text().includes("Sign in to GitHub"), "the state");
		noCreate();
		button("Sign in to GitHub").click();
		expect(opened).toMatchObject({ id: "wt", argv: ["gh", "auth", "login"] });
	} finally {
		ws.newTerminal = real;
	}
});

test("on the default branch it says so and offers to create a branch", async () => {
	repo({ remote: GITHUB });
	await open();
	await until(() => text().includes("You are on main, the default branch"), "the state");
	noCreate();
	button("Create branch").click();
	expect(picker).toBe(1);
});

test("a branch that was never pushed offers Push branch, which pushes it and moves on to Create pull request", async () => {
	const bare = join(root, `bare${n++}`);
	mkdirSync(bare);
	git(["init", "-q", "--bare"], bare);
	repo({ remote: bare });
	git(["switch", "-q", "-c", "feat/x"]);
	await open();
	await until(() => text().includes("feat/x is not pushed yet"), "the state");
	noCreate();
	button("Push branch").click();
	await until(() => button("Create pull request"), "Create pull request once the branch is pushed");
	expect(git(["rev-parse", "--abbrev-ref", "@{u}"], dir).stdout.trim()).toBe("origin/feat/x");
	expect(git(["rev-parse", "refs/heads/feat/x"], bare).code).toBe(0);
	expect(text()).toContain("No pull request for this branch");
	button("Create pull request").click();
	expect(pr.createFor).toBe("wt");
});

test("a failed push is a sentence with Details, never git's wall of text", async () => {
	repo({ remote: join(root, "nowhere") });
	git(["switch", "-q", "-c", "feat/y"]);
	await open();
	await until(() => text().includes("feat/y is not pushed yet"), "the state");
	button("Push branch").click();
	await until(() => toasts.length, "the error toast");
	expect(toasts[0]).toBe(
		"Git could not reach the remote or sign in to it. Check the address and your credentials.",
	);
});

test("a pull request shows its title, state, checks, review state and a link to GitHub", async () => {
	repo({ remote: GITHUB, branch: "ci/refuse-placeholder-identities" });
	gh.view = fixture("pr-view-open");
	gh.checks = fixture("pr-checks");
	await open();
	await until(() => text().includes("#497"), "the PR");
	expect(text()).toContain("the Signed-off-by check refuses placeholder identities");
	expect(text()).toContain(m.git_github_pr_merged());
	expect(text()).toContain(m.git_github_review_required());
	expect(text()).toMatch(
		new RegExp(
			`(${m.git_github_checks_chip_ok()}|${m.git_github_checks_chip_failed()}|${m.git_github_checks_chip_pending()})`,
		),
	);
	expect(document.body.querySelector('button[title="Open on GitHub"]')).not.toBeNull();
	expect(text()).toContain("Checks");
	noCreate();
});
