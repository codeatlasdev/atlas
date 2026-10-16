// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { branchRows, createRow, parseBranches, parseSwitched, relDate } from "./branches.ts";
import branchesFixture from "./__fixtures__/git-branches.json";
import switchFixture from "./__fixtures__/git-switch.json";

// The fixtures are the REAL serialization of the Rust values (written by `git_branch::tests`), never a hand-made mock.
const real = parseBranches(branchesFixture);

test("real Rust payload parses: current first, locals before remotes, every row has the fields the picker reads", () => {
	expect(real.current).toBe("main");
	const kinds = real.branches.map((b) => b.kind);
	expect(kinds.lastIndexOf("local")).toBeLessThan(kinds.indexOf("remote"));
	for (const b of real.branches) {
		expect(typeof b.name).toBe("string");
		expect(typeof b.current).toBe("boolean");
		expect(Number.isNaN(Date.parse(b.date))).toBe(false);
	}
	expect(real.branches.find((b) => b.name === "busy").worktree.path).toBe("/work/repo-busy");
});

test("an object that is not a branch list is rejected loudly, not rendered empty", () => {
	expect(() => parseBranches({ branches: [{ name: "x" }] })).toThrow(/branch/i);
	expect(() => parseBranches(null)).toThrow();
});

test("real switch results parse for each outcome", () => {
	const [ok, dirty, wt] = switchFixture.map(parseSwitched);
	expect(ok).toEqual({ status: "ok", branch: "feat", stashed: false });
	expect(dirty.status).toBe("dirty");
	expect(dirty.message).toContain("local changes");
	expect(wt).toEqual({ status: "worktree", path: "/work/repo-busy" });
	expect(() => parseSwitched({ status: "boom" })).toThrow();
});

test("picker rows keep Rust's order with no query and mark the current branch", () => {
	const rows = branchRows(real, "", { now: Date.parse("2026-01-02T00:00:00+00:00") });
	expect(rows.map((r) => r.name)).toEqual(real.branches.map((b) => b.name));
	expect(rows[0].current).toBe(true);
	expect(rows[0].description).toContain("current");
});

test("typing filters fuzzily over local and remote names, best match first", () => {
	const rows = branchRows(real, "fe");
	expect(rows.map((r) => r.name)).toContain("feat");
	expect(rows.map((r) => r.name)).not.toContain("busy");
	expect(rows[0].name).toBe("feat");
});

test("a remote branch with no local twin is found by typing and marked remote", () => {
	const only = {
		...real,
		branches: [
			...real.branches,
			{ ...real.branches[0], name: "origin/only-remote", kind: "remote", current: false, local: null },
		],
	};
	const rows = branchRows(only, "only");
	expect(rows.map((r) => [r.name, r.kind])).toEqual([["origin/only-remote", "remote"]]);
});

test("a remote branch that has a local twin is hidden when typing so the twin is not offered twice", () => {
	const rows = branchRows(real, "busy");
	expect(rows.filter((r) => r.name.endsWith("busy")).map((r) => r.name)).toEqual(["busy"]);
});

test("a branch checked out in another worktree says where, and is flagged to jump", () => {
	const row = branchRows(real, "busy").find((r) => r.name === "busy");
	expect(row.jump).toBe("/work/repo-busy");
	expect(row.description).toContain("repo-busy");
});

test("relDate renders the last commit age compactly and survives garbage", () => {
	const now = Date.parse("2026-01-10T12:00:00Z");
	expect(relDate("2026-01-10T11:59:40Z", now)).toBe("now");
	expect(relDate("2026-01-10T11:30:00Z", now)).toBe("30m ago");
	expect(relDate("2026-01-10T07:00:00Z", now)).toBe("5h ago");
	expect(relDate("2026-01-07T12:00:00Z", now)).toBe("3d ago");
	expect(relDate("2025-10-10T12:00:00Z", now)).toBe("3mo ago");
	expect(relDate("2023-01-10T12:00:00Z", now)).toBe("3y ago");
	expect(relDate("not a date", now)).toBe("");
});

test("createRow offers a new branch only for a valid name that does not exist yet", () => {
	expect(createRow(real, "feature/login", "main")?.label).toBe("Create branch “feature/login”");
	expect(createRow(real, "feature/login", "main")?.description).toBe("from main");
	expect(createRow(real, "feat", "main")).toBeNull(); // exists locally
	expect(createRow(real, "origin/feat", "main")).toBeNull(); // exists as remote
	expect(createRow(real, "", "main")).toBeNull();
	expect(createRow(real, "-oops", "main")).toBeNull(); // would be read as a git option
	expect(createRow(real, "has space", "main")).toBeNull();
	expect(createRow(real, "a..b", "main")).toBeNull();
	expect(createRow(real, "x.lock", "main")).toBeNull();
	expect(createRow(real, "/lead", "main")).toBeNull();
	expect(createRow(real, "trail/", "main")).toBeNull();
	expect(createRow(real, "feature/login", null)?.description).toBe("from the current commit"); // detached
});
