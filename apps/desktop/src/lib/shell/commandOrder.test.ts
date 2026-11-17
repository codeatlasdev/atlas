// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { groupCommands, listedRecents } from "./commandOrder.ts";

const cmds = ["command_palette::Toggle", "a", "b", "c"].map((id) => ({ id }));

// Regression: 1.5.0-alpha.26 first-user pass, "Command palette" was listed under "Recent commands" (the one command
// everyone has just run to get there). It is still recorded: the checklist item "Try the command palette" ticks from it.
test("the palette command is never listed as a recent command, nor floated first", () => {
	const out = groupCommands(cmds, ["command_palette::Toggle", "b"]);
	expect(out.map((c) => [c.id, c.group])).toEqual([
		["b", "Recent commands"],
		["command_palette::Toggle", "Commands"],
		["a", "Commands"],
		["c", "Commands"],
	]);
});

test("recents come first in recency order, the rest keep their order; ids that no longer exist are ignored", () => {
	const out = groupCommands(cmds, ["c", "gone", "a"]);
	expect(out.map((c) => c.id)).toEqual(["c", "a", "command_palette::Toggle", "b"]);
	expect(out.filter((c) => c.group === "Recent commands").map((c) => c.id)).toEqual(["c", "a"]);
	expect(listedRecents(["command_palette::Toggle"])).toEqual([]);
});

test("no recents: everything is under Commands in registry order", () => {
	expect(groupCommands(cmds, []).every((c) => c.group === "Commands")).toBe(true);
	expect(groupCommands(cmds, []).map((c) => c.id)).toEqual(cmds.map((c) => c.id));
});
