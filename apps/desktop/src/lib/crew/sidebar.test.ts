// @ts-nocheck
import { describe, expect, test } from "bun:test";
import {
	FAVORITE_KEY,
	favoriteOf,
	groupSections,
	loadFavorites,
	saveFavorites,
	visibleSections,
	withFavorite,
} from "./sidebar.ts";

const e = (hostKey: string, id: string, project?: string) => ({
	hostKey,
	crewmate: { id, project: project ? { name: project } : undefined },
});
const servers = [
	{ key: "ssh:a", name: "alpha" },
	{ key: "ssh:b", name: "beta" },
];

describe("favorites storage", () => {
	test("round-trips through localStorage under crew:favorite", () => {
		const store: Record<string, string> = {};
		const storage = {
			getItem: (k: string) => store[k] ?? null,
			setItem: (k: string, v: string) => (store[k] = v),
		};
		saveFavorites(new Map([["ssh:a", "c1"]]), storage);
		expect(Object.keys(store)).toEqual([FAVORITE_KEY]);
		expect([...loadFavorites(storage)]).toEqual([["ssh:a", "c1"]]);
	});

	test("missing, corrupt or unavailable storage is an empty map", () => {
		expect(loadFavorites({ getItem: () => null }).size).toBe(0);
		expect(loadFavorites({ getItem: () => "{not json" }).size).toBe(0);
		expect(loadFavorites(undefined).size).toBe(0);
	});

	test("a failing setItem does not throw", () => {
		const storage = {
			setItem: () => {
				throw new Error("quota");
			},
		};
		expect(() => saveFavorites(new Map([["ssh:a", "c1"]]), storage)).not.toThrow();
	});

	test("withFavorite replaces the server's pick, moves it last, and leaves the input alone", () => {
		const before = new Map([
			["ssh:a", "c1"],
			["ssh:b", "c9"],
		]);
		const after = withFavorite(before, "ssh:a", "c2");
		expect([...after]).toEqual([
			["ssh:b", "c9"],
			["ssh:a", "c2"],
		]);
		expect([...before]).toEqual([
			["ssh:a", "c1"],
			["ssh:b", "c9"],
		]);
	});
});

describe("favoriteOf", () => {
	const all = [e("ssh:a", "c1"), e("ssh:a", "c2"), e("ssh:b", "c3")];

	test("the first stored favorite that still exists is the hero", () => {
		const favs = new Map([
			["ssh:a", "gone"],
			["ssh:b", "c3"],
		]);
		expect(favoriteOf(favs, all)).toBe(all[2]);
	});

	test("with no usable favorite the first crewmate is the hero", () => {
		expect(favoriteOf(new Map(), all)).toBe(all[0]);
		expect(favoriteOf(new Map([["ssh:a", "gone"]]), all)).toBe(all[0]);
	});

	test("an empty crew has no hero", () => {
		expect(favoriteOf(new Map([["ssh:a", "c1"]]), [])).toBeNull();
	});

	test("the same crewmate id on another server is not the favorite", () => {
		const two = [e("ssh:a", "c1"), e("ssh:b", "c1")];
		expect(favoriteOf(new Map([["ssh:b", "c1"]]), two)).toBe(two[1]);
	});
});

describe("groupSections", () => {
	test("groups by server then project, keeping first-seen order; no project is the null group", () => {
		const s = groupSections(
			[e("ssh:a", "c1", "web"), e("ssh:b", "c2"), e("ssh:a", "c3", "web"), e("ssh:a", "c4")],
			servers,
		);
		expect(s.map((x) => [x.hostKey, x.hostName])).toEqual([
			["ssh:a", "alpha"],
			["ssh:b", "beta"],
		]);
		expect(s[0].projects.map((p) => [p.projectName, p.crewmates.map((c) => c.crewmate.id)])).toEqual([
			["web", ["c1", "c3"]],
			[null, ["c4"]],
		]);
	});

	test("an unknown server is named 'This computer'", () => {
		expect(groupSections([e("", "c1")], servers)[0].hostName).toBe("This computer");
	});

	test("no crewmates, no sections", () => {
		expect(groupSections([], servers)).toEqual([]);
	});
});

describe("visibleSections", () => {
	test("drops hidden crewmates and then empty projects and servers", () => {
		const sections = groupSections([e("ssh:a", "c1", "web"), e("ssh:a", "c2"), e("ssh:b", "c3")], servers);
		const shown = [e("ssh:a", "c2")];
		const v = visibleSections(sections, shown);
		expect(v.map((s) => s.hostKey)).toEqual(["ssh:a"]);
		expect(v[0].projects.map((p) => p.projectName)).toEqual([null]);
	});

	test("shows everything when nothing is hidden", () => {
		const all = [e("ssh:a", "c1", "web"), e("ssh:b", "c3")];
		expect(visibleSections(groupSections(all, servers), all)).toEqual(groupSections(all, servers));
	});
});
