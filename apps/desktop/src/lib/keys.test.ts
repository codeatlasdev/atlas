// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	DEFAULT_KEYMAP,
	bindingsOf,
	defaultKeymap,
	fileManager,
	kbd,
	modClick,
	label,
	merge,
	normalize,
	rebind,
	revealLabel,
	shellKeepsCtrl,
	strokeOf,
	strokesOf,
	unbind,
} from "./keys.ts";

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
	expect(m.get("shift-cmd-w")).toBe("pane::CloseActiveItem");
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

test("⌃- and ⌃⇧- are Go Back / Go Forward over jump positions, distinct from ⌘[ ⌘] tab history", () => {
	const m = merge(DEFAULT_KEYMAP, {});
	expect(m.get(normalize("ctrl--"))).toBe("editor::GoBack");
	expect(m.get(normalize("ctrl-shift--"))).toBe("editor::GoForward");
	expect(m.get("cmd-[")).toBe("shell::Back");
});

// Incident (Windows, first run): every shortcut was bound to ⌘ (the Meta key), which on Windows and Linux is the
// system's own Windows/Super key, so no app shortcut worked and every hint read `⌘K`. Off macOS, Ctrl takes ⌘'s
// place (as in VS Code and Zed) and labels read `Ctrl+K`.
test("off macOS, Ctrl is the primary modifier: Ctrl+K is the ⌘K binding", () => {
	const ev = (code: string, m: Partial<KeyboardEvent> = {}) =>
		({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...m }) as KeyboardEvent;
	expect(strokeOf(ev("KeyK", { ctrlKey: true }), false)).toBe("cmd-k");
	expect(strokeOf(ev("KeyP", { ctrlKey: true, shiftKey: true }), false)).toBe("shift-cmd-p");
	expect(strokeOf(ev("KeyS", { ctrlKey: true, altKey: true }), false)).toBe("alt-cmd-s");
	// The Windows key is the system's: it never reaches a binding.
	expect(strokeOf(ev("KeyK", { metaKey: true }), false)).toBeNull();
	// macOS is unchanged.
	expect(strokeOf(ev("KeyK", { ctrlKey: true }), true)).toBe("ctrl-k");
	expect(strokeOf(ev("KeyK", { metaKey: true }), true)).toBe("cmd-k");
});

test("off macOS a Ctrl chord also reaches a binding written with ctrl, after the ⌘ one", () => {
	const ev = (code: string, m: Partial<KeyboardEvent> = {}) =>
		({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...m }) as KeyboardEvent;
	expect(strokesOf(ev("Tab", { ctrlKey: true }), false)).toEqual(["cmd-tab", "ctrl-tab"]);
	expect(strokesOf(ev("Backquote", { ctrlKey: true }), false)).toEqual(["cmd-`", "ctrl-`"]);
	expect(strokesOf(ev("KeyK", { metaKey: true }), true)).toEqual(["cmd-k"]);
	expect(strokesOf(ev("KeyA"), false)).toEqual(["a"]);
	const m = merge(DEFAULT_KEYMAP, {});
	const run = (e: KeyboardEvent) =>
		strokesOf(e, false)
			.map((s) => m.get(s))
			.find(Boolean);
	expect(run(ev("KeyK", { ctrlKey: true }))).toBe("atlas::Jump");
	expect(run(ev("Tab", { ctrlKey: true }))).toBe("pane::ActivateRecentItem");
	expect(run(ev("Backquote", { ctrlKey: true }))).toBe("terminal_panel::ToggleFocus");
	expect(run(ev("KeyP", { ctrlKey: true, shiftKey: true }))).toBe("command_palette::Toggle");
});

test("labels read Ctrl/Alt/Shift words off macOS and glyphs on macOS", () => {
	expect(label("shift-cmd-p", false)).toBe("Ctrl+Shift+P");
	expect(label("cmd-k", false)).toBe("Ctrl+K");
	expect(label("alt-cmd-s", false)).toBe("Ctrl+Alt+S");
	expect(label("ctrl-tab", false)).toBe("Ctrl+Tab");
	expect(label("ctrl-`", false)).toBe("Ctrl+`");
	expect(label("alt-cmd-left", false)).toBe("Ctrl+Alt+←");
	expect(label("cmd--", false)).toBe("Ctrl+-");
	expect(label("escape", false)).toBe("Esc");
	expect(label("shift-cmd-p", true)).toBe("⇧⌘P");
});

// Off macOS the app's shortcuts are Ctrl chords; a shell needs ^C, ^R, ^L, ^W, ^K… for itself. In a focused terminal a
// plain Ctrl+letter goes to the program; Ctrl+Shift, digit, Tab and punctuation chords still reach the app.
test("off macOS a focused terminal keeps plain Ctrl+letter, Ctrl+[ and Alt+←/→ for the program", () => {
	const ev = (code: string, m = {}) => ({ code, ctrlKey: false, shiftKey: false, altKey: false, ...m });
	for (const code of ["KeyC", "KeyR", "KeyL", "KeyW", "KeyK", "KeyT", "KeyB"]) {
		expect(shellKeepsCtrl(ev(code, { ctrlKey: true }), false), code).toBe(true);
	}
	// Ctrl+[ is Esc in vim and readline; Alt+←/→ are readline word motions.
	expect(shellKeepsCtrl(ev("BracketLeft", { ctrlKey: true }), false)).toBe(true);
	expect(shellKeepsCtrl(ev("ArrowLeft", { altKey: true }), false)).toBe(true);
	expect(shellKeepsCtrl(ev("ArrowRight", { altKey: true }), false)).toBe(true);
	// The app still gets Ctrl+Shift chords (new/close tab), Ctrl+digits, Ctrl+Tab, Ctrl+`, Ctrl+PgUp/PgDn and Ctrl+Q.
	expect(shellKeepsCtrl(ev("KeyT", { ctrlKey: true, shiftKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("KeyW", { ctrlKey: true, shiftKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Backquote", { ctrlKey: true, shiftKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("KeyQ", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("PageDown", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("PageUp", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Digit1", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Tab", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Backquote", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("KeyR", { ctrlKey: true, altKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("F11"), false)).toBe(false);
	// Alt+1…9 (spaces) and Alt+Shift chords are not the shell's; Alt+Shift+←/→ neither.
	expect(shellKeepsCtrl(ev("Digit1", { altKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("ArrowLeft", { altKey: true, shiftKey: true }), false)).toBe(false);
	// macOS app shortcuts are ⌘ chords: nothing to keep.
	expect(shellKeepsCtrl(ev("KeyC", { ctrlKey: true }), true)).toBe(false);
	expect(shellKeepsCtrl(ev("ArrowLeft", { altKey: true }), true)).toBe(false);
});

// Incident (Windows, 2026-10-14): the palette listed "Ctrl+S" for both Save (⌘S) and Toggle sidebar (⌃⌘S), and
// "Ctrl+1" for tab 1 (⌘1), space 1 (⌃1) and recent worktree 1 (⌃⌘1). Off macOS ⌃ and ⌘ are the same key, so only
// one of each could ever run. Every default binding must run from exactly the keys its label names.
test("off macOS every default binding runs from the keys its label names, and no two share a label", () => {
	const CODE: Record<string, string> = {
		"`": "Backquote",
		"\\": "Backslash",
		"[": "BracketLeft",
		"]": "BracketRight",
		",": "Comma",
		".": "Period",
		"/": "Slash",
		"-": "Minus",
		"=": "Equal",
		";": "Semicolon",
		"'": "Quote",
		escape: "Escape",
		enter: "Enter",
		tab: "Tab",
		backspace: "Backspace",
		delete: "Delete",
		space: "Space",
		up: "ArrowUp",
		down: "ArrowDown",
		left: "ArrowLeft",
		right: "ArrowRight",
		home: "Home",
		end: "End",
		pageup: "PageUp",
		pagedown: "PageDown",
	};
	/** The key event a Windows/Linux user makes for a canonical stroke (⌘ and ⌃ are both Ctrl there). */
	const eventFor = (stroke: string) => {
		const parts = stroke.split("-");
		if (stroke.endsWith("--")) parts.splice(-2, 2, "-");
		const key = parts.pop()!;
		const code = /^[a-z]$/.test(key)
			? `Key${key.toUpperCase()}`
			: /^\d$/.test(key)
				? `Digit${key}`
				: /^f\d+$/.test(key)
					? key.toUpperCase()
					: CODE[key];
		const ctrlKey = parts.includes("cmd") || parts.includes("ctrl");
		return {
			code,
			ctrlKey,
			altKey: parts.includes("alt"),
			shiftKey: parts.includes("shift"),
			metaKey: false,
		};
	};
	const m = merge(defaultKeymap(false), {});
	const byLabel = new Map<string, string[]>();
	const wrong: string[] = [];
	for (const [stroke, id] of m) {
		const l = label(stroke, false);
		byLabel.set(l, [...(byLabel.get(l) ?? []), id]);
		const ran = strokesOf(eventFor(stroke) as KeyboardEvent, false)
			.map((s) => m.get(s))
			.find(Boolean);
		if (ran !== id) wrong.push(`${l} runs ${ran}, not ${id}`);
	}
	expect(wrong).toEqual([]);
	expect([...byLabel].filter(([, ids]) => ids.length > 1)).toEqual([]);
	// Where the macOS-only chords went.
	expect(label(bindingsOf("shell::ToggleSidebar", m)[0], false)).toBe("Ctrl+B");
	expect(label(bindingsOf("workspace::ToggleLeftDock", m)[0], false)).toBe("Alt+Shift+B");
	expect(label(bindingsOf("shell::Space1", m)[0], false)).toBe("Alt+1");
	expect(label(bindingsOf("atlas::RecentWorktree1", m)[0], false)).toBe("Alt+Shift+1");
	expect(label(bindingsOf("pane::ActivateItem1", m)[0], false)).toBe("Ctrl+1");
	expect(label(bindingsOf("editor::GoBack", m)[0], false)).toBe("Alt+←");
	expect(m.get("ctrl--")).toBeUndefined();
	expect(m.get("cmd-s")).toBe("workspace::Save");
	// macOS keeps its keymap as written.
	expect(defaultKeymap(true)).toEqual(DEFAULT_KEYMAP);
});

// Incident class (Windows): AltGr arrives as Ctrl+Alt. Typing "ś" (Polish AltGr+S) or "²" (Brazilian AltGr+2) must
// stay text, not run Ctrl+Alt+S (Save all) or a Ctrl+Alt+digit binding.
test("off macOS a Ctrl+Alt chord that types a character is AltGr text, not a shortcut", () => {
	const ev = (code: string, key: string) =>
		({ code, key, ctrlKey: true, altKey: true, shiftKey: false, metaKey: false }) as KeyboardEvent;
	expect(strokesOf(ev("KeyS", "ś"), false)).toEqual([]);
	expect(strokesOf(ev("Digit2", "²"), false)).toEqual([]);
	expect(strokesOf(ev("KeyQ", "@"), false)).toEqual([]);
	// A real Ctrl+Alt chord reports the plain key.
	expect(strokesOf(ev("KeyS", "s"), false)[0]).toBe("alt-cmd-s");
	expect(strokesOf(ev("KeyS", "S"), false)[0]).toBe("alt-cmd-s");
	expect(strokesOf(ev("ArrowLeft", "ArrowLeft"), false)[0]).toBe("alt-cmd-left");
	// macOS has no AltGr: ⌃⌥ chords stay chords.
	expect(strokesOf(ev("KeyS", "ś"), true)).toEqual(["ctrl-alt-s"]);
});

// Incident (Windows): placeholders and hints spelled macOS keys ("⌘Enter to commit", "⎋ close", "⌘-click"), which a
// Windows or Linux keyboard does not have. UI text names keys through `kbd`, which follows the platform like labels.
test("kbd names a key for UI text: glyphs on macOS, Ctrl/Shift/Esc words elsewhere", () => {
	expect(kbd("cmd-l", true)).toBe("⌘L");
	expect(kbd("cmd-l", false)).toBe("Ctrl+L");
	expect(kbd("cmd-enter", false)).toBe("Ctrl+Enter");
	expect(kbd("shift-enter", true)).toBe("⇧↩");
	expect(kbd("shift-enter", false)).toBe("Shift+Enter");
	expect(kbd("escape", true)).toBe("⎋");
	expect(kbd("escape", false)).toBe("Esc");
	expect(kbd("cmd-shift-m", false)).toBe("Ctrl+Shift+M");
	expect(modClick(true)).toBe("⌘-click");
	expect(modClick(false)).toBe("Ctrl+click");
});

// Incident (Windows): "Reveal in Finder" was shown on Windows, where the action opens File Explorer.
test("the reveal action names the platform's file manager", () => {
	expect(revealLabel(true, false)).toBe("Reveal in Finder");
	expect(revealLabel(false, true)).toBe("Reveal in File Explorer");
	expect(revealLabel(false, false)).toBe("Reveal in file manager");
	expect(fileManager(false, true)).toBe("File Explorer");
});

// The shortcut conventions decided for Windows/Linux (Chrome / VS Code / Windows Terminal) and macOS, pinned through the
// real path of a key: the event's strokes, the platform's default keymap, and the terminal's "keep for the shell" rule.
// `ran` is what a key does; `inTerminal` says what it does when a terminal has focus (the capture handler's order).
const key = (code: string, m: Partial<KeyboardEvent> = {}) =>
	({ code, key: "", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...m }) as KeyboardEvent;
const ran = (e: KeyboardEvent, mac: boolean) => {
	const km = merge(defaultKeymap(mac), {});
	return strokesOf(e, mac)
		.map((s) => km.get(s))
		.find(Boolean);
};
const inTerminal = (e: KeyboardEvent, mac: boolean) => (shellKeepsCtrl(e, mac) ? undefined : ran(e, mac));

test("tabs, Windows/Linux: Ctrl+T/W outside terminals, Ctrl+Shift+T/W everywhere including a focused terminal", () => {
	const ctrl = { ctrlKey: true };
	const ctrlShift = { ctrlKey: true, shiftKey: true };
	expect(ran(key("KeyT", ctrl), false)).toBe("workspace::NewTerminal");
	expect(ran(key("KeyW", ctrl), false)).toBe("pane::CloseActiveItem");
	expect(ran(key("KeyT", ctrlShift), false)).toBe("workspace::NewTerminal");
	expect(ran(key("KeyW", ctrlShift), false)).toBe("pane::CloseActiveItem");
	// In a terminal Ctrl+T / Ctrl+W are the shell's (transpose, delete word); Ctrl+Shift+T/W are the app's.
	expect(inTerminal(key("KeyT", ctrl), false)).toBeUndefined();
	expect(inTerminal(key("KeyW", ctrl), false)).toBeUndefined();
	expect(inTerminal(key("KeyT", ctrlShift), false)).toBe("workspace::NewTerminal");
	expect(inTerminal(key("KeyW", ctrlShift), false)).toBe("pane::CloseActiveItem");
});

test("tabs, macOS: ⌘T ⌘W ⇧⌘T as before, and ⇧⌘W closes the tab (never every tab)", () => {
	expect(ran(key("KeyT", { metaKey: true }), true)).toBe("workspace::NewTerminal");
	expect(ran(key("KeyW", { metaKey: true }), true)).toBe("pane::CloseActiveItem");
	expect(ran(key("KeyT", { metaKey: true, shiftKey: true }), true)).toBe("pane::ReopenClosedItem");
	expect(ran(key("KeyW", { metaKey: true, shiftKey: true }), true)).toBe("pane::CloseActiveItem");
	// ⌘ chords always reach the app, terminal or not.
	expect(inTerminal(key("KeyT", { metaKey: true }), true)).toBe("workspace::NewTerminal");
});

test("reopen closed tab moved off Ctrl+Shift+T to Alt+Shift+T off macOS", () => {
	expect(ran(key("KeyT", { altKey: true, shiftKey: true }), false)).toBe("pane::ReopenClosedItem");
	expect(inTerminal(key("KeyT", { altKey: true, shiftKey: true }), false)).toBe("pane::ReopenClosedItem");
	const m = merge(defaultKeymap(false), {});
	expect(label(bindingsOf("pane::ReopenClosedItem", m)[0], false)).toBe("Alt+Shift+T");
	expect(bindingsOf("workspace::NewTerminal", m).map((s) => label(s, false))).toContain("Ctrl+Shift+T");
});

test("no default key closes every tab: Close all stays in the menu and palette", () => {
	for (const mac of [true, false]) {
		const m = merge(defaultKeymap(mac), {});
		expect(bindingsOf("pane::CloseAllItems", m), `mac=${mac}`).toEqual([]);
	}
});

test("Ctrl+Q quits off macOS, from a terminal too; Alt+F4 is not a keymap binding; macOS has no Ctrl+Q", () => {
	expect(ran(key("KeyQ", { ctrlKey: true }), false)).toBe("shell::Quit");
	expect(inTerminal(key("KeyQ", { ctrlKey: true }), false)).toBe("shell::Quit");
	expect(ran(key("F4", { altKey: true }), false)).toBeUndefined();
	expect(ran(key("KeyQ", { ctrlKey: true }), true)).toBeUndefined();
	// ⌘Q is the native menu's on macOS, not a keymap command.
	expect(bindingsOf("shell::Quit", merge(defaultKeymap(true), {}))).toEqual([]);
});

test("Ctrl+PgUp / Ctrl+PgDn walk the tabs off macOS (⌘PgDn stays the GPU terminal's scroll on macOS)", () => {
	expect(ran(key("PageUp", { ctrlKey: true }), false)).toBe("pane::ActivatePreviousItem");
	expect(ran(key("PageDown", { ctrlKey: true }), false)).toBe("pane::ActivateNextItem");
	expect(inTerminal(key("PageDown", { ctrlKey: true }), false)).toBe("pane::ActivateNextItem");
	expect(ran(key("PageDown", { metaKey: true }), true)).toBeUndefined();
});

test("Ctrl+Shift+` opens a terminal (VS Code) on every platform, from a terminal too", () => {
	expect(ran(key("Backquote", { ctrlKey: true, shiftKey: true }), false)).toBe("workspace::NewTerminal");
	expect(inTerminal(key("Backquote", { ctrlKey: true, shiftKey: true }), false)).toBe(
		"workspace::NewTerminal",
	);
	expect(ran(key("Backquote", { ctrlKey: true, shiftKey: true }), true)).toBe("workspace::NewTerminal");
	// Ctrl+` keeps toggling the terminal panel.
	expect(ran(key("Backquote", { ctrlKey: true }), false)).toBe("terminal_panel::ToggleFocus");
});

// Incident class: Ctrl+B was the left dock while VS Code users expect the sidebar. Off macOS Ctrl+B is the sidebar and
// the left dock moved to Alt+Shift+B; macOS keeps ⌘B (left dock) and ⌃⌘S (sidebar).
test("Ctrl+B toggles the sidebar off macOS, the left dock moved to Alt+Shift+B; macOS is unchanged", () => {
	expect(ran(key("KeyB", { ctrlKey: true }), false)).toBe("shell::ToggleSidebar");
	expect(ran(key("KeyB", { altKey: true, shiftKey: true }), false)).toBe("workspace::ToggleLeftDock");
	expect(bindingsOf("shell::ToggleSidebar", merge(defaultKeymap(false), {}))).toEqual(["cmd-b"]);
	expect(ran(key("KeyB", { metaKey: true }), true)).toBe("workspace::ToggleLeftDock");
	expect(ran(key("KeyS", { metaKey: true, ctrlKey: true }), true)).toBe("shell::ToggleSidebar");
	// A focused terminal keeps plain Ctrl+B (tmux prefix, readline back-char).
	expect(inTerminal(key("KeyB", { ctrlKey: true }), false)).toBeUndefined();
});

test("Ctrl+[ and Alt+←/→ reach the shell in a focused terminal, and are history/back elsewhere", () => {
	expect(inTerminal(key("BracketLeft", { ctrlKey: true }), false)).toBeUndefined();
	expect(inTerminal(key("ArrowLeft", { altKey: true }), false)).toBeUndefined();
	expect(inTerminal(key("ArrowRight", { altKey: true }), false)).toBeUndefined();
	expect(ran(key("BracketLeft", { ctrlKey: true }), false)).toBe("shell::Back");
	expect(ran(key("ArrowLeft", { altKey: true }), false)).toBe("editor::GoBack");
	expect(ran(key("ArrowRight", { altKey: true }), false)).toBe("editor::GoForward");
	// macOS: ⌘[ is the app's even in a terminal (⌘ never reaches a shell).
	expect(inTerminal(key("BracketLeft", { metaKey: true }), true)).toBe("shell::Back");
});

test("F11 toggles full screen off macOS only", () => {
	expect(ran(key("F11"), false)).toBe("shell::ToggleFullscreen");
	expect(inTerminal(key("F11"), false)).toBe("shell::ToggleFullscreen");
	expect(ran(key("F11"), true)).toBeUndefined();
});

test("the off-macOS defaults bind every stroke once", () => {
	const strokes = Object.keys(defaultKeymap(false)).map((k) => normalize(k));
	expect(strokes.every(Boolean)).toBe(true);
	expect(new Set(strokes).size).toBe(strokes.length);
});
