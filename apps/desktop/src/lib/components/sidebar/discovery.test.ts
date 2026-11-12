// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	addable,
	discoverLead,
	elsewhereLabel,
	noProjectsLine,
	pickable,
	tickedByDefault,
	ago,
	groupFound,
	matchesQuery,
	movedCopy,
	normalizeRemote,
	SPOTLIGHT,
	splitFound,
	parsePrefs,
	type Found,
} from "./discovery.ts";

const f = (path: string, over: Partial<Found> = {}): Found => ({
	path,
	root: "/r",
	name: path.split("/").pop()!,
	services: 1,
	error: null,
	branch: null,
	remote: null,
	modified: null,
	has_config: true,
	nested: false,
	stack: null,
	...over,
});

test("query matches name, path, branch and remote case-insensitively", () => {
	const x = f("/r/Billing", { branch: "feat/Pay", remote: "git@github.com:Org/Ledger.git" });
	for (const q of ["bill", "FEAT/pay", "ledger", "/r/"]) expect(matchesQuery(x, q)).toBe(true);
	expect(matchesQuery(x, "zzz")).toBe(false);
	expect(matchesQuery(x, "  ")).toBe(true);
});

test("groups follow first-seen root order, newest first, and honor the filter", () => {
	const list = [
		f("/a/old", { root: "/a", modified: 10 }),
		f("/b/x", { root: "/b", modified: 5 }),
		f("/a/new", { root: "/a", modified: 99 }),
		f("/c/sp/only", { root: "" }),
		f("/d/other", { root: "" }),
	];
	const g = groupFound(list);
	expect(g.map((x) => x.root)).toEqual(["/a", "/b", SPOTLIGHT]);
	expect(g[0].items.map((i) => i.name)).toEqual(["new", "old"]);
	expect(g[2].items.map((i) => i.name)).toEqual(["only", "other"]);
	expect(groupFound(list, "x").map((x) => x.root)).toEqual(["/b"]);
	expect(groupFound(list, "nothing")).toEqual([]);
});

test("candidates are split from real projects", () => {
	const { projects, candidates } = splitFound([f("/a"), f("/b", { has_config: false, stack: "node" })]);
	expect(projects.map((p) => p.path)).toEqual(["/a"]);
	expect(candidates.map((p) => p.path)).toEqual(["/b"]);
});

test("addable excludes registered folders and candidates", () => {
	const list = [f("/a"), f("/b"), f("/c", { has_config: false })];
	expect(addable(list, new Set(["/a"])).map((x) => x.path)).toEqual(["/b"]);
});

// PR #158 review (S5): with no atlas.yaml anywhere (the common case) the dialog said "No projects found" over a full
// "Could use Atlas" list, and "Add selected" stayed disabled because those folders had no checkbox.
test("folders without atlas.yaml can be ticked, but only valid configs are ticked by default", () => {
	const list = [
		f("/a"),
		f("/bad", { error: "x" }),
		f("/c", { has_config: false, stack: "node" }),
		f("/d", { has_config: false }),
	];
	expect(pickable(list, new Set(["/d"])).map((x) => x.path)).toEqual(["/a", "/bad", "/c"]);
	expect(list.filter(tickedByDefault).map((x) => x.path)).toEqual(["/a"]);
});

test("the empty line never says 'no projects' above a list of candidates", () => {
	expect(noProjectsLine("", 3)).toBe("No folder with an atlas.yaml yet. Add one from the list below.");
	expect(noProjectsLine("", 0)).toBe("No projects found in the scanned folders.");
	expect(noProjectsLine("api", 3)).toBe("No project matches “api”.");
});

// PR #158 review (S6): Spotlight is macOS only.
test("Spotlight is only named on macOS", () => {
	expect(discoverLead(true)).toContain("Spotlight");
	expect(discoverLead(false)).not.toContain("Spotlight");
	expect(elsewhereLabel(true)).toContain("Spotlight");
	expect(elsewhereLabel(false)).toBe("Elsewhere");
});

test("remotes normalize across ssh/https/case", () => {
	expect(normalizeRemote("git@github.com:Org/Repo.git")).toBe("github.com/org/repo");
	expect(normalizeRemote("https://user:tok@github.com/org/repo/")).toBe("github.com/org/repo");
	expect(normalizeRemote("ssh://git@github.com/org/repo")).toBe("github.com/org/repo");
});

test("moved copy: remote beats name, ambiguity yields nothing, registered folders are ignored", () => {
	const list = [
		f("/n/renamed", { remote: "https://github.com/org/app" }),
		f("/o/app", { remote: "git@github.com:other/x.git" }),
	];
	expect(movedCopy({ name: "app", remote: "git@github.com:Org/app.git" }, list, new Set())?.path).toBe(
		"/n/renamed",
	);
	expect(movedCopy({ name: "app" }, list, new Set())?.path).toBe("/o/app");
	const dup = [f("/x/app"), f("/y/app")];
	expect(movedCopy({ name: "app" }, dup, new Set())).toBeNull();
	expect(movedCopy({ name: "app" }, dup, new Set(["/y/app"]))?.path).toBe("/x/app");
	expect(movedCopy({ name: "none" }, dup, new Set())).toBeNull();
});

test("ago picks the largest unit", () => {
	const now = 1_000_000_000;
	expect(ago(null, now)).toBe("");
	expect(ago(now - 5, now)).toBe("just now");
	expect(ago(now - 90, now)).toBe("1min ago");
	expect(ago(now - 3 * 3600, now)).toBe("3h ago");
	expect(ago(now - 3 * 86400, now)).toBe("3d ago");
	expect(ago(now - 400 * 86400, now)).toBe("1y ago");
	expect(ago(now + 100, now)).toBe("just now");
});

// Incident: almost nobody has an atlas.yaml, and "Also folders without atlas.yaml" defaulted to off, so a new user's
// first scan showed an empty list. It is on until the user turns it off.
test("folders without atlas.yaml are included by default, and only an explicit off is remembered", () => {
	expect(parsePrefs(null).candidates).toBe(true);
	expect(parsePrefs("{}").candidates).toBe(true);
	expect(parsePrefs("not json").candidates).toBe(true);
	expect(parsePrefs('{"candidates":false}').candidates).toBe(false);
	expect(parsePrefs('{"candidates":true,"roots":["/a",3]}')).toEqual({ roots: ["/a"], candidates: true });
});
