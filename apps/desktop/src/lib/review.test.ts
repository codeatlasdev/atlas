// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
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
