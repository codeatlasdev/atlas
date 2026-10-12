// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { DEFAULT_KEYMAP, bindingsOf, label, merge, normalize, rebind, strokeOf, unbind } from "./keys.ts";

test("keystrokes normalize modifier order, aliases and shifted characters", () => {
	expect(normalize("shift-cmd-P")).toBe("shift-cmd-p");
	expect(normalize("cmd-shift-p")).toBe("shift-cmd-p");
	expect(normalize("cmd-?")).toBe("shift-cmd-/");
	expect(normalize("option-super-y")).toBe("alt-cmd-y");
	expect(normalize("cmd--")).toBe("cmd--");
	expect(normalize("hyper-x")).toBeNull();
});

test("key events map by physical key", () => {
	const ev = (code: string, m: Partial<KeyboardEvent> = {}) =>
		({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...m }) as KeyboardEvent;
	expect(strokeOf(ev("Slash", { metaKey: true, shiftKey: true }))).toBe(normalize("cmd-?"));
	expect(strokeOf(ev("Backquote", { ctrlKey: true }))).toBe("ctrl-`");
	expect(strokeOf(ev("KeyY", { altKey: true, metaKey: true }))).toBe("alt-cmd-y");
	expect(strokeOf(ev("ShiftLeft", { shiftKey: true }))).toBeNull();
});

test("user keymap overrides, rebinds and unbinds defaults", () => {
	const m = merge(DEFAULT_KEYMAP, {
		"cmd-j": null,
		"cmd-shift-j": "workspace::ToggleBottomDock",
		"nope-x": "x",
	});
	expect(m.get("cmd-j")).toBeUndefined();
	expect(m.get("shift-cmd-j")).toBe("workspace::ToggleBottomDock");
	expect(m.get("cmd-b")).toBe("workspace::ToggleLeftDock");
	expect(label("shift-cmd-p")).toBe("⇧⌘P");
});

test("rebind moves a command to a new key and takes it from its previous owner", () => {
	const r = rebind(DEFAULT_KEYMAP, {}, "workspace::ToggleBottomDock", "cmd-shift-j");
	expect(r.displaced).toBeUndefined();
	const m = merge(DEFAULT_KEYMAP, r.keymap);
	expect(m.get("shift-cmd-j")).toBe("workspace::ToggleBottomDock");
	expect(m.get("cmd-j")).toBeUndefined();

	const steal = rebind(DEFAULT_KEYMAP, {}, "workspace::ToggleBottomDock", "cmd-b");
	expect(steal.displaced).toBe("workspace::ToggleLeftDock");
	const m2 = merge(DEFAULT_KEYMAP, steal.keymap);
	expect(m2.get("cmd-b")).toBe("workspace::ToggleBottomDock");
	expect(bindingsOf("workspace::ToggleLeftDock", m2)).toEqual([]);
});

test("rebind replaces earlier user entries regardless of modifier order", () => {
	const user = { "shift-cmd-j": "workspace::ToggleBottomDock", "cmd-j": null };
	const r = rebind(DEFAULT_KEYMAP, user, "workspace::ToggleBottomDock", "cmd-shift-k");
	const m = merge(DEFAULT_KEYMAP, r.keymap);
	expect(bindingsOf("workspace::ToggleBottomDock", m)).toEqual(["shift-cmd-k"]);
	expect(rebind(DEFAULT_KEYMAP, {}, "x", "hyper-x")).toBeNull();
});

test("unbind removes every binding, defaults included", () => {
	const m = merge(DEFAULT_KEYMAP, unbind(DEFAULT_KEYMAP, {}, "workspace::ToggleLeftDock"));
	expect(bindingsOf("workspace::ToggleLeftDock", m)).toEqual([]);
	expect(m.get("cmd-r")).toBe("workspace::ToggleRightDock");
});

test("default keymap binds every stroke once and has the shell keys", () => {
	const strokes = Object.keys(DEFAULT_KEYMAP).map((k) => normalize(k));
	expect(strokes.every(Boolean)).toBe(true);
	expect(new Set(strokes).size).toBe(strokes.length);
	const m = merge(DEFAULT_KEYMAP, {});
	expect(m.get("cmd-s")).toBe("workspace::Save");
	expect(m.get("alt-cmd-s")).toBe("workspace::SaveAll");
	expect(m.get("ctrl-cmd-s")).toBe("shell::ToggleSidebar");
	expect(label("ctrl-cmd-s")).toBe("⌃⌘S");
	expect(m.get("cmd-[")).toBe("shell::Back");
	expect(m.get("alt-cmd-right")).toBe("shell::NextSpace");
	expect(m.get("cmd-w")).toBe("pane::CloseActiveItem");
	expect(m.get("shift-cmd-t")).toBe("pane::ReopenClosedItem");
	expect(m.get("shift-cmd-a")).toBe("atlas::NewAgentSession");
	expect(m.get("ctrl-tab")).toBe("pane::ActivateRecentItem");
	expect(m.get("cmd-9")).toBe("pane::ActivateLastItem");
	expect(m.get("ctrl-cmd-1")).toBe("atlas::RecentWorktree1");
	expect(m.get("shift-cmd-r")).toBe("atlas::RestartService");
	expect(m.get("cmd-r")).toBe("workspace::ToggleRightDock");
	expect(m.get("ctrl-9")).toBe("shell::Space9");
	expect(label("alt-cmd-left")).toBe("⌥⌘←");
});

test("⌘1…8 and ⌘9 are tab shortcuts, worktrees moved to ⌃⌘", () => {
	const m = merge(DEFAULT_KEYMAP, {});
	for (let n = 1; n <= 8; n++) expect(m.get(`cmd-${n}`)).toBe(`pane::ActivateItem${n}`);
	for (let n = 1; n <= 6; n++) expect(m.get(`ctrl-cmd-${n}`)).toBe(`atlas::RecentWorktree${n}`);
	expect(label("ctrl-cmd-1")).toBe("⌃⌘1");
	expect(strokeOf({ code: "Tab", ctrlKey: true, altKey: false, shiftKey: true, metaKey: false })).toBe(
		"ctrl-shift-tab",
	);
});
