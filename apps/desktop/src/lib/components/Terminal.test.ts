// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { prefsMock } from "../../../test/prefsMock.ts";

// Incident (desktop flicker, xterm path, Linux and remote terminals): the terminal opened with the DOM renderer,
// swapped to WebGL a frame later, fitted only after the pty attach answered (first frames at 80x24, then a reflow),
// replayed with `reset()` + `write()` on screen (a blank frame, then a full redraw), kept one WebGL context per
// mounted space (WebKit drops the oldest ones: a terminal went black and repainted) and had no context-loss handler.
// The mocks record the ORDER of calls: the contract is the sequence, not any one call.

const log: string[] = [];
let webgl: FakeWebgl;
/** Every WebGL context the fake renderer created: a context is freed only when it is lost (`WEBGL_lose_context`) or garbage collected. */
const contexts: { lost: boolean }[] = [];
/** Every Tauri command the page invoked, in order. */
const invoked: { cmd: string; args: unknown }[] = [];
/** The terminal's channel, as the Rust side would deliver a frame: kind byte (1 = history, 0 = output), then bytes. */
let channel: { cb: (b: ArrayBuffer) => void };
const emit = (p: { data: string; replay: boolean }) => {
	const bytes = new TextEncoder().encode(atob(p.data));
	const frame = new Uint8Array(bytes.length + 1);
	frame[0] = p.replay ? 1 : 0;
	frame.set(bytes, 1);
	channel.cb(frame.buffer);
};
let attachDone: () => void;
let renderCb: () => void;
/** The families the webview can draw (what `document.fonts.check` answers) and the user's pref list. */
let installed: string[] = [];
let userFamily = "Fira Code, mono";
let fontsReady: () => void;

interface Addon {
	name?: string;
	activate?(t: FakeTerminal): void;
	/** xterm tells an addon when the terminal got its element (the WebGL renderer adds its canvas then). */
	opened?(el: HTMLElement): void;
	dispose?(): void;
}

class FakeTerminal {
	cols = 80;
	rows = 24;
	options: Record<string, unknown>;
	constructor(opts: Record<string, unknown> = {}) {
		// xterm re-measures the cell whenever `fontFamily` is set: the log records each set, in order.
		this.options = new Proxy(opts, {
			set(o, k, v) {
				// xterm ignores a set that does not change the value: only a real change re-measures the cell.
				if (k === "fontFamily" && o[k] !== v) log.push(`font:${v}`);
				o[k] = v;
				return true;
			},
		});
		if (opts.fontFamily) log.push(`font:${opts.fontFamily}`);
	}
	unicode = { activeVersion: "" };
	element: HTMLElement | undefined;
	addons: Addon[] = [];
	loadAddon(a: Addon) {
		log.push(`load:${a.name ?? a.constructor.name}`);
		this.addons.push(a);
		a.activate?.(this);
		// An addon loaded into an open terminal builds its renderer at once (xterm defers it only until `open`).
		if (this.element) a.opened?.(this.element);
	}
	open(el: HTMLElement) {
		log.push("open");
		this.element = el;
		for (const a of this.addons) a.opened?.(el);
	}
	onRender(cb: () => void) {
		renderCb = cb;
		return { dispose() {} };
	}
	refresh() {}
	onData() {}
	onSelectionChange() {
		return { dispose() {} };
	}
	hasSelection() {
		return false;
	}
	getSelection() {
		return "";
	}
	attachCustomKeyEventHandler() {}
	focus() {}
	reset() {
		log.push("reset");
	}
	write(_b: unknown, cb?: () => void) {
		log.push("write");
		cb?.();
	}
	dispose() {
		log.push("dispose");
		// xterm disposes the addons it loaded with the terminal.
		for (const a of this.addons) a.dispose?.();
	}
}
class FitAddon {
	name = "fit";
	fit() {
		log.push("fit");
	}
}
class Plain {
	activate() {}
}
class FakeWebgl {
	name = "webgl";
	ctxLoss: (() => void) | undefined;
	canvas: HTMLCanvasElement | undefined;
	disposed = false;
	constructor() {
		webgl = this;
	}
	/** As xterm's WebglRenderer: a canvas in the terminal's screen whose `webgl2` context lives as long as nobody loses it. */
	opened(el: HTMLElement) {
		const state = { lost: false };
		contexts.push(state);
		const gl = {
			isContextLost: () => state.lost,
			getExtension: (n: string) => (n === "WEBGL_lose_context" ? { loseContext: () => void (state.lost = true) } : null),
		};
		const canvas = document.createElement("canvas");
		canvas.getContext = ((t: string) => (t === "webgl2" ? gl : null)) as typeof canvas.getContext;
		el.append(canvas);
		this.canvas = canvas;
	}
	onContextLoss(cb: () => void) {
		this.ctxLoss = cb;
		return { dispose() {} };
	}
	dispose() {
		if (this.disposed) return;
		this.disposed = true;
		log.push("webgl:dispose");
		// xterm removes the canvas from the DOM and nothing more: the context stays alive until a GC finds it.
		this.canvas?.remove();
	}
}

mock.module("@xterm/xterm", () => ({ Terminal: FakeTerminal }));
mock.module("@xterm/addon-fit", () => ({ FitAddon }));
mock.module("@xterm/addon-search", () => ({ SearchAddon: Plain }));
mock.module("@xterm/addon-web-links", () => ({ WebLinksAddon: Plain }));
mock.module("@xterm/addon-unicode11", () => ({ Unicode11Addon: Plain }));
mock.module("@xterm/addon-webgl", () => ({ WebglAddon: FakeWebgl }));
/** Every event name the component (and the shared host dispatcher) asked Tauri to listen to, in order. */
const listened: string[] = [];
mock.module("@tauri-apps/api/event", () => ({
	listen: async (name: string) => {
		listened.push(name);
		return () => {};
	},
}));
// Module mocks are process-wide in one `bun test` run and workspace.test.ts already replaces `#lib/api.ts` and
// `#lib/host.ts`, so whichever file loads first decides what the other sees. The host mock therefore covers the pty
// calls of both files; base64 lives in the pure `#lib/b64.ts`, and this component does not import `api.ts` at all.
const hostFake = {
	ptyResize: async () => {},
	ptyDetach: async () => {},
	ptyWrite: async () => {},
	ptyKill: async () => {},
	ptyList: async () => [],
	ptyCreate: async () => ({ pty: "p1" }),
};
mock.module("#lib/host.ts", () => ({ host: hostFake }));
mock.module("@tauri-apps/api/core", () => ({
	Channel: class {
		constructor(public cb: (b: ArrayBuffer) => void) {
			channel = this;
		}
	},
	// `pty_stream` answers with the stream id when the test says the attach is done.
	invoke: (cmd: string, args?: unknown) => {
		invoked.push({ cmd, args });
		return cmd === "pty_stream"
			? new Promise<number>((r) => {
					attachDone = () => r(1);
				})
			: Promise.resolve(null);
	},
}));
await (await import("../../../test/uiKit.ts")).installUiKit();
// The theme store pulls the whole kit (colorScheme, bits-ui), which bun cannot load; no other test touches it.
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
// Same for prefs (editor/settings.test.ts mocks it too): the real store links `api.ts`, which workspace.test.ts stubs.
mock.module("#lib/stores/prefs.svelte.ts", () =>
	prefsMock({
		terminal: {
			max_scroll_history_lines: 1000,
			get font_family() {
				return userFamily;
			},
			font_size: 13,
			font_thicken: false,
			blinking: false,
			cursor_shape: "block",
			padding_x: 0,
			padding_y: 0,
		},
	}),
);

// A fixed-pitch face for every installed family; the rest cannot be drawn by the webview.
mock.module("./terminal/xtermFont.ts", async () => {
	const real = await import("./terminal/xtermFont.ts?real");
	return {
		...real,
		// WKWebView: an undrawable family measures as the serif fallback (narrow `i`, wide `W`), and
		// `document.fonts.check` says true for every name, so the env has no `check`.
		browserFontEnv: () => ({
			measure: (f: string, ch: string) => (installed.includes(f) ? 7 : ch === "i" ? 4.45 : 15.1),
		}),
	};
});
Object.defineProperty(document, "fonts", {
	configurable: true,
	value: {
		ready: new Promise<void>((r) => (fontsReady = r)),
		addEventListener() {},
		removeEventListener() {},
	},
});

// Dynamic on purpose: the module mocks above and the Svelte loader from test/setup.ts must exist before these load.
const { flushSync, mount, unmount } = await import("svelte");
const { proxy } = await import("svelte/internal/client");
const { default: Terminal } = await import("./Terminal.svelte");

const realOffset = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
const realRO = globalThis.ResizeObserver;
beforeEach(() => {
	log.length = 0;
	contexts.length = 0;
	invoked.length = 0;
	// laid out: the host element has a size when the component mounts
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 400 });
	globalThis.ResizeObserver = class {
		observe() {}
		unobserve() {}
		disconnect() {}
	};
});
afterEach(() => {
	// The stub must not outlive this file: later suites (VirtualList) need the full observer API.
	globalThis.ResizeObserver = realRO;
	if (realOffset) Object.defineProperty(HTMLElement.prototype, "offsetWidth", realOffset);
	document.body.innerHTML = "";
});

function setup(props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const p = proxy({ pty: "p1", focused: false, readonly: false, visible: true, onLink() {}, ...props });
	const app = flushSync(() => mount(Terminal, { target, props: p }));
	return { p, target, app, host: () => target.querySelector(".xterm-host") as HTMLElement };
}

// Incident (2026-10-26, Linux: one terminal tab lags, two freeze): each terminal registered its own listeners for
// `host://event` and `host://reconnected` (two IPC round trips and a JS callback per hostd event, per terminal).
test("any number of terminals add no listener of their own for host events: only their pty stream", async () => {
	const a = setup();
	await Promise.resolve();
	const first = listened.filter((n) => n.startsWith("host://")).length;
	const b = setup({ pty: "p2" });
	const c = setup({ pty: "p3" });
	await Promise.resolve();
	await Promise.resolve();
	expect(listened.filter((n) => n.startsWith("host://")).length).toBe(first);
	expect(first).toBeLessThanOrEqual(2);
	unmount(a.app);
	unmount(b.app);
	unmount(c.app);
});

test("WebGL is loaded before open, so the first paint is already WebGL (no DOM-renderer to WebGL swap)", () => {
	const { app } = setup();
	expect(log.indexOf("load:webgl")).toBeGreaterThanOrEqual(0);
	expect(log.indexOf("load:webgl")).toBeLessThan(log.indexOf("open"));
	unmount(app);
});

test("the grid is fitted right after open, before the pty attach answers and before any output is written", () => {
	const { app } = setup();
	// attach is still pending here: the fit must not wait for it
	expect(log.indexOf("fit")).toBeGreaterThan(log.indexOf("open"));
	emit({ data: btoa("hello"), replay: true });
	expect(log.indexOf("fit")).toBeLessThan(log.indexOf("write"));
	unmount(app);
});

test("the host stays hidden until the first render, then is revealed", () => {
	const { app, host } = setup();
	expect(host().style.visibility).toBe("hidden");
	renderCb();
	flushSync();
	expect(host().style.visibility).toBe("");
	unmount(app);
});

test("a replay is written into a fresh buffer and the host is revealed only once it is drawn", () => {
	const { app, host } = setup();
	renderCb(); // first render of the empty grid
	flushSync();
	expect(host().style.visibility).toBe("");
	emit({ data: btoa("x"), replay: true });
	flushSync();
	// reset() then write(): while the replay is not drawn the host stays hidden, so the blank frame is never on screen
	expect(log.indexOf("reset")).toBeLessThan(log.indexOf("write"));
	expect(host().style.visibility).toBe("hidden");
	renderCb(); // the replayed screen is drawn
	flushSync();
	expect(host().style.visibility).toBe("");
	unmount(app);
});

test("a lost WebGL context falls back to the DOM renderer instead of leaving a black terminal", () => {
	const { app } = setup();
	webgl.ctxLoss!();
	expect(log).toContain("webgl:dispose");
	unmount(app);
});

test("WebGL is released while the terminal is not visible (parked neighbour space) and rebuilt when it is again", () => {
	const { app, p } = setup();
	log.length = 0;
	p.visible = false;
	flushSync();
	expect(log).toContain("webgl:dispose");
	log.length = 0;
	p.visible = true;
	flushSync();
	expect(log).toContain("load:webgl");
	unmount(app);
});

// Incident (study cause 6, docs/perf/linux-webkit-study.md): the web process went 205 -> 1,134 MB after a terminal flood and
// stayed above 725 MB with the tab closed. xterm removes the WebGL canvas from the page on dispose, but never loses its
// context: the context (textures, the glyph atlas, GPU buffers) lives until a garbage collection finds it, and WebKit
// caps live contexts (16), so closed and parked terminals pile up until it drops a live one.
test("closing a terminal loses its WebGL context: the canvas leaving the page is not enough", () => {
	const { app } = setup();
	expect(contexts).toHaveLength(1);
	expect(contexts[0].lost).toBe(false);
	unmount(app);
	expect(contexts.filter((c) => !c.lost)).toEqual([]);
	expect(document.querySelectorAll("canvas")).toHaveLength(0);
});

test("a terminal parked behind another space gives its WebGL context back, and the next one it builds is live", () => {
	const { app, p } = setup();
	p.visible = false;
	flushSync();
	expect(contexts.map((c) => c.lost)).toEqual([true]);
	p.visible = true;
	flushSync();
	expect(contexts.map((c) => c.lost)).toEqual([true, false]);
	unmount(app);
	expect(contexts.filter((c) => !c.lost)).toEqual([]);
});

test("closing a terminal ends its stream on the Rust side, also when it closes before the attach answered", async () => {
	const answered = setup();
	attachDone();
	await Promise.resolve();
	await Promise.resolve();
	unmount(answered.app);
	expect(invoked.filter((i) => i.cmd === "pty_stream_stop")).toEqual([{ cmd: "pty_stream_stop", args: { id: 1 } }]);

	invoked.length = 0;
	const early = setup({ pty: "p2" });
	unmount(early.app);
	attachDone(); // the attach answers after the tab is gone
	await Promise.resolve();
	await Promise.resolve();
	expect(invoked.filter((i) => i.cmd === "pty_stream_stop")).toEqual([{ cmd: "pty_stream_stop", args: { id: 1 } }]);
});

// Incident (#68, xterm path): a user-installed font WKWebView could not draw fell back to a serif while the cell was
// measured elsewhere: the terminal showed `W e l c o m e`. The list given to xterm must only hold drawable families
// and end with the generic monospace, and must be redone once the webfonts are ready (the cell is re-measured).
test("a family the webview cannot draw never reaches xterm: its list drops it and ends with monospace", () => {
	installed = ["mono"];
	userFamily = "Fira Code, mono";
	const { app } = setup();
	expect(log.filter((l) => l.startsWith("font:"))).toEqual(["font:mono, monospace"]);
	unmount(app);
});

test("with no usable family xterm gets plain monospace, not the user's unloadable list", () => {
	installed = [];
	userFamily = "Fira Code";
	const { app } = setup();
	expect(log.filter((l) => l.startsWith("font:"))).toEqual(["font:monospace"]);
	unmount(app);
});

test("when the webfonts finish loading the family list is resolved again so xterm re-measures the cell", async () => {
	installed = [];
	userFamily = "Fira Code, mono";
	const { app } = setup();
	expect(log.filter((l) => l.startsWith("font:")).at(-1)).toBe("font:monospace");
	installed = ["Fira Code", "mono"];
	fontsReady();
	await Promise.resolve();
	await Promise.resolve();
	expect(log.filter((l) => l.startsWith("font:")).at(-1)).toBe("font:Fira Code, mono, monospace");
	unmount(app);
});
