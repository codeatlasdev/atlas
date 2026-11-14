// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { shortcutOrder } from "./crewmateOrder.ts";

// ⌘1…⌘9 in the Crew workspace pick the crewmate at that position in the sidebar, so the order must be what the sidebar
// shows: the first mate (hero) on top, then the list grouped by server and project, not the store's own order.
describe("shortcutOrder", () => {
	const b = (hostKey: string, id: string) => ({ hostKey, crewmate: { id } });

	test("the first mate is 1, then the list as grouped on screen", () => {
		const hero = b("server", "carlos");
		const sections = [
			{
				projects: [
					{ crewmates: [b("server", "ana"), b("server", "bia")] },
					{ crewmates: [b("server", "joao")] },
				],
			},
			{ projects: [{ crewmates: [b("local", "zed")] }] },
		];
		expect(shortcutOrder(hero, sections).map((x) => x.crewmateId)).toEqual([
			"carlos",
			"ana",
			"bia",
			"joao",
			"zed",
		]);
	});

	test("the first mate is not listed twice, and with no first mate the list starts at 1", () => {
		const hero = b("server", "ana");
		const sections = [{ projects: [{ crewmates: [b("server", "ana"), b("server", "bia")] }] }];
		expect(shortcutOrder(hero, sections).map((x) => x.crewmateId)).toEqual(["ana", "bia"]);
		expect(shortcutOrder(null, sections).map((x) => x.crewmateId)).toEqual(["ana", "bia"]);
	});

	test("the same crewmate id on two servers is two entries", () => {
		const sections = [{ projects: [{ crewmates: [b("a", "x"), b("b", "x")] }] }];
		expect(shortcutOrder(null, sections)).toEqual([
			{ hostKey: "a", crewmateId: "x" },
			{ hostKey: "b", crewmateId: "x" },
		]);
	});
});
