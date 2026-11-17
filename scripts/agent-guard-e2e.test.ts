import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import agentGuard from "../.omp/extensions/agent-guard";
import { GIT_ENV, git, makeRepo } from "./lib/agent-guard/fixture";

// End to end: the real entry points, driven the way the harnesses and git drive them.
const entry = join(import.meta.dir, "agent-guard.ts");
const hooksDir = join(import.meta.dir, "git-hooks");
const r = makeRepo();

/** What Claude Code writes on the hook's stdin for a PreToolUse event. */
function claudeHook(tool_name: string, tool_input: unknown, env: Record<string, string> = {}) {
	const payload = { session_id: "s", transcript_path: "/x", cwd: r.a, permission_mode: "default", hook_event_name: "PreToolUse", tool_name, tool_input, tool_use_id: "t" };
	const p = Bun.spawnSync([process.execPath, entry, "hook"], { cwd: r.a, stdin: new TextEncoder().encode(JSON.stringify(payload)), env: { ...process.env, ATLAS_MAINTAINER_OK: "", ...env } });
	return { code: p.exitCode, stderr: p.stderr.toString(), stdout: p.stdout.toString() };
}

test("Claude Code hook: a Write into the main checkout exits 2 and tells the agent where to write", () => {
	const res = claudeHook("Write", { file_path: join(r.main, "crates", "x.rs"), content: "fn main() {}" });
	expect(res.code).toBe(2);
	expect(res.stderr).toContain(`refused: ${join(r.main, "crates", "x.rs")} is in the main checkout`);
	expect(res.stderr).toContain(`use the absolute path under your worktree: ${join(r.a, "crates", "x.rs")}`);
});

test("Claude Code hook: the same Write inside the worktree exits 0 with no output", () => {
	const res = claudeHook("Write", { file_path: join(r.a, "crates", "x.rs"), content: "fn main() {}" });
	expect(res).toEqual({ code: 0, stderr: "", stdout: "" });
});

test("Claude Code hook: Bash commands are judged, other tools are not", () => {
	expect(claudeHook("Bash", { command: "git push origin HEAD:main" }).code).toBe(2);
	expect(claudeHook("Bash", { command: "gh pr merge 5 --squash" }).code).toBe(2);
	expect(claudeHook("Bash", { command: "git stash" }).stderr).toContain("shared by every worktree");
	expect(claudeHook("Bash", { command: "git push -u origin feat/a" }).code).toBe(0);
	expect(claudeHook("Edit", { file_path: join(r.main, "x"), old_string: "a", new_string: "b" }).code).toBe(2);
	expect(claudeHook("Read", { file_path: join(r.main, "x") }).code).toBe(0);
});

test("hook: the maintainer's environment lifts the rule, and --json reports the verdict", () => {
	expect(claudeHook("Bash", { command: "git push origin main" }, { ATLAS_MAINTAINER_OK: "push" }).code).toBe(0);
	const payload = JSON.stringify({ cwd: r.a, tool_name: "Bash", tool_input: { command: "git stash" } });
	const p = Bun.spawnSync([process.execPath, entry, "hook", "--json"], { cwd: r.a, stdin: new TextEncoder().encode(payload), env: { ...process.env, ATLAS_MAINTAINER_OK: "" } });
	expect(p.exitCode).toBe(2);
	expect(JSON.parse(p.stdout.toString())).toMatchObject({ allow: false, action: "stash" });
});

test("hook: garbage on stdin is no opinion, not a crash", () => {
	const p = Bun.spawnSync([process.execPath, entry, "hook"], { cwd: r.a, stdin: new TextEncoder().encode("not json") });
	expect(p.exitCode).toBe(0);
});

test("Codex hook: an apply_patch that touches the main checkout exits 2", () => {
	const patch = `*** Begin Patch\n*** Update File: ${join(r.main, "README.md")}\n@@\n-a\n+b\n*** End Patch`;
	expect(claudeHook("apply_patch", { command: patch }).code).toBe(2);
	expect(claudeHook("apply_patch", { command: patch.replace(r.main, r.a) }).code).toBe(0);
});

test("omp extension: tool_call is blocked with the same message, allowed otherwise", async () => {
	let handler: ((e: unknown, c: { cwd?: string }) => Promise<unknown>) | undefined;
	agentGuard({ on: (_event, h) => void (handler = h) });
	const call = (e: unknown) => (handler as NonNullable<typeof handler>)(e, { cwd: r.a });
	expect(await call({ toolName: "write", input: { path: join(r.main, "x.rs"), content: "" } })).toMatchObject({ block: true, reason: expect.stringContaining("is in the main checkout") });
	expect(await call({ toolName: "bash", input: { command: "git stash pop" } })).toMatchObject({ block: true });
	expect(await call({ toolName: "write", input: { path: join(r.a, "x.rs"), content: "" } })).toBeUndefined();
	expect(await call({ toolName: "read", input: { path: join(r.main, "x.rs") } })).toBeUndefined();
});

// The git-level backstop, with real git and a real remote.
const gitDescribe = process.platform === "win32" ? test.skip : test;

gitDescribe("pre-push hook: refuses main, release/*, force-push and deletes through real git; feature pushes pass", () => {
	const remote = join(r.base, "origin.git");
	mkdirSync(remote);
	git(remote, "init", "-q", "--bare", "-b", "main");
	git(r.main, "remote", "add", "origin", remote);
	git(r.main, "config", "core.hooksPath", hooksDir);
	const push = (cwd: string, args: string[], env: Record<string, string> = {}) => {
		const p = Bun.spawnSync(["git", "push", ...args], { cwd, env: { ...process.env, ...GIT_ENV, ATLAS_MAINTAINER_OK: "", ...env } });
		return { code: p.exitCode, stderr: p.stderr.toString() };
	};

	const toMain = push(r.a, ["origin", "HEAD:main"]);
	expect(toMain.code).not.toBe(0);
	expect(toMain.stderr).toContain("refused: push to main");
	expect(push(r.a, ["origin", "HEAD:release/1.5"]).code).not.toBe(0);

	expect(push(r.a, ["-u", "origin", "feat/a"]).code).toBe(0);

	// Rewrite the pushed commit: a force-push.
	writeFileSync(join(r.a, "f.txt"), "1");
	git(r.a, "add", "f.txt");
	git(r.a, "commit", "-q", "-m", "one");
	expect(push(r.a, ["origin", "feat/a"]).code).toBe(0);
	git(r.a, "commit", "-q", "--amend", "-m", "one, rewritten");
	const forced = push(r.a, ["--force", "origin", "feat/a"]);
	expect(forced.code).not.toBe(0);
	expect(forced.stderr).toContain("force-push");
	expect(push(r.a, ["--force", "origin", "feat/a"], { ATLAS_MAINTAINER_OK: "force-push" }).code).toBe(0);

	const del = push(r.a, ["origin", "--delete", "feat/a"]);
	expect(del.code).not.toBe(0);
	expect(del.stderr).toContain("deleting");
	expect(push(r.a, ["origin", "--delete", "feat/a"], { ATLAS_MAINTAINER_OK: "delete" }).code).toBe(0);

	expect(push(r.a, ["origin", "HEAD:main"], { ATLAS_MAINTAINER_OK: "push" }).code).toBe(0);
});
