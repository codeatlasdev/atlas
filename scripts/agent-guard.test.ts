import { describe, expect, test } from "bun:test";
import { mkdirSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { realCtx } from "./lib/agent-guard/context";
import { type Call, decide } from "./lib/agent-guard/decide";
import { dirLink, git, makeRepo } from "./lib/agent-guard/fixture";
import { fromHook, fromOmp } from "./lib/agent-guard/payload";
import { decidePush, parsePushLines } from "./lib/agent-guard/prepush";
import { parseShell } from "./lib/agent-guard/shell";

// The guard decides on a real repository (primary checkout + two linked worktrees), because the rules are about where
// paths really land: symlinks, `..`, other worktrees. The session runs in worktree `a`, like an agent does.
const r = makeRepo();
const ctxA = (env: Record<string, string> = {}) => realCtx(r.a, env);
const write = (path: string, ctx = ctxA()) => decide({ kind: "write", paths: [path] }, ctx);
const bash = (command: string, ctx = ctxA()) => decide({ kind: "bash", command }, ctx);

describe("file writes", () => {
	test("inside your own worktree is allowed, existing or new, however deep", () => {
		expect(write(join(r.a, "sub", "x.rs")).allow).toBe(true);
		expect(write(join(r.a, "new", "deeper", "x.rs")).allow).toBe(true);
		expect(write("relative/x.rs").allow).toBe(true); // relative to the session cwd
		expect(write("sub/../x.rs").allow).toBe(true);
	});

	test("the main checkout is refused and the message names the path to use instead", () => {
		const v = write(join(r.main, "sub", "x.rs"));
		expect(v.allow).toBe(false);
		if (v.allow) return;
		expect(v.action).toBe("write");
		expect(v.message).toContain(`refused: ${join(r.main, "sub", "x.rs")} is in the main checkout ${r.main}`);
		expect(v.message).toContain(`use the absolute path under your worktree: ${join(r.a, "sub", "x.rs")}`);
	});

	test("another agent's worktree is refused", () => {
		const v = write(join(r.b, "x.rs"));
		expect(v.allow).toBe(false);
		if (!v.allow) expect(v.message).toContain(`is in another worktree ${r.b}`);
	});

	test("`..` that climbs out of the worktree is refused, lexically and through a relative path", () => {
		expect(write(`${r.a}/../atlas/x.rs`).allow).toBe(false);
		expect(write("../atlas-wt-b/x.rs").allow).toBe(false);
		expect(write("sub/../../atlas/x.rs").allow).toBe(false);
	});

	test("a symlink inside the worktree that points at the main checkout does not launder the write", () => {
		dirLink(r.main, join(r.a, "viamain"));
		expect(write(join(r.a, "viamain", "x.rs")).allow).toBe(false);
		expect(write("viamain/sub/x.rs").allow).toBe(false);
	});

	test("`link/..` goes to the link target's parent, not back into the worktree", () => {
		dirLink(join(r.main, "sub"), join(r.a, "viasub"));
		// Lexically viasub/../x.rs is <a>/x.rs; the kernel resolves it to <main>/x.rs.
		expect(write(`${r.a}/viasub/../x.rs`).allow).toBe(false);
	});

	test.skipIf(process.platform === "win32")("a dangling symlink to a main-checkout file is followed", () => {
		symlinkSync(join(r.main, "not-yet.rs"), join(r.a, "dangling.rs"));
		expect(write(join(r.a, "dangling.rs")).allow).toBe(false);
	});

	test("a symlink that stays inside the worktree is fine", () => {
		dirLink(join(r.a, "sub"), join(r.a, "alias"));
		expect(write(join(r.a, "alias", "x.rs")).allow).toBe(true);
	});

	test("temp dirs are allowed, an unrelated directory is not", () => {
		expect(write(join(tmpdir(), "guard-scratch", "x.txt")).allow).toBe(true);
		const v = write(join(homedir(), "Documents", "notes.txt"), { ...ctxA(), scratch: [] });
		expect(v.allow).toBe(false);
		if (!v.allow) expect(v.message).toContain(`outside your worktree ${r.a}`);
	});

	test("git's own data in the worktree (hooks, config) is not edited as a file", () => {
		expect(write(join(r.a, ".git")).allow).toBe(false);
		expect(write(join(r.main, ".git", "hooks", "pre-push")).allow).toBe(false);
	});

	test("the maintainer override lifts exactly the write rule, per action or all", () => {
		expect(write(join(r.main, "x.rs"), ctxA({ ATLAS_MAINTAINER_OK: "write" })).allow).toBe(true);
		expect(write(join(r.main, "x.rs"), ctxA({ ATLAS_MAINTAINER_OK: "push,merge" })).allow).toBe(false);
		expect(write(join(r.main, "x.rs"), ctxA({ ATLAS_MAINTAINER_OK: "all" })).allow).toBe(true);
	});

	test("session in the primary checkout: free by default, refused after `git config atlas.agentWrites deny`", () => {
		const inMain = () => realCtx(r.main, {});
		expect(write(join(r.main, "x.rs"), inMain()).allow).toBe(true);
		git(r.main, "config", "atlas.agentWrites", "deny");
		try {
			const v = write(join(r.main, "sub", "x.rs"), inMain());
			expect(v.allow).toBe(false);
			if (!v.allow) expect(v.message).toContain("git worktree add -b <type/short-desc>");
			expect(write(join(r.a, "x.rs"), inMain()).allow).toBe(true); // a worktree is where the work goes
			expect(bash(`echo x > ${join(r.main, "y.txt")}`, inMain()).allow).toBe(false);
			expect(bash("git commit -am x", inMain()).allow).toBe(false);
			expect(bash(`git -C ${r.main} worktree add -b feat/c ${join(r.base, "c")} origin/main`, inMain()).allow).toBe(true);
		} finally {
			git(r.main, "config", "--unset", "atlas.agentWrites");
		}
	});

	test("outside a git repository the guard has no opinion", () => {
		expect(write(join(homedir(), "x"), realCtx(tmpdir(), {})).allow).toBe(true);
	});
});

const denied: [string, string][] = [
	// pushes to main and release branches
	["git push origin main", "push"],
	["git push origin HEAD:main", "push"],
	["git push origin feat/a:release/1.5", "push"],
	["git push origin refs/heads/main", "push"],
	["git push -u origin master", "push"],
	["git push --all", "push"],
	["git push --mirror origin", "push"],
	["git push --no-verify origin feat/a", "push"],
	[`git -C ${r.main} push`, "push"], // current branch there is main
	["git -c core.hooksPath=/dev/null push origin feat/a", "push"],
	// force
	["git push --force", "force-push"],
	["git push -f origin feat/a", "force-push"],
	["git push --force-with-lease origin feat/a", "force-push"],
	["git push origin +feat/a", "force-push"],
	["git push -fu origin feat/a", "force-push"],
	// delete
	["git push --delete origin feat/a", "delete"],
	["git push origin :feat/a", "delete"],
	["git push origin --delete v1.0.0", "delete"],
	["git branch -D feat/a", "delete"],
	["git branch -d feat/a", "delete"],
	["git branch --delete feat/a", "delete"],
	["git tag -d v1.0.0", "delete"],
	["git tag -f v1.0.0", "delete"],
	["git update-ref -d refs/heads/feat/a", "delete"],
	["gh api -X DELETE repos/codeatlasdev/atlas/git/refs/heads/feat/a", "delete"],
	// merge
	["gh pr merge 5 --squash", "merge"],
	// the repo's own merge script runs `gh pr merge` in a subprocess, invisible in the command text
	["bun run pr:merge 5", "merge"],
	["bun run pr:merge 5 --squash", "merge"],
	["bun scripts/pr-merge.ts 5", "merge"],
	["npm run pr:merge -- 5", "merge"],
	["cd . && bun run pr:merge 5", "merge"],
	["gh pr merge --admin 5", "merge"],
	["gh pr merge --auto --squash", "merge"],
	["gh api -X PUT repos/codeatlasdev/atlas/pulls/5/merge", "merge"],
	["gh api graphql -f query='mutation { mergePullRequest(input: {pullRequestId: \"x\"}) { clientMutationId } }'", "merge"],
	// stash
	["git stash", "stash"],
	["git stash pop", "stash"],
	["git stash push -u -m wip", "stash"],
	["git stash apply", "stash"],
	["git stash drop", "stash"],
	// tampering with the guards
	["git config core.hooksPath /dev/null", "config"],
	["git config --global core.hooksPath ''", "config"],
	["git config --unset atlas.agentWrites", "config"],
	// writes outside the worktree through the shell
	[`echo hi > ${join(r.main, "a.txt")}`, "write"],
	[`echo hi >> ${join(r.main, "a.txt")}`, "write"],
	[`cargo test 2>&1 | tee ${join(r.main, "log.txt")}`, "write"],
	[`echo hi | tee -a ${join(r.b, "log.txt")}`, "write"],
	[`cat <<EOF > ${join(r.main, "a.txt")}\nbody\nEOF`, "write"],
	[`echo hi &> ${join(r.main, "a.txt")}`, "write"],
	["echo hi > ../atlas/a.txt", "write"],
	[`cp x ${join(r.main, "y")}`, "write"],
	[`cp -t ${r.main} x`, "write"],
	[`mv a ${join(r.main, "b")}`, "write"],
	[`rm -rf ${join(r.main, "crates")}`, "write"],
	[`rm -rf ${r.main}/crates/*.rs`, "write"],
	[`mkdir -p ${join(r.b, "d")}`, "write"],
	[`touch ${join(r.main, "t")}`, "write"],
	[`dd if=/dev/zero of=${join(r.main, "z")} count=1`, "write"],
	[`cd ${r.main} && ls`, "write"],
	[`pushd ${r.main}`, "write"],
	[`cd ${r.b}`, "write"],
	[`cd ${r.main}/sub && git add -A`, "write"],
	[`git -C ${r.main} commit -am x`, "write"],
	[`git -C ${r.main} checkout -b y`, "write"],
	[`cd ${r.main}`, "write"],
	// the same commands hidden one level down
	["bash -c 'git push origin main'", "push"],
	["sh -lc \"gh pr merge 5\"", "merge"],
	["FOO=1 BAR=2 git push origin main", "push"],
	["env git push origin main", "push"],
	["sudo -u root git stash", "stash"],
	["nice -n 15 git push --force", "force-push"],
	["atlas run --heavy -- git push origin main", "push"],
	["echo $(git push origin main)", "push"],
	["echo `git stash`", "stash"],
	["true && git push origin main", "push"],
	["false || git stash", "stash"],
	["ls; git push origin main", "push"],
	["eval 'git push origin main'", "push"],
	["git -C . push origin main", "push"],
	["git --no-pager -c color.ui=never push origin main", "push"],
	["/usr/bin/git push origin main", "push"],
];

const allowed = [
	"git push",
	"git push -u origin feat/a",
	"git push origin feat/a",
	"git push origin HEAD:feat/a",
	"git push origin HEAD",
	"git push --tags",
	"git push origin v1.0.0",
	"git fetch origin main",
	"git pull --rebase origin main",
	"git checkout -b feat/c origin/main",
	"git commit -s -m 'fix: x'",
	"git stash list",
	"git stash show",
	"git branch -a",
	"git branch --list 'feat/*'",
	"git branch --show-current",
	"git tag -l",
	"git tag -a v1.0.0 -m x",
	"git config --get core.hooksPath",
	"git config user.name",
	"git log --oneline -3",
	`git -C ${r.a} status`,
	`git -C ${r.main} log -1`,
	`git -C ${r.main} fetch -q origin`,
	`git -C ${r.main} worktree add -b feat/c ${join(r.base, "c")} origin/main`,
	"gh pr create --draft --title x --body y",
	"gh pr view 5",
	"gh pr checks 5",
	"gh pr ready 5",
	"gh api repos/codeatlasdev/atlas/pulls/5",
	"gh run view 1 --json status",
	"bun run check",
	"bun run pr:merge 5 --dry-run",
	"bun scripts/pr-merge.ts 5 --dry-run",
	"cargo build 2>&1 | tail -5",
	"echo hi > out.txt",
	"echo hi >> sub/out.txt",
	`echo hi > ${join(r.a, "out.txt")}`,
	"echo hi > /dev/null 2>&1",
	"echo hi 1>&2",
	"echo hi >&2",
	"cat <(echo a) > /dev/null",
	`echo hi > ${join(tmpdir(), "x.txt")}`,
	"echo hi > $TMPDIR/x.txt", // cannot be resolved statically: no opinion
	`echo hi | tee ${join(tmpdir(), "x.txt")}`,
	"cp x y",
	`cp ${join(r.main, "README.md")} ${join(r.a, "README.md")}`, // reading from the main checkout is fine
	`rm -rf ${join(r.a, "target")}`,
	`rm -rf ${join(tmpdir(), "x")}`,
	`mkdir -p ${join(r.a, "d")} && cd ${join(r.a, "d")} && ls`,
	`cd ${r.a}/sub && git add -A`,
	`cd ${tmpdir()} && ls`,
	`cd ${r.a}`,
	`cat ${join(r.main, "README.md")}`,
	`ls ${r.main}`,
	// words that only mention the dangerous commands
	"git commit -m 'git push origin main'",
	'echo "gh pr merge 5"',
	"grep -rn 'git stash' docs",
	"echo git push --force",
	`cat <<'EOF' > notes.md\ngit push origin main\ngit stash\ngh pr merge 5\nEOF`,
	"# git push origin main",
	"rsync -a host:/srv/x ./x",
	"scp file host:/srv/x",
];

describe("shell commands", () => {
	for (const [command, action] of denied) {
		test(`denied (${action}): ${command.replace(/\n/g, "\\n")}`, () => {
			const v = bash(command);
			expect(v.allow).toBe(false);
			if (!v.allow) {
				expect(v.action).toBe(action as typeof v.action);
				expect(v.message).toStartWith("refused:");
			}
		});
	}
	for (const command of allowed) {
		test(`allowed: ${command.replace(/\n/g, "\\n")}`, () => {
			expect(bash(command).allow).toBe(true);
		});
	}

	test("on branch main a bare `git push` goes to main and is refused", () => {
		const onMain = realCtx(r.main, {});
		expect(decide({ kind: "bash", command: "git push" }, onMain).allow).toBe(false);
		expect(decide({ kind: "bash", command: "git push origin" }, onMain).allow).toBe(false);
		expect(decide({ kind: "bash", command: "git push origin feat/x" }, onMain).allow).toBe(true);
	});

	test("an agent cannot give itself the override, with or without the real one", () => {
		for (const cmd of ["ATLAS_MAINTAINER_OK=all git push origin main", "export ATLAS_MAINTAINER_OK=push; git push origin main", "env ATLAS_MAINTAINER_OK=merge gh pr merge 5"]) {
			for (const env of [{}, { ATLAS_MAINTAINER_OK: "all" }]) {
				const v = bash(cmd, ctxA(env));
				expect(v.allow).toBe(false);
				if (!v.allow) expect(v.action).toBe("self-override");
			}
		}
	});

	test("the maintainer's override lifts only the named action", () => {
		const push = ctxA({ ATLAS_MAINTAINER_OK: "push" });
		expect(bash("git push origin main", push).allow).toBe(true);
		expect(bash("git push --force origin feat/a", push).allow).toBe(false);
		expect(bash("git stash", push).allow).toBe(false);
		expect(bash("gh pr merge 5", ctxA({ ATLAS_MAINTAINER_OK: "merge, stash" })).allow).toBe(true);
		expect(bash("git stash", ctxA({ ATLAS_MAINTAINER_OK: "merge, stash" })).allow).toBe(true);
		expect(bash("git push origin main && git stash", ctxA({ ATLAS_MAINTAINER_OK: "all" })).allow).toBe(true);
	});

	test("a denial says what to do instead", () => {
		const m = (c: string) => {
			const v = bash(c);
			return v.allow ? "" : v.message;
		};
		expect(m("git push origin main")).toContain("gh pr create --draft");
		expect(m("git stash")).toContain("git diff > /tmp/x.patch");
		expect(m("gh pr merge 5")).toContain("a maintainer merges it");
		expect(m("git push -f")).toContain("git push");
		expect(m("git push origin main")).toContain("ATLAS_MAINTAINER_OK=push");
		expect(m(`cd ${r.main}`)).toContain(`Stay in ${r.a}`);
	});

	test("`cd` moves the directory that later relative writes and `git -C` resolve against", () => {
		expect(bash(`cd ${join(r.a, "sub")} && echo hi > ../ok.txt`).allow).toBe(true);
		expect(bash(`cd ${join(r.a, "sub")} && echo hi > ../../atlas/x.txt`).allow).toBe(false);
		expect(bash(`cd ${tmpdir()} && echo hi > x.txt`).allow).toBe(true);
	});
});

describe("shell reader", () => {
	test("splits on operators, strips quotes and finds redirect targets", () => {
		expect(parseShell(`a b && c 'd e' "f g" ; h | i`).map((s) => s.words)).toEqual([["a", "b"], ["c", "d e", "f g"], ["h"], ["i"]]);
		expect(parseShell("echo hi 2>err.log >> out.log >| clob 2>&1 <in")).toEqual([{ words: ["echo", "hi"], writes: ["err.log", "out.log", "clob"] }]);
		expect(parseShell("echo a\\ b")[0].words).toEqual(["echo", "a b"]);
		expect(parseShell("echo $(git stash)")).toEqual([{ words: ["git", "stash"], writes: [] }, { words: ["echo", ""], writes: [] }]);
	});

	test("heredoc bodies are data, the command after them is read again", () => {
		const segs = parseShell("cat <<EOF > f.txt\ngit stash\nEOF\ngit push");
		expect(segs.map((s) => s.words)).toEqual([["cat"], ["git", "push"]]);
		expect(segs[0].writes).toEqual(["f.txt"]);
	});
});

describe("harness payloads", () => {
	test("Claude Code: Write, Edit, MultiEdit, NotebookEdit and Bash", () => {
		const tool = (tool_name: string, tool_input: unknown) => fromHook({ tool_name, tool_input, cwd: "/x" });
		expect(tool("Write", { file_path: "/a/b", content: "" })).toEqual({ kind: "write", paths: ["/a/b"] });
		expect(tool("Edit", { file_path: "/a/b" })).toEqual({ kind: "write", paths: ["/a/b"] });
		expect(tool("MultiEdit", { file_path: "/a/b", edits: [] })).toEqual({ kind: "write", paths: ["/a/b"] });
		expect(tool("NotebookEdit", { notebook_path: "/a/n.ipynb" })).toEqual({ kind: "write", paths: ["/a/n.ipynb"] });
		expect(tool("Bash", { command: "ls" })).toEqual({ kind: "bash", command: "ls" });
		expect(tool("Read", { file_path: "/a/b" })).toBeUndefined();
		expect(fromHook("not an object")).toBeUndefined();
	});

	test("Codex apply_patch lists every file of the patch, moves included", () => {
		const patch = "*** Begin Patch\n*** Add File: a.txt\n+x\n*** Update File: b/c.rs\n*** Move to: d/c.rs\n@@\n-x\n+y\n*** Delete File: e.rs\n*** End Patch";
		expect(fromHook({ tool_name: "apply_patch", tool_input: { command: patch } })).toEqual({ kind: "write", paths: ["a.txt", "b/c.rs", "d/c.rs", "e.rs"] });
	});

	test("omp: write, edit (path, paths, hashline headers) and bash with its cwd", () => {
		expect(fromOmp({ toolName: "write", input: { path: "/a/b", content: "" } })).toEqual({ kind: "write", paths: ["/a/b"] });
		expect(fromOmp({ toolName: "edit", input: { paths: ["/a/b", "/a/c"] } })).toEqual({ kind: "write", paths: ["/a/b", "/a/c"] });
		expect(fromOmp({ toolName: "edit", input: { input: "[/a/b.rs#1A2B]\nPUT 1.=1:\n+x" } })).toEqual({ kind: "write", paths: ["/a/b.rs"] });
		expect(fromOmp({ toolName: "bash", input: { command: "ls", cwd: "sub" } })).toEqual({ kind: "bash", command: "ls", cwd: "sub" });
		expect(fromOmp({ toolName: "read", input: { path: "/a" } })).toBeUndefined();
	});

	test("a tool call's own cwd is where its relative paths and commands run", () => {
		const call: Call = { kind: "bash", command: "echo hi > x.txt", cwd: r.main };
		expect(decide(call, ctxA()).allow).toBe(false);
		expect(decide({ kind: "bash", command: "echo hi > x.txt", cwd: join(r.a, "sub") }, ctxA()).allow).toBe(true);
	});
});

describe("git pre-push decisions", () => {
	const sha = (c: string) => c.repeat(40);
	const ZERO = "0".repeat(40);
	const never = () => false;
	const always = () => true;
	const line = (remoteRef: string, local = sha("a"), remote = ZERO) => `refs/heads/x ${local} ${remoteRef} ${remote}`;
	const push = (lines: string[], isAncestor = always, env: Record<string, string> = {}) => decidePush(parsePushLines(lines.join("\n") + "\n"), ctxA(env), isAncestor);

	test("a new or fast-forwarded feature branch passes", () => {
		expect(push([line("refs/heads/feat/a")]).allow).toBe(true);
		expect(push([line("refs/heads/feat/a", sha("b"), sha("a"))]).allow).toBe(true);
		expect(push([line("refs/tags/v1.0.0")]).allow).toBe(true); // a new tag is a release, which a maintainer cuts
	});

	test("main, master and release/* are refused, even as a fast-forward and even as a deletion", () => {
		for (const ref of ["refs/heads/main", "refs/heads/master", "refs/heads/release/1.5"]) {
			expect(push([line(ref, sha("b"), sha("a"))]).allow).toBe(false);
			expect(push([line(ref, ZERO, sha("a"))]).allow).toBe(false);
		}
		expect(push([line("refs/heads/mainline")]).allow).toBe(true);
		expect(push([line("refs/heads/releases")]).allow).toBe(true);
	});

	test("a push that drops remote commits is a force-push, a push that contains them is not", () => {
		const v = push([line("refs/heads/feat/a", sha("b"), sha("a"))], never);
		expect(v.allow).toBe(false);
		if (!v.allow) expect(v.action).toBe("force-push");
		expect(push([line("refs/heads/feat/a", sha("b"), sha("a"))], always).allow).toBe(true);
	});

	test("deleting a remote branch or moving a tag is refused, creating one is not", () => {
		const del = push([line("refs/heads/feat/a", ZERO, sha("a"))]);
		expect(del.allow).toBe(false);
		if (!del.allow) expect(del.action).toBe("delete");
		expect(push([line("refs/tags/v1.0.0", sha("b"), sha("a"))]).allow).toBe(false);
		expect(push([line("refs/tags/v1.0.0", ZERO, sha("a"))]).allow).toBe(false);
	});

	test("one bad ref among good ones refuses the push; the override lifts the named action", () => {
		const lines = [line("refs/heads/feat/a"), line("refs/heads/main", sha("b"), sha("a"))];
		expect(push(lines).allow).toBe(false);
		expect(push(lines, always, { ATLAS_MAINTAINER_OK: "push" }).allow).toBe(true);
		expect(push(lines, always, { ATLAS_MAINTAINER_OK: "force-push" }).allow).toBe(false);
		expect(push([line("refs/heads/feat/a", sha("b"), sha("a"))], never, { ATLAS_MAINTAINER_OK: "force-push" }).allow).toBe(true);
	});

	test("garbage on stdin is no refs", () => {
		expect(parsePushLines("")).toEqual([]);
		expect(parsePushLines("one two\n\n")).toEqual([]);
	});
});

// Keep the fixture honest: the worktrees really are linked and on their own branches.
test("fixture: a and b are linked worktrees of main", () => {
	expect(git(r.a, "rev-parse", "--abbrev-ref", "HEAD")).toBe("feat/a");
	expect(realCtx(r.a, {}).tree()?.primary).toBe(false);
	expect(realCtx(r.main, {}).tree()?.primary).toBe(true);
	writeFileSync(join(r.a, "marker"), "");
	mkdirSync(join(r.a, "sub"), { recursive: true });
});
