// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, afterEach, beforeAll, beforeEach, expect, mock, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

// Incidents (first-user pass, shots 23 and 26):
//  - a user with no git identity pressed Commit and was shown git's raw "Author identity unknown *** Please tell me
//    who you are" instead of being asked for a name and an e-mail;
//  - that commit error stayed on screen after the state it described was gone (next to "Stage all").
// The real component and stores run against REAL git in throwaway repositories (HOME and the global config isolated, so
// nothing reads or writes the developer's identity): the mocked IPC only forwards `git_run`/`git_status` to the git
// binary, as hostd does. `useConfigOnly` makes "no identity" the same answer on every machine.
let root;
let globalConfig;
let repo = "";
let gh = null;
const decoder = new TextDecoder();

const env = () => ({
	PATH: process.env.PATH,
	HOME: root,
	GIT_CONFIG_GLOBAL: globalConfig,
	GIT_CONFIG_NOSYSTEM: "1",
	GIT_TERMINAL_PROMPT: "0",
});
const git = (args, stdin) => {
	const p = Bun.spawnSync(["git", "-C", repo, ...args], {
		env: env(),
		stdin: stdin === undefined ? "ignore" : new TextEncoder().encode(stdin),
	});
	return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
};

function status() {
	const out = git(["status", "--porcelain=v1", "--branch"]).stdout.split("\n").filter(Boolean);
	const branch = (out.shift() ?? "").replace(/^## (No commits yet on )?/, "").split("...")[0] || "main";
	const entries = out.map((l) => ({ x: l[0], y: l[1], path: l.slice(3) }));
	return { branch, upstream: null, ab: null, base: "HEAD", vs_base: null, entries, operation: null };
}

mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd, args) => {
		switch (cmd) {
			case "git_run":
				return git(args.args, args.stdin ?? undefined);
			case "git_status":
				return status();
			case "gh_identity":
				return gh;
			case "commit_msg_provider":
				return { state: "none" };
			default:
				return null;
		}
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
// Other suites replace "#lib/api.ts" (and the status feed built on it) for the whole run: this one needs the real
// `git` calls over the mocked IPC above, so it puts the real module back, as a private instance.
const realApi = await import("../api.ts?real");
mock.module("#lib/api.ts", () => realApi);
const { createGitStatusFeed } = await import("#lib/stores/gitStatusFeed.ts");
mock.module("#lib/stores/gitStatus.ts", () => ({
	gitStatus: createGitStatusFeed({
		fetch: realApi.git.status,
		listen: async () => () => {},
		delay: 20,
		maxWait: 200,
		now: () => Date.now(),
		setTimeout: (fn, ms) => window.setTimeout(fn, ms),
		clearTimeout: (id) => window.clearTimeout(id),
	}),
}));
await (
	await import("../../../test/uiKit.ts")
).useRealKit("Button", "Spinner", "TextArea", "DropdownMenu", "DropdownMenu/DropdownMenuItem");
const { flushSync, mount, tick, unmount } = await import("svelte");
const { gitStatus } = await import("#lib/stores/gitStatus.ts");
const { default: SourceControl } = await import("./SourceControl.svelte");

// The identity form's "all repositories" choice writes `git config --global`: it must land in the isolated
// GIT_CONFIG_GLOBAL above, never in the developer's real config.
const realConfigs = [
	join(homedir(), ".gitconfig"),
	join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "git", "config"),
];
const snapshot = () => realConfigs.map((p) => (existsSync(p) ? readFileSync(p, "utf8") : null));
let before;

beforeAll(() => {
	before = snapshot();
	root = mkdtempSync(join(tmpdir(), "atlas-scm-"));
	globalConfig = join(root, "global.gitconfig");
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

let n = 0;
let done;
beforeEach(() => {
	writeFileSync(globalConfig, "[user]\n\tuseConfigOnly = true\n");
	gh = null;
	repo = join(root, `repo${n++}`);
	Bun.spawnSync(["git", "init", "-q", "-b", "main", repo], { env: env() });
	writeFileSync(join(repo, "f.txt"), "hello\n");
});
afterEach(() => {
	done?.();
	done = undefined;
	document.body.innerHTML = "";
});

const until = async (cond, what) => {
	for (let i = 0; i < 400; i++) {
		flushSync();
		if (cond()) return;
		await new Promise((r) => setTimeout(r, 10));
	}
	throw new Error(`timed out waiting for ${what}\n${document.body.textContent}`);
};
const text = () => document.body.textContent.replace(/\s+/g, " ");
const button = (label) =>
	[...document.body.querySelectorAll("button")].find((b) => b.textContent.trim() === label);
const field = (label) =>
	[...document.body.querySelectorAll("label")]
		.find((l) => l.textContent.trim().startsWith(label))
		?.querySelector("input");
const type = (input, value) => {
	input.value = value;
	input.dispatchEvent(new Event("input", { bubbles: true }));
	flushSync();
};
const click = (el) => {
	el.click();
	flushSync();
};

async function open() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(SourceControl, { target, props: { worktreeId: "wt" } }));
	done = () => (unmount(app), gitStatus.refresh("wt"));
	await until(() => button("Stage all"), "the status to load");
	return target;
}

/** Stage everything through the panel and type a commit message. */
async function readyToCommit(message = "first commit") {
	await open();
	click(button("Stage all"));
	await until(() => button("Commit"), "files to be staged");
	type(document.body.querySelector("textarea"), message);
}

test("Commit with no identity asks for one instead of showing git's wall of text, then commits with it", async () => {
	gh = { name: "Octo Cat", email: "1+octo@users.noreply.github.com" };
	await readyToCommit();
	click(button("Commit"));
	await until(() => field("Name"), "the identity form");
	expect(text()).not.toContain("Author identity unknown");
	expect(text()).not.toContain("Please tell me who you are");
	// Prefilled from the signed-in GitHub account; the commit has not happened yet.
	expect(field("Name").value).toBe("Octo Cat");
	expect(field("E-mail").value).toBe("1+octo@users.noreply.github.com");
	expect(git(["rev-parse", "--verify", "HEAD"]).code).not.toBe(0);

	type(field("Name"), "Ada Lovelace");
	type(field("E-mail"), "ada@example.com");
	click(button("Save & commit"));
	await until(() => git(["rev-parse", "--verify", "HEAD"]).code === 0, "the retried commit");
	expect(git(["log", "-1", "--format=%an <%ae>%n%s"]).stdout).toBe(
		"Ada Lovelace <ada@example.com>\nfirst commit\n",
	);
	// Global by default: a second repository is fixed too.
	expect(git(["config", "--global", "--get", "user.email"]).stdout.trim()).toBe("ada@example.com");
	await until(() => !field("Name"), "the form to close");
	expect(document.body.querySelector("textarea").value).toBe("");
	expect(text()).not.toContain("identity");
});

test("choosing 'this repository only' writes the repository's config and leaves the global one alone", async () => {
	await readyToCommit();
	click(button("Commit"));
	await until(() => field("Name"), "the identity form");
	type(field("Name"), "Grace Hopper");
	type(field("E-mail"), "grace@example.com");
	const radios = [...document.body.querySelectorAll('input[type="radio"]')];
	click(radios.find((r) => r.value === "local"));
	click(button("Save & commit"));
	await until(() => git(["rev-parse", "--verify", "HEAD"]).code === 0, "the retried commit");
	expect(git(["config", "--local", "--get", "user.email"]).stdout.trim()).toBe("grace@example.com");
	expect(git(["config", "--global", "--get", "user.email"]).code).not.toBe(0);
});

test("a name git already has is kept in the form, and gh is not asked for it", async () => {
	git(["config", "user.name", "Already Set"]);
	gh = { name: "Octo Cat", email: "octo@example.com" };
	await readyToCommit();
	click(button("Commit"));
	await until(() => field("Name"), "the identity form");
	await until(() => field("E-mail").value !== "", "the e-mail prefill");
	expect(field("Name").value).toBe("Already Set");
	expect(field("E-mail").value).toBe("octo@example.com");
});

test("an invalid e-mail is explained in the form and nothing is written or committed", async () => {
	await readyToCommit();
	click(button("Commit"));
	await until(() => field("Name"), "the identity form");
	type(field("Name"), "Ada");
	type(field("E-mail"), "not-an-email");
	click(button("Save & commit"));
	await until(() => text().includes("e-mail address like you@example.com"), "the inline error");
	expect(field("Name")).not.toBeNull();
	expect(git(["config", "--global", "--get", "user.name"]).code).not.toBe(0);
	expect(git(["rev-parse", "--verify", "HEAD"]).code).not.toBe(0);
});

test("Cancel closes the form and keeps the message for later", async () => {
	await readyToCommit("keep me");
	click(button("Commit"));
	await until(() => field("Name"), "the identity form");
	click(button("Cancel"));
	await until(() => !field("Name"), "the form to close");
	expect(document.body.querySelector("textarea").value).toBe("keep me");
});

function failingHook() {
	const hook = join(repo, ".git", "hooks", "pre-commit");
	writeFileSync(hook, "#!/bin/sh\necho 'hook says no' >&2\nexit 1\n");
	chmodSync(hook, 0o755);
	return hook;
}

test("a commit error goes away when the message is edited: it described the old message", async () => {
	git(["config", "user.name", "Ada"]);
	git(["config", "user.email", "ada@example.com"]);
	failingHook();
	await readyToCommit();
	click(button("Commit"));
	await until(() => text().includes("hook says no"), "the commit error");
	type(document.body.querySelector("textarea"), "first commit, reworded");
	await until(() => !text().includes("hook says no"), "the stale error to clear");
});

test("a commit error goes away when the staged set changes under it (committed or unstaged elsewhere)", async () => {
	git(["config", "user.name", "Ada"]);
	git(["config", "user.email", "ada@example.com"]);
	const hook = failingHook();
	await readyToCommit();
	click(button("Commit"));
	await until(() => text().includes("hook says no"), "the commit error");
	// Fixed in a terminal: the hook is gone and the commit happens outside the panel.
	rmSync(hook);
	expect(git(["commit", "-m", "from the terminal"]).code).toBe(0);
	await gitStatus.refresh("wt");
	await until(() => !text().includes("hook says no"), "the stale error to clear");
	// What is left is the next step, not a failure.
	expect(button("Commit")).toBeUndefined();
});

test("a failed push is not a commit error: editing the message leaves it on screen", async () => {
	git(["config", "user.name", "Ada"]);
	git(["config", "user.email", "ada@example.com"]);
	git(["add", "f.txt"]);
	git(["commit", "-m", "init"]);
	await open2();
	click(button("Push"));
	await until(
		() => /no configured push destination|No configured push destination/.test(text()),
		"the push error",
	);
	type(document.body.querySelector("textarea"), "something");
	await tick();
	flushSync();
	expect(text()).toMatch(/push destination/i);
});

async function open2() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(SourceControl, { target, props: { worktreeId: "wt" } }));
	done = () => (unmount(app), gitStatus.refresh("wt"));
	await until(() => button("Push"), "the status to load");
}

test("a later successful commit leaves no error behind", async () => {
	git(["config", "user.name", "Ada"]);
	git(["config", "user.email", "ada@example.com"]);
	const hook = failingHook();
	await readyToCommit();
	click(button("Commit"));
	await until(() => text().includes("hook says no"), "the commit error");
	rmSync(hook);
	click(button("Commit"));
	await until(() => git(["rev-parse", "--verify", "HEAD"]).code === 0, "the commit");
	await until(() => !text().includes("hook says no"), "the error to clear");
});

test("the developer's own global git config is exactly as these tests found it", () => {
	expect(snapshot()).toEqual(before);
	expect(readFileSync(globalConfig, "utf8")).toContain("useConfigOnly");
});
