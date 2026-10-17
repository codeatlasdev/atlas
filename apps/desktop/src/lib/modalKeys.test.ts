// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";

// Incident (codeatlasdev/atlas#66): with Settings open the app shortcuts still ran behind it (⌘B toggled the sidebar,
// ⌘T opened a terminal, ⌘N a worktree dialog on top of Settings). A modal makes the page behind it inert for pointer
// and focus (the kit's `showModal`), but the keymap listens on `window`, ahead of any element, so it ignored it.
// The kit announces layers as a `layerchange` event on `document` (`detail.modal`); the keymap must run nothing while a
// modal is open, from the three ways a command is reached: a key on the window, a stroke forwarded from a native page,
// a native menu item. The kit's real event is pinned in kitOverlays.test.ts, which owns the real-kit setup.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const { register, dispatch, dispatchStroke, runCommand } = await import("./keymap.svelte.ts");
const { startModalWatch } = await import("./modalOpen.ts");

let ran: string[] = [];
register([
	{ id: "workspace::ToggleLeftDock", label: "left", run: () => ran.push("left") },
	{ id: "workspace::NewTerminal", label: "term", run: () => ran.push("term") },
]);
const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { bubbles: true, ...init });
const cmdB = () => key({ key: "b", code: "KeyB", metaKey: true });
/** What the kit dispatches when a layer opens or closes. */
const layers = (...kinds: string[]) =>
	document.dispatchEvent(
		new CustomEvent("layerchange", {
			detail: {
				layers: kinds.map((kind) => ({ kind })),
				count: kinds.length,
				modal: kinds.includes("modal"),
			},
		}),
	);

const cleanups: (() => void)[] = [];
afterEach(() => {
	while (cleanups.length) cleanups.pop()();
	ran = [];
});
const watch = () => cleanups.push(startModalWatch());

test("with no modal open a shortcut runs its command", () => {
	watch();
	expect(dispatch(cmdB())).toBe(true);
	expect(ran).toEqual(["left"]);
});

test("while a modal is open no shortcut, forwarded stroke or menu item runs a command", () => {
	watch();
	layers("modal");
	expect(dispatch(cmdB())).toBe(false);
	expect(dispatchStroke("cmd-b")).toBe(false);
	runCommand("workspace::NewTerminal");
	expect(ran).toEqual([]);
});

test("closing the modal gives the shortcuts back", () => {
	watch();
	layers("modal");
	expect(dispatch(cmdB())).toBe(false);
	layers();
	expect(dispatch(cmdB())).toBe(true);
	expect(ran).toEqual(["left"]);
});

test("a menu or tooltip open over the page is not a modal: shortcuts keep working", () => {
	watch();
	layers("popover", "hint");
	expect(dispatch(cmdB())).toBe(true);
});

test("a menu opened inside a modal does not reopen the gate; the modal closing does", () => {
	watch();
	layers("modal", "popover");
	expect(dispatch(cmdB())).toBe(false);
	layers("modal");
	expect(dispatch(cmdB())).toBe(false);
	layers();
	expect(dispatch(cmdB())).toBe(true);
});

test("stopping the watch reopens the gate even with a modal up (hot reload must not strand it shut)", () => {
	const off = startModalWatch();
	layers("modal");
	expect(dispatch(cmdB())).toBe(false);
	off();
	expect(dispatch(cmdB())).toBe(true);
	layers("modal"); // no listener any more: still open
	expect(dispatch(cmdB())).toBe(true);
});
