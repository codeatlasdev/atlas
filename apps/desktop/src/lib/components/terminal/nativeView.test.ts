// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { beforeEach, expect, mock, test } from "bun:test";

// Incident (2026-10-26, GIF of the analisa-ai space showing the `ac` project's terminal): a space switch unmounted a layer
// while its terminal's `term_open` was in flight. `term_close` ran outside the open/bounds chain, found no view and
// returned; the open then created a native view nobody owned, which stayed on screen over every other space.
const calls: string[] = [];
let releaseOpen: () => void = () => {};
const call = async (cmd: string) => {
	calls.push(cmd);
	if (cmd === "term_open") await new Promise<void>((r) => (releaseOpen = r));
	return null;
};
const toasts: string[] = [];
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
mock.module("@codeatlasdev/ui", () => ({
	toast: { error: (t: string) => toasts.push(`error: ${t}`), info: (t: string) => toasts.push(`info: ${t}`) },
}));
mock.module("@tauri-apps/api/event", () => ({
	listen: async (name: string, h: (e: { payload: unknown }) => void) => {
		handlers[name] = h;
		return () => {};
	},
}));
// the theme store globs the bundled theme files (a Vite feature): this test needs none of it
mock.module("../../stores/theme.svelte.ts", () => ({ themes: { terminal: null, current: null } }));
// the colour payload normalizes CSS colours through a 2D canvas, which the test DOM lacks
HTMLCanvasElement.prototype.getContext = () => ({
	fillStyle: "#000000",
	fillRect() {},
	getImageData: () => ({ data: [0, 0, 0, 255] }),
});
const { NativeView, listenFontFallback } = await import("./nativeView.ts");
const { gpu } = await import("../../stores/gpu.svelte.ts");
const { prefs } = await import("../../stores/prefs.svelte.ts");
const { DEFAULT_PREFS } = await import("../../prefs.ts");

function slot() {
	const el = document.createElement("div");
	el.getBoundingClientRect = () => new DOMRect(0, 0, 400, 300);
	document.body.append(el);
	return el;
}

beforeEach(() => {
	HTMLCanvasElement.prototype.getContext = () => ({
		fillStyle: "#000000",
		fillRect() {},
		clearRect() {},
		getImageData: () => ({ data: [0, 0, 0, 255] }),
	});
	document.body.innerHTML = "";
	// another file leaves the boot marker on, whose `boot_mark` goes through a mocked invoke
	delete window.__ATLAS_BOOT__;
	// another test file may leave the shared store without terminal prefs
	prefs.value = structuredClone(DEFAULT_PREFS);
});

test("dispose while term_open is in flight closes the view after the open, never before", async () => {
	calls.length = 0;
	const area = slot();
	const v = new NativeView(
		() => ({ tab: "t1", pty: "p1" }),
		() => area,
		() => true,
		call,
		() => ({}),
	);
	void v.sync();
	for (let i = 0; i < 50 && !calls.length; i++) await new Promise((r) => setTimeout(r, 1));
	expect(calls).toEqual(["term_open"]);
	const closed = v.dispose();
	releaseOpen();
	await closed;
	expect(calls.at(-1)).toBe("term_close");
	expect(calls.indexOf("term_close")).toBeGreaterThan(calls.indexOf("term_open"));
});

test("after dispose no sync opens or shows the view again", async () => {
	calls.length = 0;
	const area = slot();
	const v = new NativeView(
		() => ({ tab: "t2", pty: "p2" }),
		() => area,
		() => true,
		call,
		() => ({}),
	);
	await v.dispose();
	await v.sync();
	expect(calls.filter((c) => c === "term_open" || c === "term_bounds")).toEqual([]);
});

// Phase 3 (Linux) "never a blank terminal": a `term_open` that fails (here the message Rust's Vulkan surface creation gives
// on a compositor with no dmabuf, as the pixman sway showed) must leave every terminal on xterm.js with one toast saying
// why, not an empty slot. The same for the render thread giving up on its surface (`term://unusable`).
async function openThatFails(tab: string, error: string) {
	const area = slot();
	const failing = async (cmd: string) => {
		if (cmd === "term_open") throw error;
		return null;
	};
	const v = new NativeView(
		() => ({ tab, pty: `p-${tab}` }),
		() => area,
		() => true,
		failing,
		() => ({}),
		() => {},
		false, // Linux: the suite runs as macOS (test/setup.ts)
	);
	await v.sync();
	for (let i = 0; i < 200 && !gpu.failed; i++) await Promise.resolve();
	return v;
}

test("a failed term_open sends every terminal to xterm.js with one toast that says why", async () => {
	gpu.failed = false;
	toasts.length = 0;
	await openThatFails("f1", "Vulkan surface: VK_ERROR_INITIALIZATION_FAILED");
	expect(gpu.failed).toBe(true);
	expect(toasts).toHaveLength(1);
	expect(toasts[0]).toStartWith("info: ");
	expect(toasts[0]).toContain("VK_ERROR_INITIALIZATION_FAILED");
	// a second terminal failing the same way adds no second toast
	await openThatFails("f2", "Vulkan surface: VK_ERROR_INITIALIZATION_FAILED");
	expect(toasts).toHaveLength(1);
	gpu.failed = false;
});

test("a render thread that gives up on its surface (term://unusable) falls back the same way", async () => {
	gpu.failed = false;
	toasts.length = 0;
	listenFontFallback();
	for (let i = 0; i < 200 && !handlers["term://unusable"]; i++) await Promise.resolve();
	handlers["term://unusable"]({ payload: { tab: "t9", error: "the surface never came back" } });
	expect(gpu.failed).toBe(true);
	expect(toasts).toHaveLength(1);
	expect(toasts[0]).toContain("the surface never came back");
	gpu.failed = false;
});
