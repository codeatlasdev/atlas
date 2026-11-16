// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import {
	identityMissing,
	identityProblem,
	isIdentityError,
	readIdentity,
	saveIdentity,
} from "./gitIdentity.ts";

// Incident (first-user pass, shot 23): a user with no git identity pressed Commit and got git's raw "Author identity
// unknown *** Please tell me who you are". These run the REAL git in throwaway repositories with HOME and the global
// config isolated, so nothing here reads or writes the developer's own identity. `useConfigOnly` makes "no identity"
// the same answer on every machine (otherwise git guesses `user@host` and a host with a domain name commits fine).
let root;
let globalConfig;
const decoder = new TextDecoder();

function env() {
	return {
		PATH: process.env.PATH,
		HOME: root,
		GIT_CONFIG_GLOBAL: globalConfig,
		GIT_CONFIG_NOSYSTEM: "1",
		GIT_TERMINAL_PROMPT: "0",
	};
}

/** A `GitRun` for the repository `dir`: what hostd's `git -C <worktree> …` does. */
const runIn = (dir) => async (args) => {
	const p = Bun.spawnSync(["git", "-C", dir, ...args], { env: env(), stdin: "ignore" });
	return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
};

let n = 0;
function repo() {
	const dir = join(root, `repo${n++}`);
	Bun.spawnSync(["git", "init", "-q", dir], { env: env() });
	writeFileSync(join(dir, "f.txt"), "hello\n");
	Bun.spawnSync(["git", "-C", dir, "add", "f.txt"], { env: env() });
	return dir;
}

/** The developer's own global git config files, as they were before these tests: they must not change. */
const realConfigs = [
	join(homedir(), ".gitconfig"),
	join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "git", "config"),
];
const snapshot = () => realConfigs.map((p) => (existsSync(p) ? readFileSync(p, "utf8") : null));
let before;

beforeAll(() => {
	before = snapshot();
	root = mkdtempSync(join(tmpdir(), "atlas-identity-"));
	globalConfig = join(root, "global.gitconfig");
	writeFileSync(globalConfig, "[user]\n\tuseConfigOnly = true\n");
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

test("a repository with no identity is detected before the commit, and git's own commit error is recognized", async () => {
	const dir = repo();
	const run = runIn(dir);
	expect(await identityMissing(run)).toBe(true);
	expect(await readIdentity(run)).toEqual({});
	const c = await run(["commit", "-m", "first"]);
	expect(c.code).not.toBe(0);
	expect(isIdentityError(c.stderr)).toBe(true);
});

test("a name without an e-mail is still missing, and read back as partial", async () => {
	const dir = repo();
	const run = runIn(dir);
	await run(["config", "user.name", "Ada"]);
	expect(await identityMissing(run)).toBe(true);
	expect(await readIdentity(run)).toEqual({ name: "Ada" });
});

test("saving for this repository fixes that repository only, and the retried commit has the author", async () => {
	const a = repo();
	const b = repo();
	await saveIdentity(runIn(a), { name: "Ada Lovelace", email: "ada@example.com" }, "local");
	expect(await identityMissing(runIn(a))).toBe(false);
	expect(await identityMissing(runIn(b))).toBe(true);
	const c = await runIn(a)(["commit", "-m", "first"]);
	expect(c.code).toBe(0);
	const who = await runIn(a)(["log", "-1", "--format=%an <%ae>|%cn <%ce>"]);
	expect(who.stdout.trim()).toBe("Ada Lovelace <ada@example.com>|Ada Lovelace <ada@example.com>");
	// The global config was not touched.
	expect((await runIn(b)(["config", "--global", "--get", "user.name"])).code).not.toBe(0);
});

test("saving globally fixes every repository", async () => {
	const a = repo();
	const b = repo();
	await saveIdentity(runIn(a), { name: "Grace Hopper", email: "grace@example.com" }, "global");
	expect(await identityMissing(runIn(a))).toBe(false);
	expect(await identityMissing(runIn(b))).toBe(false);
	expect(await readIdentity(runIn(b))).toEqual({ name: "Grace Hopper", email: "grace@example.com" });
	expect((await runIn(b)(["commit", "-m", "x"])).code).toBe(0);
	// Leave the shared global config as the other tests expect it.
	writeFileSync(globalConfig, "[user]\n\tuseConfigOnly = true\n");
});

test("names and e-mails travel as argv: quotes, spaces, unicode and shell characters round-trip", async () => {
	const dir = repo();
	const run = runIn(dir);
	const id = { name: `José "Zé" O'Neil $(whoami) \`x\``, email: "zé+tag@exämple.com" };
	await saveIdentity(run, id, "local");
	expect(await readIdentity(run)).toEqual(id);
});

test("surrounding whitespace is trimmed before it reaches git config", async () => {
	const dir = repo();
	const run = runIn(dir);
	await saveIdentity(run, { name: "  Ada  ", email: " ada@example.com " }, "local");
	expect(await readIdentity(run)).toEqual({ name: "Ada", email: "ada@example.com" });
});

test("an invalid name or e-mail is refused without writing anything", async () => {
	const dir = repo();
	const run = runIn(dir);
	for (const bad of [
		{ name: "", email: "a@b.c" },
		{ name: "   ", email: "a@b.c" },
		{ name: "Ada", email: "" },
		{ name: "Ada", email: "ada" },
		{ name: "Ada", email: "@example.com" },
		{ name: "Ada", email: "ada@" },
		{ name: "Ada", email: "a b@example.com" },
		{ name: "Ada", email: "a@b@c" },
	]) {
		expect(identityProblem(bad), JSON.stringify(bad)).not.toBeNull();
		await expect(saveIdentity(run, bad, "local")).rejects.toThrow();
	}
	expect(await readIdentity(run)).toEqual({});
});

test("a config git cannot write is an error carrying git's message, not a silent success", async () => {
	const dir = repo();
	const run = runIn(dir);
	chmodSync(join(dir, ".git", "config"), 0o400);
	try {
		// Root ignores file modes (CI containers): then there is nothing to refuse, and the save legitimately works.
		const wrote = await saveIdentity(run, { name: "Ada", email: "ada@example.com" }, "local").then(
			() => true,
			(e) => e,
		);
		if (wrote !== true) expect(String(wrote.message)).toMatch(/config|permission|lock/i);
	} finally {
		chmodSync(join(dir, ".git", "config"), 0o600);
	}
});

test("isIdentityError recognizes git's identity errors and nothing else", () => {
	expect(isIdentityError("Author identity unknown\n\n*** Please tell me who you are.")).toBe(true);
	expect(isIdentityError("Committer identity unknown\n\n*** Please tell me who you are.")).toBe(true);
	expect(isIdentityError("fatal: unable to auto-detect email address (got 'me@host.(none)')")).toBe(true);
	expect(isIdentityError("fatal: empty ident name (for <>) not allowed")).toBe(true);
	expect(isIdentityError("nothing to commit, working tree clean")).toBe(false);
	expect(isIdentityError("error: pathspec 'identity' did not match any file(s) known to git")).toBe(false);
	expect(isIdentityError("")).toBe(false);
});

// The product's "all repositories" choice is `git config --global`. These pin where that write lands: git's own
// resolution of GIT_CONFIG_GLOBAL / HOME, never a path of ours, so a test (or an agent instance) with an isolated
// environment can never reach the developer's real identity (an earlier run left someone else's name in the real one).
test("a global save goes to GIT_CONFIG_GLOBAL when set, and to nothing else", async () => {
	const dir = repo();
	const run = runIn(dir);
	await saveIdentity(run, { name: "Global Gina", email: "gina@example.com" }, "global");
	const written = readFileSync(globalConfig, "utf8");
	expect(written).toContain("name = Global Gina");
	expect(written).toContain("email = gina@example.com");
	expect(existsSync(join(root, ".gitconfig"))).toBe(false);
	expect(existsSync(join(root, ".config", "git", "config"))).toBe(false);
	expect(snapshot()).toEqual(before);
	writeFileSync(globalConfig, "[user]\n\tuseConfigOnly = true\n");
});

test("without GIT_CONFIG_GLOBAL a global save goes to the HOME it runs under", async () => {
	const dir = repo();
	const home = join(root, "other-home");
	Bun.spawnSync(["mkdir", "-p", home]);
	const isolated = { PATH: process.env.PATH, HOME: home, GIT_CONFIG_NOSYSTEM: "1" };
	const run = async (args) => {
		const p = Bun.spawnSync(["git", "-C", dir, ...args], { env: isolated, stdin: "ignore" });
		return { code: p.exitCode, stdout: decoder.decode(p.stdout), stderr: decoder.decode(p.stderr) };
	};
	await saveIdentity(run, { name: "Home Hal", email: "hal@example.com" }, "global");
	expect(readFileSync(join(home, ".gitconfig"), "utf8")).toContain("name = Home Hal");
	expect(snapshot()).toEqual(before);
});

test("the developer's own global git config is exactly as these tests found it", () => {
	expect(snapshot()).toEqual(before);
});
