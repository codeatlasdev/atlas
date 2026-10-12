// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { buttonState, menuGroups, unavailable } from "./actions.ts";

const a = (key, label, extra = {}) => ({ key, label, group: null, confirm: null, session: false, ...extra });

describe("unavailable", () => {
	test("stopped stack, busy and session actions say why", () => {
		expect(unavailable(a("c", "Check"), false, false)).toContain("Start the stack");
		expect(unavailable(a("c", "Check"), true, true)).toBe("Already running");
		expect(unavailable(a("m", "Mobile", { session: true }), true, false)).toContain("terminal UI");
		expect(unavailable(a("c", "Check"), true, false)).toBeNull();
	});
});

describe("menuGroups", () => {
	const list = [
		a("c", "Check"),
		a("i", "iOS", { group: "Mobile" }),
		a("p", "DB plan"),
		a("d", "DB push", { confirm: "Wipes data" }),
		a("x", "Android", { group: "Mobile" }),
	];
	test("ungrouped first, then groups in order of first appearance", () => {
		const g = menuGroups(list, true, () => false);
		expect(g.map((x) => x.label)).toEqual(["", "Mobile"]);
		expect(g[0].items.map((i) => i.key)).toEqual(["c", "p", "d"]);
		expect(g[1].items.map((i) => i.key)).toEqual(["i", "x"]);
	});
	test("a group that appears first still sorts after ungrouped", () => {
		const g = menuGroups([a("i", "iOS", { group: "Mobile" }), a("c", "Check")], true, () => false);
		expect(g.map((x) => x.label)).toEqual(["", "Mobile"]);
	});
	test("keeps confirm text and flags the busy item", () => {
		const g = menuGroups(list, true, (k) => k === "p");
		expect(g[0].items[2].confirm).toBe("Wipes data");
		expect(g[0].items[1]).toMatchObject({ busy: true, disabled: "Already running" });
	});
	test("everything is disabled with a reason when stopped", () => {
		const g = menuGroups(list, false, () => false);
		expect(g.flatMap((x) => x.items).every((i) => i.disabled?.includes("Start the stack"))).toBe(true);
	});
});

describe("buttonState", () => {
	test("shows the running action's label", () => {
		const items = menuGroups([a("c", "Check"), a("p", "DB plan")], true, (k) => k === "p")[0].items;
		expect(buttonState(items)).toMatchObject({ label: "DB plan" });
		expect(buttonState([]).label).toBe("Actions");
	});
});
