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
mock.module("@codeatlasdev/ui", () => ({ toast: { error: () => {}, info: () => {} } }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
// the theme store globs the bundled theme files (a Vite feature): this test needs none of it
mock.module("../../stores/theme.svelte.ts", () => ({ themes: { terminal: null, current: null } }));
// the colour payload normalizes CSS colours through a 2D canvas, which the test DOM lacks
HTMLCanvasElement.prototype.getContext = () => ({
	fillStyle: "#000000",
	fillRect() {},
	getImageData: () => ({ data: [0, 0, 0, 255] }),
});
const { NativeView } = await import("./nativeView.ts");
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
