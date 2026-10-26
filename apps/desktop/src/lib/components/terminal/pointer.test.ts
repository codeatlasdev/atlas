// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, beforeEach, expect, jest, mock, test } from "bun:test";

// The Pointer talks to the native terminal only through `invoke`. This fake records every call and
// answers the way the Rust commands do (`term_link_at` → {kind, target}, `term_mouse_captured`, …), so
// the tests check what the user's gestures make the terminal do.
type Call = [cmd: string, args: Record<string, unknown>];
interface LinkAnswer {
	kind: "url" | "path";
	target: string;
}
let calls: Call[] = [];
let links: Record<string, LinkAnswer> = {};
let captured = false;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		calls.push([cmd, args]);
		if (cmd === "term_stats") return { t1: { cell_w: 10, cell_h: 20 } };
		if (cmd === "term_mouse_captured") return captured;
		if (cmd === "term_link_at") return links[`${args.col},${args.row}`] ?? null;
		if (cmd === "term_click_move") return false;
		return null;
	},
}));

// Dynamic: the Tauri mock above must be installed before `pointer.ts` first imports `@tauri-apps/api` (static
// imports are hoisted above `mock.module`), as in `edgePeekZones.test.ts`.
const { Pointer } = await import("./pointer.ts");

const names = () => calls.map((c) => c[0]);
const of = (cmd: string) => calls.filter((c) => c[0] === cmd).map((c) => c[1]);
/** Let the fake `invoke`'s promises (and the Pointer's awaits on them) run. */
const settle = async () => {
	for (let i = 0; i < 12; i++) await Promise.resolve();
};
/** Advance the autoscroll interval by `ms`, letting each tick's promises run. */
const advance = async (ms: number) => {
	jest.advanceTimersByTime(ms);
	await settle();
};

// The grid starts at (100, 50) in the page; cells are 10x20; padding 10/6 → cell (0,0) is at (110, 56).
const area = {
	getBoundingClientRect: () => ({ left: 100, top: 50, right: 700, bottom: 450, width: 600, height: 400 }),
	clientHeight: 400,
	contains: () => false,
};
const px = (col: number, row: number) => ({ clientX: 110 + col * 10 + 2, clientY: 56 + row * 20 + 10 });

interface Seen {
	links: [string, boolean][];
	files: string[];
	mode: boolean[];
}
let seen: Seen;
let pointer: InstanceType<typeof Pointer>;

beforeEach(() => {
	jest.useFakeTimers();
	calls = [];
	links = {};
	captured = false;
	seen = { links: [], files: [], mode: [] };
	pointer = new Pointer({
		tab: () => "t1",
		area: () => area,
		focusInput: () => {},
		readonly: () => false,
		prefs: () => ({ click_to_move: false, copy_on_select: true, padding_x: 10, padding_y: 6 }),
		onLink: (url, external) => seen.links.push([url, external]),
		onFile: (token) => (seen.files.push(token), token.startsWith("src/")),
		isFile: (token) => token.startsWith("src/"),
		onLinkMode: (on) => seen.mode.push(on),
	});
});
afterEach(() => {
	pointer.dispose();
	jest.useRealTimers();
});

interface Mods {
	shiftKey?: boolean;
	altKey?: boolean;
	metaKey?: boolean;
	detail?: number;
	buttons?: number;
}
/** What a real MouseEvent always carries: every modifier is a boolean, never missing. */
const base = { button: 0, detail: 1, shiftKey: false, altKey: false, metaKey: false, ctrlKey: false };
const down = (col: number, row: number, o: Mods = {}) =>
	pointer.mousedown({ ...base, buttons: 1, preventDefault() {}, ...px(col, row), ...o });
const move = (col: number, row: number, o: Mods = {}) =>
	pointer.mousemove({ ...base, buttons: 0, ...px(col, row), ...o });
const up = (col: number, row: number, o: Mods = {}) =>
	pointer.mouseup({ ...base, buttons: 0, ...px(col, row), ...o });
/** A window-level event, as when the pointer is outside the slot. */
const windowMouse = (type: "mousemove" | "mouseup", x: number, y: number, buttons: number) =>
	window.dispatchEvent(new MouseEvent(type, { clientX: x, clientY: y, buttons }));

test("a plain drag selects from the press cell to the release cell and copies on select", async () => {
	await down(1, 2);
	move(5, 2, { buttons: 1 });
	await up(5, 2);
	await settle();
	expect(of("term_select_start")[0]).toMatchObject({ col: 1, row: 2, clicks: 1, block: false });
	expect(of("term_select_update").at(-1)).toMatchObject({ col: 5, row: 2 });
	expect(of("term_select_end")[0]).toMatchObject({ copy: true });
});

test("double and triple click ask for a word and a line; a fourth click is still a line", async () => {
	await down(3, 1, { detail: 2 });
	await down(3, 1, { detail: 3 });
	await down(3, 1, { detail: 4 });
	expect(of("term_select_start").map((a) => a.clicks)).toEqual([2, 3, 3]);
});

test("⌥-drag asks for a block selection", async () => {
	await down(2, 2, { altKey: true });
	expect(of("term_select_start")[0]).toMatchObject({ block: true });
});

// Incident: shift-click always started a new selection, so there was no way to extend one.
test("⇧-click extends the selection instead of starting a new one, and a drag after it keeps extending", async () => {
	await down(4, 3, { shiftKey: true });
	expect(names()).not.toContain("term_select_start");
	expect(of("term_select_extend")[0]).toMatchObject({ col: 4, row: 3 });
	move(8, 3, { buttons: 1, shiftKey: true });
	expect(of("term_select_update").at(-1)).toMatchObject({ col: 8, row: 3 });
	await up(8, 3, { shiftKey: true });
	expect(of("term_select_end")).toHaveLength(1);
});

// Ghostty: a program that asked for the mouse gets plain clicks; ⇧ gives the selection back.
test("a program that owns the mouse gets the click, and ⇧ takes it back for selection", async () => {
	captured = true;
	await down(2, 2);
	await up(2, 2);
	expect(of("term_mouse").map((a) => a.action)).toEqual(["press", "release"]);
	expect(names()).not.toContain("term_select_start");
	calls = [];
	await down(2, 2, { shiftKey: true });
	expect(names()).toContain("term_select_extend");
	expect(names()).not.toContain("term_mouse");
});

// Incident: a release that is not over the slot (a drag that left the pane) never arrived, leaving the
// selection stuck in "dragging" until the next move over the slot.
test("a release outside the slot ends the drag (window-level tracking)", async () => {
	await down(2, 2);
	windowMouse("mouseup", 900, 700, 0);
	await settle();
	expect(of("term_select_end")).toHaveLength(1);
});

test("a drag that leaves the slot keeps updating the selection", async () => {
	await down(2, 4);
	windowMouse("mousemove", 400, 300, 1);
	await settle();
	expect(of("term_select_update").at(-1)).toMatchObject({ col: 29, row: 12 });
});

test("a drag held outside the grid autoscrolls toward the pointer, even while it holds still", async () => {
	await down(2, 4);
	// Below the bottom edge (grid bottom = 450 − padding 6 = 444).
	windowMouse("mousemove", 300, 520, 1);
	await advance(130);
	const down1 = of("term_select_scroll");
	expect(down1.length).toBeGreaterThanOrEqual(3);
	expect(down1[0].lines).toBeLessThan(0);
	// Above the top edge (grid top = 50 + 6 = 56) scrolls the other way.
	windowMouse("mousemove", 300, 10, 1);
	await advance(130);
	expect(of("term_select_scroll").at(-1).lines).toBeGreaterThan(0);
	// Back inside: it stops.
	windowMouse("mousemove", 300, 200, 1);
	await advance(10);
	const n = of("term_select_scroll").length;
	await advance(200);
	expect(of("term_select_scroll")).toHaveLength(n);
});

test("releasing the button stops the autoscroll", async () => {
	await down(2, 4);
	windowMouse("mousemove", 300, 520, 1);
	await advance(70);
	windowMouse("mouseup", 300, 520, 0);
	await settle();
	const n = of("term_select_scroll").length;
	await advance(300);
	expect(of("term_select_scroll")).toHaveLength(n);
});

// ⌘-hover: URL and OSC 8 underline with the hand; a path only when it is a file; a word nothing.
test("⌘-hover over a URL underlines it and shows the hand; moving off clears both", async () => {
	links["3,1"] = { kind: "url", target: "https://a.dev" };
	move(3, 1, { metaKey: true });
	await settle();
	expect(of("term_hover").at(-1)).toMatchObject({ col: 3, row: 1, paths: false });
	expect(seen.mode.at(-1)).toBe(true);
	move(8, 8, { metaKey: true });
	await settle();
	expect(of("term_hover").at(-1)).toEqual({ tab: "t1" });
	expect(seen.mode.at(-1)).toBe(false);
});

test("⌘-hover over a path underlines it only when it names a worktree file", async () => {
	links["2,0"] = { kind: "path", target: "src/a.ts:2:5" };
	links["6,0"] = { kind: "path", target: "example.com" };
	move(2, 0, { metaKey: true });
	await settle();
	expect(of("term_hover").at(-1)).toMatchObject({ col: 2, row: 0, paths: true });
	expect(seen.mode.at(-1)).toBe(true);
	move(6, 0, { metaKey: true });
	await settle();
	expect(of("term_hover").at(-1)).toEqual({ tab: "t1" });
	expect(seen.mode.at(-1)).toBe(false);
});

// Incident: pressing ⌘ with the pointer already over a link did nothing until the mouse moved.
test("pressing ⌘ over a link underlines it without moving the mouse; releasing ⌘ clears it", async () => {
	links["3,1"] = { kind: "url", target: "https://a.dev" };
	move(3, 1);
	calls = [];
	await pointer.metaChanged(true);
	expect(of("term_hover").at(-1)).toMatchObject({ col: 3, row: 1 });
	expect(seen.mode.at(-1)).toBe(true);
	await pointer.metaChanged(false);
	expect(of("term_hover").at(-1)).toEqual({ tab: "t1" });
	expect(seen.mode.at(-1)).toBe(false);
});

// Incident: ⌘-click always asked for the system browser (`external: true`), so Settings ▸ Links ▸
// "Atlas browser" was ignored for every link in a GPU terminal.
test("⌘-click on a link opens it per the Links setting, not always in the system browser", async () => {
	links["3,1"] = { kind: "url", target: "https://a.dev" };
	await down(3, 1, { metaKey: true });
	expect(seen.links).toEqual([["https://a.dev", false]]);
	expect(names()).not.toContain("term_select_start");
});

test("⌘-click on a path opens the file; on a path that is no file it selects like a plain click", async () => {
	links["2,0"] = { kind: "path", target: "src/a.ts:2:5" };
	links["6,0"] = { kind: "path", target: "example.com" };
	await down(2, 0, { metaKey: true });
	expect(seen.files).toEqual(["src/a.ts:2:5"]);
	expect(names()).not.toContain("term_select_start");
	calls = [];
	await down(6, 0, { metaKey: true });
	expect(names()).toContain("term_select_start");
});

test("⌘-click on empty space is a plain click (selects, never opens)", async () => {
	await down(9, 9, { metaKey: true });
	expect(seen.links).toEqual([]);
	expect(names()).toContain("term_select_start");
});

test("the pointer leaving the slot clears the hover and the hand", async () => {
	links["3,1"] = { kind: "url", target: "https://a.dev" };
	move(3, 1, { metaKey: true });
	await settle();
	calls = [];
	pointer.clearHover();
	expect(of("term_hover").at(-1)).toEqual({ tab: "t1" });
	expect(seen.mode.at(-1)).toBe(false);
});

// SGR-Pixels (DECSET 1016): every report carries the pointer's device-px offset from the grid origin
// (after padding), both axes or neither; the VT ignores it unless the program turned 1016 on.
test("reports to a mouse-reporting program carry the device-pixel offset inside the grid", async () => {
	captured = true;
	const dpr = window.devicePixelRatio || 1;
	await down(2, 2);
	move(4, 3, { buttons: 1 });
	await up(4, 3);
	const reports = of("term_mouse");
	expect(reports.map((r) => r.action)).toEqual(["press", "motion", "release"]);
	expect(reports[0]).toMatchObject({
		px: Math.round((2 * 10 + 2) * dpr),
		py: Math.round((2 * 20 + 10) * dpr),
	});
	expect(reports[2]).toMatchObject({
		px: Math.round((4 * 10 + 2) * dpr),
		py: Math.round((3 * 20 + 10) * dpr),
	});
});
