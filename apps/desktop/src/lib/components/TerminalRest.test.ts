// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, jest, mock, test } from "bun:test";
import { prefsMock } from "../../../test/prefsMock.ts";

// Incident (2026-11-23, Linux release build, `terminal-output`): a terminal that prints for minutes in a window nobody
// looks at (blurred past the shared clock's grace) still rendered every frame: the page's JS and the compositor paid for
// pixels no one sees. xterm keeps parsing (the buffer must stay right) but must stop rendering while the clock rests, and
// draw ONE full refresh when the page wakes. These tests drive the REAL xterm.js through the real component and the real
// clock; only the host/IPC boundary, prefs and theme are stubbed, and `IntersectionObserver` (happy-dom has no layout)
// follows the browser's rule that matters here: a target with `display: none` in its chain is not intersecting.

let channel: { cb: (b: ArrayBuffer) => void };
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("#lib/host.ts", () => ({
	host: {
		ptyResize: async () => {},
		ptyDetach: async () => {},
		ptyWrite: async () => {},
		ptyKill: async () => {},
		ptyList: async () => [],
		ptyCreate: async () => ({ pty: "p1" }),
	},
}));
mock.module("@tauri-apps/api/core", () => ({
	Channel: class {
		constructor(public cb: (b: ArrayBuffer) => void) {
			channel = this;
		}
	},
	invoke: () => Promise.withResolvers().promise,
}));
// Dynamic imports below: the module mocks above must exist before these modules load (as in Terminal.test.ts).
await (await import("../../../test/uiKit.ts")).installUiKit();
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
mock.module("#lib/stores/prefs.svelte.ts", () =>
	prefsMock({
		terminal: {
			max_scroll_history_lines: 1000,
			font_family: "monospace",
			font_size: 13,
			font_thicken: false,
			blinking: false,
			cursor_shape: "block",
			padding_x: 0,
			padding_y: 0,
		},
	}),
);
// No WebGL in happy-dom: the component's own fallback (the DOM renderer) is what runs.
mock.module("@xterm/addon-webgl", () => ({
	WebglAddon: class {
		constructor() {
			throw new Error("no webgl");
		}
	},
}));

const { flushSync, mount, unmount } = await import("svelte");
const { proxy } = await import("svelte/internal/client");
const { Terminal: Xterm } = await import("@xterm/xterm");
const { default: Terminal } = await import("./Terminal.svelte");

/** `IntersectionObserver` as a browser answers it for a target that is laid out or `display: none`; `settle()` delivers the changes. */
const observers = new Set<{ target: Element; cb: (e: unknown[]) => void; last?: boolean }>();
class FakeIntersectionObserver {
	#o: { target: Element; cb: (e: unknown[]) => void; last?: boolean } | undefined;
	constructor(private cb: (e: unknown[]) => void) {}
	observe(target: Element) {
		this.#o = { target, cb: this.cb };
		observers.add(this.#o);
	}
	disconnect() {
		if (this.#o) observers.delete(this.#o);
	}
}
const shown = (el: Element | null) => {
	for (; el instanceof HTMLElement; el = el.parentElement) if (el.style.display === "none") return false;
	return true;
};
function settle() {
	for (const o of observers) {
		const isIntersecting = shown(o.target);
		if (o.last === isIntersecting) continue;
		o.last = isIntersecting;
		o.cb([{ isIntersecting, intersectionRatio: isIntersecting ? 1 : 0 }]);
	}
}

const realIO = globalThis.IntersectionObserver;
const realRO = globalThis.ResizeObserver;
const realOffset = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
let term: InstanceType<typeof Xterm>;
const opened = Xterm.prototype.open;
beforeEach(() => {
	globalThis.IntersectionObserver = FakeIntersectionObserver;
	globalThis.ResizeObserver = class {
		observe() {}
		unobserve() {}
		disconnect() {}
	};
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 400 });
	// The component keeps its terminal private: take it as it opens.
	Xterm.prototype.open = function (el) {
		term = this;
		return opened.call(this, el);
	};
});
afterEach(() => {
	globalThis.IntersectionObserver = realIO;
	globalThis.ResizeObserver = realRO;
	if (realOffset) Object.defineProperty(HTMLElement.prototype, "offsetWidth", realOffset);
	Xterm.prototype.open = opened;
	document.body.innerHTML = "";
	observers.clear();
	window.dispatchEvent(new Event("focus"));
});

/** The real clock rests after a blur longer than its grace (60 s): fake timers for that wait only, xterm draws on real frames. */
function rest() {
	jest.useFakeTimers();
	try {
		document.hasFocus = () => false;
		window.dispatchEvent(new Event("blur"));
		jest.advanceTimersByTime(60_001);
	} finally {
		jest.useRealTimers();
	}
	settle();
}
function wake() {
	document.hasFocus = () => true;
	window.dispatchEvent(new Event("focus"));
	settle();
}
/** Two animation frames: xterm renders from `requestAnimationFrame`, so whatever it was going to draw has been drawn. */
/** Write to the terminal and wait until the parser has consumed it (xterm parses from a timer, not inside `write`). */
const print = (text: string) => {
	const { promise, resolve } = Promise.withResolvers<void>();
	term.write(text, () => resolve());
	return promise;
};
const frame = () => {
	const { promise, resolve } = Promise.withResolvers<void>();
	requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
	return promise;
};

async function mountTerminal() {
	const target = document.createElement("div");
	document.body.append(target);
	const p = proxy({ pty: "p1", focused: false, readonly: false, visible: true, onLink() {} });
	const app = flushSync(() => mount(Terminal, { target, props: p }));
	settle();
	const renders: { start: number; end: number }[] = [];
	term.onRender((r) => renders.push(r));
	await print("boot\r\n");
	await frame();
	renders.length = 0;
	return { app, renders, row0: () => term.buffer.active.getLine(0)?.translateToString(true) };
}

test("while the page is visible, output is rendered as it arrives (the rest gate changes nothing there)", async () => {
	const { app, renders } = await mountTerminal();
	await print("visible output\r\n");
	await frame();
	expect(renders.length).toBeGreaterThan(0);
	unmount(app);
});

test("a resting page keeps parsing output but renders nothing, then wakes with one full refresh", async () => {
	const { app, renders, row0 } = await mountTerminal();
	rest();
	await frame();
	renders.length = 0;
	for (let i = 0; i < 50; i++) {
		await print(`line ${i} printed while resting\r\n`);
		await frame();
	}
	expect(renders).toEqual([]);
	// parsing never stopped: the buffer is right, so the wake draws the real screen
	expect(term.buffer.active.getLine(1)?.translateToString(true)).toBe("line 0 printed while resting");
	expect(term.buffer.active.length).toBeGreaterThan(50);
	wake();
	await frame();
	expect(renders).toEqual([{ start: 0, end: term.rows - 1 }]);
	// and it keeps rendering afterwards
	renders.length = 0;
	await print("after wake\r\n");
	await frame();
	expect(renders.length).toBeGreaterThan(0);
	void row0;
	unmount(app);
});

test("a wake with nothing printed meanwhile still leaves the terminal rendering live", async () => {
	const { app, renders } = await mountTerminal();
	rest();
	wake();
	await frame();
	renders.length = 0;
	await print("x\r\n");
	await frame();
	expect(renders.length).toBeGreaterThan(0);
	unmount(app);
});

test("a terminal xterm itself paused as off-screen stays paused when the page wakes (a hidden pane must not render)", async () => {
	const { app, renders } = await mountTerminal();
	const screen = term.element.querySelector(".xterm-screen");
	screen.style.display = "none"; // the pane is not on screen: xterm's own observer pauses it
	settle();
	rest();
	wake();
	await frame();
	renders.length = 0;
	await print("unseen\r\n");
	await frame();
	expect(renders).toEqual([]);
	screen.style.display = "";
	settle();
	await frame();
	expect(renders.length).toBeGreaterThan(0); // xterm's own un-pause redraws it
	unmount(app);
});

test("the gate lets go of the clock when the terminal is closed", async () => {
	const { app } = await mountTerminal();
	unmount(app);
	// A closed terminal must not be redrawn (or throw) by a later wake.
	expect(() => (rest(), wake())).not.toThrow();
});
