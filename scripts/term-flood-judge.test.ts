// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { classify, sigLoop, tally } from "./term-flood-judge.ts";

const INNER = 26; // rows carrying the frame number
const sig = (frames: (number | null)[]) => {
	const ids = [
		...new Set(
			frames.filter((f) => f !== null).map((f) => String(f).padStart(6, "0")),
		),
	];
	return { ids, rows: frames.filter((f) => f !== null).length };
};
const whole = (f: number) => sig(Array.from({ length: INNER }, () => f));

test("a screen of one frame is whole", () => {
	expect(classify(whole(7), INNER, true)).toBe("whole");
});

// The repaint of frame 8 over frame 7 stopped half way: the exact picture a flicker is made of.
test("rows of two frames are torn", () => {
	expect(
		classify(sig([...Array(13).fill(8), ...Array(13).fill(7)]), INNER, true),
	).toBe("torn");
});

// Erase-then-draw: after the erase and before the draw the block is gone, or partly.
test("a missing block, or part of it, after a frame was seen is erased", () => {
	expect(classify(sig([]), INNER, true)).toBe("erased");
	expect(
		classify(sig([...Array(5).fill(8), ...Array(21).fill(null)]), INNER, true),
	).toBe("erased");
});

test("an empty screen before the first frame is startup, not tearing", () => {
	expect(classify(sig([]), INNER, false)).toBe("before-first-frame");
	expect(tally([sig([]), whole(1), sig([]), whole(2)], INNER)).toEqual({
		whole: 2,
		torn: 0,
		erased: 1,
		"before-first-frame": 1,
	});
});

// The page snippet is the part that failed silently once (an escape eaten by the template): run it against a fake
// bridge and check the signatures it records.
test("the page snippet records the frame numbers and rows of each read", async () => {
	const screens = [
		Array.from({ length: 3 }, (_, r) => `\u2502 frame 000042 row ${r}`).join(
			"\n",
		),
		"",
	];
	let i = 0;
	const window = {};
	const T = { invoke: async () => screens[Math.min(i++, screens.length - 1)] };
	const run = new Function(
		"window",
		"__T",
		`const T = __T; ${sigLoop("t", 1).replace("T = window.__TAURI_INTERNALS__", "_u = 0")}`,
	);
	window.__TAURI_INTERNALS__ = T;
	run(window, T);
	await Bun.sleep(30);
	window.__sigs.stop = true;
	expect(window.__sigs.list[0]).toEqual({ ids: ["000042"], rows: 3 });
	expect(window.__sigs.list.at(-1)).toEqual({ ids: [], rows: 0 });
});
