// The diff base decides what "changed" means for check:changed, the CI plan job and the ratchets. A wrong base either
// hides a change (nothing runs, a broken PR merges) or blames a PR for main's code. These tests run real git.
import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { lines, parseChanged, resolveBase, showAt } from "./base.ts";

const made: string[] = [];
afterAll(() => made.forEach((d) => rmSync(d, { recursive: true, force: true })));

function git(dir: string, ...args: string[]): string {
	const p = Bun.spawnSync(["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", ...args], { cwd: dir });
	if (p.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${p.stderr.toString()}`);
	return p.stdout.toString().trim();
}
const put = (dir: string, file: string, text: string) => {
	mkdirSync(join(dir, file, ".."), { recursive: true });
	writeFileSync(join(dir, file), text);
};
const commit = (dir: string, msg: string) => {
	git(dir, "add", "-A");
	git(dir, "commit", "-q", "-m", msg);
	return git(dir, "rev-parse", "HEAD");
};

/** A repo whose `main` has one commit, plus an `origin/main` ref pointing at it. */
function repo() {
	const dir = mkdtempSync(join(tmpdir(), "base-"));
	made.push(dir);
	git(dir, "init", "-q", "-b", "main");
	put(dir, "a.txt", "a\n");
	const c0 = commit(dir, "c0");
	git(dir, "update-ref", "refs/remotes/origin/main", c0);
	return { dir, c0 };
}

describe("parsing git's lines", () => {
	test("CRLF output (Windows git, autocrlf) gives the same paths as LF", () => {
		const lf = parseChanged("M\ta.ts\nA\tb.ts\nD\tc.ts\nR100\told.ts\tnew.ts\n", "u.ts\n");
		const crlf = parseChanged("M\ta.ts\r\nA\tb.ts\r\nD\tc.ts\r\nR100\told.ts\tnew.ts\r\n", "u.ts\r\n");
		expect(crlf).toEqual(lf);
		expect(lines("x\r\n\r\ny\n")).toEqual(["x", "y"]);
	});

	test("touched has every path the change affects: deleted files and the old side of a rename too", () => {
		const p = parseChanged("M\ta.ts\nD\tgone.ts\nR090\tcrates/a/x.rs\tcrates/b/x.rs\nA\tn.ts\n", "u.ts\n");
		expect(p.touched).toEqual(["a.ts", "crates/a/x.rs", "crates/b/x.rs", "gone.ts", "n.ts", "u.ts"]);
		expect([...p.changed]).toEqual([
			["a.ts", "a.ts"],
			["crates/b/x.rs", "crates/a/x.rs"],
			["n.ts", undefined],
			["u.ts", undefined],
		]);
	});
});

describe("locally", () => {
	test("the merge-base with origin/main, and committed, staged, unstaged and untracked changes all count", () => {
		const { dir, c0 } = repo();
		// main moves on after the branch was cut: its file must not be blamed on the branch.
		git(dir, "checkout", "-q", "-b", "work");
		put(dir, "committed.ts", "1");
		commit(dir, "work");
		git(dir, "update-ref", "refs/remotes/origin/main", c0);
		put(dir, "staged.ts", "1");
		git(dir, "add", "staged.ts");
		put(dir, "a.txt", "edited\n");
		put(dir, "untracked.ts", "1");
		const b = resolveBase(dir, {})!;
		expect(b.mode).toBe("origin/main");
		expect(b.mb).toBe(c0);
		expect(b.touched).toEqual(["a.txt", "committed.ts", "staged.ts", "untracked.ts"]);
	});

	test("origin/main moved ahead: the base is the fork point, not the tip", () => {
		const { dir, c0 } = repo();
		git(dir, "checkout", "-q", "-b", "work");
		put(dir, "mine.ts", "1");
		commit(dir, "mine");
		git(dir, "checkout", "-q", "main");
		put(dir, "theirs.ts", "1");
		const tip = commit(dir, "theirs");
		git(dir, "update-ref", "refs/remotes/origin/main", tip);
		git(dir, "checkout", "-q", "work");
		const b = resolveBase(dir, {})!;
		expect(b.mb).toBe(c0);
		expect(b.touched).toEqual(["mine.ts"]);
	});

	test("no origin/main: falls back to main; a repo with neither shares no history: no base", () => {
		const { dir } = repo();
		git(dir, "update-ref", "-d", "refs/remotes/origin/main");
		git(dir, "checkout", "-q", "-b", "work");
		put(dir, "x.ts", "1");
		expect(resolveBase(dir, {})?.mode).toBe("main");
		git(dir, "branch", "-q", "-m", "work", "other");
		git(dir, "branch", "-q", "-D", "main");
		expect(resolveBase(dir, {})).toBeUndefined();
	});

	test("an explicit base wins; an unknown one is an error, not a quiet fallback; all zeros means unset", () => {
		const { dir, c0 } = repo();
		put(dir, "b.txt", "b");
		const c1 = commit(dir, "c1");
		put(dir, "c.txt", "c");
		expect(resolveBase(dir, {}, c0)?.touched).toEqual(["b.txt", "c.txt"]);
		expect(resolveBase(dir, { ATLAS_BASE: c1 })?.touched).toEqual(["c.txt"]);
		expect(resolveBase(dir, {}, c0)?.mode).toBe("explicit");
		expect(() => resolveBase(dir, {}, "no-such-ref")).toThrow("no-such-ref");
		expect(resolveBase(dir, {}, "0".repeat(40))?.mode).toBe("origin/main");
	});

	test("a subdirectory of a work tree is not a root: no base", () => {
		const { dir } = repo();
		mkdirSync(join(dir, "sub"));
		expect(resolveBase(join(dir, "sub"), {})).toBeUndefined();
		expect(resolveBase(join(tmpdir(), "definitely-not-a-repo-xyz"), {})).toBeUndefined();
	});

	test("the same checkout reached through a symlink (Windows short name, /var -> /private/var) resolves alike", () => {
		const { dir } = repo();
		const link = `${dir}-link`;
		symlinkSync(dir, link);
		made.push(link);
		put(dir, "x.ts", "1");
		expect(resolveBase(link, {})?.touched).toEqual(["x.ts"]);
	});

	test("paths with non-ASCII characters are not quoted", () => {
		const { dir } = repo();
		put(dir, "docs/ação.md", "1");
		put(dir, "docs/tracked.md", "1");
		git(dir, "add", "docs/tracked.md");
		expect(resolveBase(dir, {})?.touched).toEqual(["docs/ação.md", "docs/tracked.md"]);
	});
});

describe("in CI", () => {
	test("a pull request is judged against its merge ref's first parent, not the base sha in the event", () => {
		const { dir, c0 } = repo();
		git(dir, "checkout", "-q", "-b", "pr");
		put(dir, "pr.ts", "1");
		commit(dir, "pr");
		git(dir, "checkout", "-q", "main");
		put(dir, "main-later.ts", "1");
		const tip = commit(dir, "main moved on");
		// What actions/checkout builds for pull_request: a merge of the PR into the current main.
		git(dir, "checkout", "-q", "--detach", tip);
		git(dir, "merge", "-q", "--no-ff", "-m", "merge ref", "pr");
		const env = { GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "pull_request" };
		const b = resolveBase(dir, env)!;
		expect(b.mode).toBe("pull_request");
		expect(b.ref).toBe(tip);
		// The event's base.sha (c0) would have listed main-later.ts too.
		expect(b.touched).toEqual(["pr.ts"]);
		expect(git(dir, "diff", "--name-only", c0, "HEAD").split("\n")).toContain("main-later.ts");
	});

	test("a pull request checkout that is not a merge commit has no base: everything runs", () => {
		const { dir } = repo();
		put(dir, "x.ts", "1");
		commit(dir, "x");
		expect(resolveBase(dir, { GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "pull_request" })).toBeUndefined();
	});

	function eventFile(before: string) {
		const f = join(mkdtempSync(join(tmpdir(), "event-")), "event.json");
		made.push(join(f, ".."));
		writeFileSync(f, JSON.stringify({ before }));
		return f;
	}

	test("a push is judged against the commit it replaced", () => {
		const { dir, c0 } = repo();
		put(dir, "one.ts", "1");
		const c1 = commit(dir, "one");
		put(dir, "two.ts", "1");
		commit(dir, "two");
		const env = (before: string) => ({ GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "push", GITHUB_EVENT_PATH: eventFile(before) });
		expect(resolveBase(dir, env(c1))).toMatchObject({ mode: "push", mb: c1, touched: ["two.ts"] });
		expect(resolveBase(dir, env(c0))?.touched).toEqual(["one.ts", "two.ts"]);
	});

	test("a new branch (all-zero before), an unknown before, an unreadable event, workflow_dispatch: no base, never origin/main", () => {
		const { dir } = repo();
		put(dir, "x.ts", "1");
		commit(dir, "x");
		const push = { GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "push" };
		expect(resolveBase(dir, { ...push, GITHUB_EVENT_PATH: eventFile("0".repeat(40)) })).toBeUndefined();
		expect(resolveBase(dir, { ...push, GITHUB_EVENT_PATH: eventFile("f".repeat(40)) })).toBeUndefined();
		expect(resolveBase(dir, { ...push, GITHUB_EVENT_PATH: join(dir, "missing.json") })).toBeUndefined();
		// origin/main exists here and HEAD is on it: used locally, but a dispatch must not read "nothing changed".
		expect(resolveBase(dir, { GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "workflow_dispatch" })).toBeUndefined();
	});
});

describe("showAt", () => {
	test("a file as it was at a commit; undefined when it did not exist", () => {
		const { dir, c0 } = repo();
		put(dir, "a.txt", "new\n");
		commit(dir, "edit");
		expect(showAt(dir, c0, "a.txt")).toBe("a\n");
		expect(showAt(dir, c0, "nope.txt")).toBeUndefined();
	});
});
