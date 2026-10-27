// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeAll, expect, mock, test } from "bun:test";

// The window's capture handler (`onKeydownCapture`) with real DOM focus: which element counts as "a terminal has focus"
// and what the handler does with the event. The per-platform keymap decisions (Windows/Linux vs macOS, terminal focused
// or not) are pinned in keys.test.ts through `defaultKeymap(mac)` and `shellKeepsCtrl(e, mac)`, which take the platform
// explicitly; here the suite's platform (macOS, see test/setup.ts) is what `onKeydownCapture` reads from `keys.ts`.
//
// Module mocks must be in place before the modules under test load, which hoisted static imports cannot do: hence the
// dynamic imports below (same pattern as modalKeys.test.ts).
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
// The real `window` module links `SERIALIZE_TO_IPC_FN` from `api/core`, which the `core` mock above lacks.
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
const { register } = await import("../keymap.svelte.ts");
const { IS_MAC, defaultKeymap } = await import("../keys.ts");
const { onKeydownCapture } = await import("./windowKeys.ts");

let ran: string[] = [];
beforeAll(() => {
	// Every command id the platform's defaults bind, so a key that resolves to one runs it here.
	const ids = new Set(Object.values(defaultKeymap(IS_MAC)));
	register([...ids].map((id) => ({ id, label: id, run: () => ran.push(id) })));
});
afterEach(() => {
	ran = [];
	document.body.innerHTML = "";
});

const press = (code: string, init: KeyboardEventInit = {}) => {
	const e = new KeyboardEvent("keydown", { code, key: "", bubbles: true, cancelable: true, ...init });
	onKeydownCapture(e);
	return e;
};
const focus = (html: string) => {
	document.body.innerHTML = html;
	document.getElementById("t").focus();
};
/** xterm.js's hidden textarea inside `.xterm` (every terminal off macOS, remote ones on macOS). */
const focusXterm = () => focus(`<div class="xterm"><textarea id="t"></textarea></div>`);
/** The GPU terminal's input sink (macOS): `data-term-tab` is set by GpuTerminal.svelte. */
const focusGpuSink = () => focus(`<textarea id="t" data-term-tab="t1"></textarea>`);

test("macOS, GPU terminal focused: ⌘T opens a terminal tab, ⇧⌘W closes the tab, ⇧⌘T reopens one", () => {
	focusGpuSink();
	const t = press("KeyT", { metaKey: true });
	const w = press("KeyW", { metaKey: true, shiftKey: true });
	press("KeyT", { metaKey: true, shiftKey: true });
	expect(ran).toEqual(["workspace::NewTerminal", "pane::CloseActiveItem", "pane::ReopenClosedItem"]);
	expect(t.defaultPrevented && w.defaultPrevented).toBe(true);
});

test("macOS: ⌘B keeps toggling the left dock and no key closes every tab", () => {
	press("KeyB", { metaKey: true });
	expect(ran).toEqual(["workspace::ToggleLeftDock"]);
	for (const init of [
		{ metaKey: true, shiftKey: true },
		{ metaKey: true },
		{ metaKey: true, altKey: true },
	]) {
		press("KeyW", init);
	}
	expect(ran).not.toContain("pane::CloseAllItems");
});

test("macOS: Ctrl chords are the program's, never the app's, in both terminal kinds", () => {
	for (const focusIt of [focusGpuSink, focusXterm]) {
		focusIt();
		const e = press("KeyT", { ctrlKey: true });
		expect(e.defaultPrevented).toBe(false);
	}
	expect(ran).toEqual([]);
});

test("a key that is not bound runs nothing and is left alone", () => {
	focusXterm();
	const e = press("KeyA", { metaKey: true, altKey: true, shiftKey: true });
	expect(e.defaultPrevented).toBe(false);
	expect(ran).toEqual([]);
});
