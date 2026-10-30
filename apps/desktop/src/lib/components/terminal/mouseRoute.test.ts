// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { mouseRoute } from "./mouseRoute.ts";

const mods = (over = {}) => ({ shiftKey: false, metaKey: false, ...over });

// A TUI that turned on mouse reporting (vim, tmux, an agent UI) owns clicks; ⌘ is the app's gesture
// (links, file paths) and ⇧ hands the selection back to the terminal (Ghostty mouse-shift-capture).
test("a program that owns the mouse gets plain clicks; ⇧ and ⌘ stay with Atlas", () => {
	expect(mouseRoute(true, mods())).toBe("program");
	expect(mouseRoute(true, mods({ shiftKey: true }))).toBe("terminal");
	expect(mouseRoute(true, mods({ metaKey: true }))).toBe("terminal");
	expect(mouseRoute(false, mods())).toBe("terminal");
});
