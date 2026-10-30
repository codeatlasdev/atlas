// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { shortcutOrder } from "./botOrder.ts";

// ⌘1…⌘9 in the Bots workspace pick the bot at that position in the sidebar, so the order must be what the sidebar
// shows: the first mate (hero) on top, then the list grouped by server and project, not the store's own order.
describe("shortcutOrder", () => {
	const b = (hostKey: string, id: string) => ({ hostKey, bot: { id } });

	test("the first mate is 1, then the list as grouped on screen", () => {
		const hero = b("ship", "carlos");
		const sections = [
			{ projects: [{ bots: [b("ship", "ana"), b("ship", "bia")] }, { bots: [b("ship", "joao")] }] },
			{ projects: [{ bots: [b("local", "zed")] }] },
		];
		expect(shortcutOrder(hero, sections).map((x) => x.botId)).toEqual(["carlos", "ana", "bia", "joao", "zed"]);
	});

	test("the first mate is not listed twice, and with no first mate the list starts at 1", () => {
		const hero = b("ship", "ana");
		const sections = [{ projects: [{ bots: [b("ship", "ana"), b("ship", "bia")] }] }];
		expect(shortcutOrder(hero, sections).map((x) => x.botId)).toEqual(["ana", "bia"]);
		expect(shortcutOrder(null, sections).map((x) => x.botId)).toEqual(["ana", "bia"]);
	});

	test("the same bot id on two servers is two entries", () => {
		const sections = [{ projects: [{ bots: [b("a", "x"), b("b", "x")] }] }];
		expect(shortcutOrder(null, sections)).toEqual([
			{ hostKey: "a", botId: "x" },
			{ hostKey: "b", botId: "x" },
		]);
	});
});
