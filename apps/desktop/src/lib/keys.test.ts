// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	DEFAULT_KEYMAP,
	bindingsOf,
	defaultKeymap,
	kbd,
	modClick,
	label,
	merge,
	normalize,
	rebind,
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
test("off macOS a focused terminal keeps plain Ctrl+letter for the program", () => {
	const ev = (code: string, m = {}) => ({ code, ctrlKey: false, shiftKey: false, altKey: false, ...m });
	for (const code of ["KeyC", "KeyR", "KeyL", "KeyW", "KeyK", "KeyT"]) {
		expect(shellKeepsCtrl(ev(code, { ctrlKey: true }), false)).toBe(true);
	}
	// The app still gets Ctrl+Shift chords, Ctrl+digits, Ctrl+Tab and Ctrl+`.
	expect(shellKeepsCtrl(ev("KeyT", { ctrlKey: true, shiftKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Digit1", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Tab", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("Backquote", { ctrlKey: true }), false)).toBe(false);
	expect(shellKeepsCtrl(ev("KeyR", { ctrlKey: true, altKey: true }), false)).toBe(false);
	// macOS app shortcuts are ⌘ chords: nothing to keep.
	expect(shellKeepsCtrl(ev("KeyC", { ctrlKey: true }), true)).toBe(false);
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
	expect(label(bindingsOf("shell::ToggleSidebar", m)[0], false)).toBe("Alt+Shift+S");
	expect(label(bindingsOf("shell::Space1", m)[0], false)).toBe("Alt+1");
	expect(label(bindingsOf("atlas::RecentWorktree1", m)[0], false)).toBe("Alt+Shift+1");
	expect(label(bindingsOf("pane::ActivateItem1", m)[0], false)).toBe("Ctrl+1");
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
