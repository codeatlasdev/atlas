// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import prViewOpen from "#lib/__fixtures__/gh/pr-view-open.json";
import { m } from "#lib/i18n/m.ts";
import { childrenOf, matches, prChip, stepLabel, strongest } from "./model.ts";

const p = (id: string, over: Record<string, unknown> = {}) => ({
	id,
	name: id,
	root: `/r/${id}`,
	branch: null,
	parent: null,
	pinned: false,
	...over,
});

test("childrenOf keeps only the repo's worktrees, pinned first", () => {
	const rows = [
		p("a"),
		p("b", { parent: "a" }),
		p("c", { parent: "a", pinned: true }),
		p("d", { parent: "z" }),
	];
	expect(childrenOf(rows, "a").map((r) => r.id)).toEqual(["c", "b"]);
});

test("matches hides archived rows unless shown, and filters on name/branch/worktree/root", () => {
	const row = p("api", { branch: "feat/x" });
	expect(matches(row, { name: "w", archived: true }, "", false)).toBe(false);
	expect(matches(row, { name: "w", archived: true }, "", true)).toBe(true);
	expect(matches(row, undefined, "  FEAT ", false)).toBe(true);
	expect(matches(row, { name: "scratch", archived: false }, "scr", false)).toBe(true);
	expect(matches(row, undefined, "/r/api", false)).toBe(true);
	expect(matches(row, undefined, "nope", false)).toBe(false);
});

test("strongest picks blocked over waiting over working over done over idle", () => {
	expect(strongest([])).toBeNull();
	expect(strongest(["idle", "done"])).toBe("done");
	expect(strongest(["working", "waiting", "idle"])).toBe("waiting");
	expect(strongest(["done", "blocked"])).toBe("blocked");
});

// `worktree://progress` sends the step as a bare id from `create.rs`; every one of them must read as a phrase, not as the id.
test("every progress step the worktree creation reports has a label", () => {
	const src = readFileSync(
		new URL("../../../../../../crates/atlas-desktop/src/worktree/create.rs", import.meta.url),
		"utf8",
	);
	const steps = [...src.matchAll(/progress\("(\w+)",/g)].map(([, step]) => step);
	expect(steps).toEqual(expect.arrayContaining(["fetch", "add", "shared", "seed"]));
	for (const step of new Set(steps)) {
		if (step === "error") continue;
		expect(stepLabel(step), step).not.toBe(step);
	}
	expect(stepLabel("rebase")).toBe("rebase");
});

// The state comes from `gh pr view --json state` (the recorded real answer): GitHub's own upper-case value.
test("the chip words the pull request state gh reports, and shows an unknown one as it came", () => {
	const view = JSON.parse(prViewOpen.result.stdout);
	expect(prChip(497, view.state)).toBe(m.sidebar_worktree_row_pr_state({ number: 497, state: m.sidebar_worktree_row_pr_merged() }));
	expect(prChip(7, "OPEN")).toBe("PR #7 · open");
	expect(prChip(7, "CLOSED")).toBe("PR #7 · closed");
	expect(prChip(7, "QUEUED")).toBe("PR #7 · queued");
	expect(prChip(7, undefined)).toBe("PR #7");
});
