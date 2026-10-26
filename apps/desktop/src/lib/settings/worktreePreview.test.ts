// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { worktreePreview } from "./worktreePreview.ts";

// Mirrors `crates/atlas-desktop/src/worktree/create.rs`: the folder is `<workspaceDir>/<repo>/<name>` (or
// `<workspaceDir>/<name>` when `nestWorkspaces` is "false"), the branch is `<prefix>/<name>`, and the prefix is the
// slugified git user name, the slugified custom text, or nothing. The diagram must say what Atlas will do.
const base = { dir: "", nest: true, prefix: "git-username", custom: "" };

describe("where a new worktree lands", () => {
	test("the default folder is ~/.atlas/worktrees, grouped by repository", () => {
		expect(worktreePreview(base).path).toEqual(["~/.atlas/worktrees", "atlas", "fix-login"]);
	});

	test("without grouping the worktree sits directly in the folder", () => {
		expect(worktreePreview({ ...base, nest: false }).path).toEqual(["~/.atlas/worktrees", "fix-login"]);
	});

	test("a folder you chose is shown as typed, minus trailing slashes", () => {
		expect(worktreePreview({ ...base, dir: "~/code/wt//" }).path[0]).toBe("~/code/wt");
		expect(worktreePreview({ ...base, dir: "   " }).path[0]).toBe("~/.atlas/worktrees");
	});
});

describe("the branch Atlas creates", () => {
	test("git name: prefix is a placeholder for your name, since the app cannot know it", () => {
		expect(worktreePreview(base).branch).toBe("<your-git-name>/fix-login");
	});

	test("none: the branch is the bare name", () => {
		expect(worktreePreview({ ...base, prefix: "none" }).branch).toBe("fix-login");
	});

	test("custom text is slugified like the app does (lowercase, dashes)", () => {
		expect(worktreePreview({ ...base, prefix: "custom", custom: "Team Alpha!" }).branch).toBe(
			"team-alpha/fix-login",
		);
	});

	test("an empty or symbol-only custom prefix means no prefix, not a stray slash", () => {
		expect(worktreePreview({ ...base, prefix: "custom", custom: "" }).branch).toBe("fix-login");
		expect(worktreePreview({ ...base, prefix: "custom", custom: " !!! " }).branch).toBe("fix-login");
	});

	test("an unknown prefix mode behaves like the default, as the app does", () => {
		expect(worktreePreview({ ...base, prefix: "weird" }).branch).toBe("<your-git-name>/fix-login");
	});
});
