// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";

// Incident (desktop flicker, xterm path, Linux and remote terminals): the terminal opened with the DOM renderer,
// swapped to WebGL a frame later, fitted only after the pty attach answered (first frames at 80x24, then a reflow),
// replayed with `reset()` + `write()` on screen (a blank frame, then a full redraw), kept one WebGL context per
// mounted space (WebKit drops the oldest ones: a terminal went black and repainted) and had no context-loss handler.
// The mocks record the ORDER of calls: the contract is the sequence, not any one call.

const log: string[] = [];
let webgl: FakeWebgl;
type PtyEvent = { payload: { data: string; replay: boolean } };
let sendEvent: (e: PtyEvent) => void;
const emit = (payload: PtyEvent["payload"]) => sendEvent({ payload });
let attachDone: () => void;
let renderCb: () => void;

interface Addon {
	name?: string;
	activate?(t: FakeTerminal): void;
}

class FakeTerminal {
	cols = 80;
	rows = 24;
	options: Record<string, unknown> = {};
	unicode = { activeVersion: "" };
	element: HTMLElement | undefined;
	loadAddon(a: Addon) {
		log.push(`load:${a.name ?? a.constructor.name}`);
		a.activate?.(this);
	}
	open(el: HTMLElement) {
		log.push("open");
		this.element = el;
	}
	onRender(cb: () => void) {
		renderCb = cb;
		return { dispose() {} };
	}
	refresh() {}
	onData() {}
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
	constructor() {
		webgl = this;
	}
	onContextLoss(cb: () => void) {
		this.ctxLoss = cb;
		return { dispose() {} };
	}
	dispose() {
		log.push("webgl:dispose");
	}
}

mock.module("@xterm/xterm", () => ({ Terminal: FakeTerminal }));
mock.module("@xterm/addon-fit", () => ({ FitAddon }));
mock.module("@xterm/addon-search", () => ({ SearchAddon: Plain }));
mock.module("@xterm/addon-web-links", () => ({ WebLinksAddon: Plain }));
mock.module("@xterm/addon-unicode11", () => ({ Unicode11Addon: Plain }));
mock.module("@xterm/addon-webgl", () => ({ WebglAddon: FakeWebgl }));
mock.module("@tauri-apps/api/event", () => ({
	listen: async (name: string, cb: typeof sendEvent) => {
		if (name.startsWith("pty://")) sendEvent = cb;
		return () => {};
	},
}));
// Module mocks are process-wide in one `bun test` run and workspace.test.ts already replaces `#lib/api.ts` and
// `#lib/host.ts`, so whichever file loads first decides what the other sees. The host mock therefore covers the pty
// calls of both files; base64 lives in the pure `#lib/b64.ts`, and this component does not import `api.ts` at all.
const hostFake = {
	ptyAttach: () =>
		new Promise<void>((r) => {
			attachDone = r;
		}),
	ptyResize: async () => {},
	ptyDetach: async () => {},
	ptyWrite: async () => {},
	ptyKill: async () => {},
	ptyList: async () => [],
	ptyCreate: async () => ({ pty: "p1" }),
};
mock.module("#lib/host.ts", () => ({ host: hostFake }));
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
await (await import("../../../test/uiKit.ts")).installUiKit();
// The theme store pulls the whole kit (colorScheme, bits-ui), which bun cannot load; no other test touches it.
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
// Same for prefs (editor/settings.test.ts mocks it too): the real store links `api.ts`, which workspace.test.ts stubs.
mock.module("#lib/stores/prefs.svelte.ts", () => ({
	prefs: {
		value: {
			terminal: {
				max_scroll_history_lines: 1000,
				font_family: "mono",
				font_size: 13,
				font_thicken: false,
				blinking: false,
				cursor_shape: "block",
				padding_x: 0,
				padding_y: 0,
			},
		},
	},
}));

// Dynamic on purpose: the module mocks above and the Svelte loader from test/setup.ts must exist before these load.
const { flushSync, mount, unmount } = await import("svelte");
const { proxy } = await import("svelte/internal/client");
const { default: Terminal } = await import("./Terminal.svelte");

const realOffset = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
beforeEach(() => {
	log.length = 0;
	// laid out: the host element has a size when the component mounts
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 400 });
	globalThis.ResizeObserver = class {
		observe() {}
		disconnect() {}
	};
});
afterEach(() => {
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
