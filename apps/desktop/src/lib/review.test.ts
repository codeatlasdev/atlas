// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { hunkPatch, reviewPrompt } from "./review.ts";

test("hunk patch keeps prefixes and uses /dev/null for new files", () => {
	const h = {
		header: "@@ -0,0 +1,2 @@",
		old_start: 0,
		old_lines: 0,
		new_start: 1,
		new_lines: 2,
		lines: [
			{ kind: "add", text: "a" },
			{ kind: "add", text: "" },
		],
	};
	const p = hunkPatch({ path: "x.md", status: "added", staged: false, binary: false, hunks: [h] }, h);
	expect(p).toBe("diff --git a/x.md b/x.md\n--- /dev/null\n+++ b/x.md\n@@ -0,0 +1,2 @@\n+a\n+\n");
});

test("review prompt lists notes one per line", () => {
	const p = reviewPrompt([
		{ id: "1", path: "a.ts", line: 3, side: "new", text: "rename\nthis", resolved: false, createdAt: 0 },
	]);
	expect(p).toBe("Review notes:\n- a.ts:3 (new): rename this\n");
});

// Incident (batch S09): the Changes view got a quoted, escaped path from the parser (`"a/a\303\247..."`), and staging
// a hunk wrote that text into the patch header. The path is the real name now; the patch must still apply, with
// git's own quoting for the characters git would quote. Real `git apply --cached`, real index.
const git = (cwd: string, args: string[], stdin?: string) => {
	const p = Bun.spawnSync(["git", "-C", cwd, "-c", "user.email=a@b", "-c", "user.name=n", ...args], {
		stdin: stdin === undefined ? "ignore" : new TextEncoder().encode(stdin),
	});
	return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString() };
};

const NAMES = [
	"ação.txt",
	"日本.txt",
	"pasta ação/é.md",
	...(process.platform === "win32"
		? []
		: ['name with "quote".txt', "tab\there.txt", "new\nline.txt", "back\\slash.txt"]),
];

test("a hunk of a file with a quote, a tab, a newline or a letter outside ASCII in its name stages with git apply", () => {
	for (const name of NAMES) {
		const dir = mkdtempSync(join(tmpdir(), "atlas-review-"));
		try {
			git(dir, ["init", "-q", "."]);
			const abs = join(dir, name);
			mkdirSync(dirname(abs), { recursive: true });
			writeFileSync(abs, "one\ntwo\n");
			git(dir, ["add", "-A"]);
			git(dir, ["commit", "-qm", "init"]);
			const h = {
				header: "@@ -1,2 +1,3 @@",
				old_start: 1,
				old_lines: 2,
				new_start: 1,
				new_lines: 3,
				lines: [
					{ kind: "ctx", text: "one" },
					{ kind: "ctx", text: "two" },
					{ kind: "add", text: "three" },
				],
			};
			writeFileSync(abs, "one\ntwo\nthree\n");
			const patch = hunkPatch(
				{ path: name, status: "modified", staged: false, binary: false, hunks: [h] },
				h,
			);
			const r = git(dir, ["apply", "--cached", "-"], patch);
			expect({ name, code: r.code, err: r.err }).toEqual({ name, code: 0, err: "" });
			expect(git(dir, ["show", `:${name}`]).out).toBe("one\ntwo\nthree\n");
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	}
});

test("a path git would quote is written quoted in the patch header and the others as they are", () => {
	const h = {
		header: "@@ -1 +1 @@",
		old_start: 1,
		old_lines: 1,
		new_start: 1,
		new_lines: 1,
		lines: [{ kind: "ctx", text: "x" }],
	};
	const patch = (path: string) =>
		hunkPatch({ path, status: "modified", staged: false, binary: false, hunks: [h] }, h);
	expect(patch("ação.txt")).toStartWith("diff --git a/ação.txt b/ação.txt\n--- a/ação.txt\n+++ b/ação.txt\n");
	expect(patch("tab\there.txt")).toStartWith(
		'diff --git "a/tab\\there.txt" "b/tab\\there.txt"\n--- "a/tab\\there.txt"\n',
	);
	expect(patch('q"q')).toStartWith('diff --git "a/q\\"q" "b/q\\"q"\n');
	expect(patch("b\\s")).toStartWith('diff --git "a/b\\\\s" "b/b\\\\s"\n');
	expect(patch("c\x01")).toStartWith('diff --git "a/c\\001" "b/c\\001"\n');
});
