// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { backing, loopMode, needsResize, pageIsFocused } from "./player.ts";

describe("loopMode: when the paint loop may run", () => {
	test("animates only when visible, focused and motion is allowed", () => {
		expect(loopMode({ visible: true, focused: true, reducedMotion: false })).toBe("animate");
	});
	test("hidden or unfocused pauses (no GPU/CPU burn behind another window)", () => {
		expect(loopMode({ visible: false, focused: true, reducedMotion: false })).toBe("pause");
		expect(loopMode({ visible: true, focused: false, reducedMotion: false })).toBe("pause");
	});
	test("reduced motion draws one still frame, but a hidden canvas still pauses", () => {
		expect(loopMode({ visible: true, focused: true, reducedMotion: true })).toBe("static");
		expect(loopMode({ visible: false, focused: true, reducedMotion: true })).toBe("pause");
	});
});

describe("pageIsFocused: an agent instance has no focus and must still animate", () => {
	// Incident (first agent-mode welcome recording): the globe stayed black because loopMode() pauses an unfocused window
	// and an agent instance is never focused by design.
	test("a normal window is as focused as the document says", () => {
		expect(pageIsFocused(true, false)).toBe(true);
		expect(pageIsFocused(false, false)).toBe(false);
	});
	test("an agent instance counts as focused, so its globe animates and records", () => {
		expect(pageIsFocused(false, true)).toBe(true);
		expect(loopMode({ visible: true, focused: pageIsFocused(false, true), reducedMotion: false })).toBe(
			"animate",
		);
	});
	test("it is still paused when hidden: only the focus requirement is waived", () => {
		expect(loopMode({ visible: false, focused: pageIsFocused(false, true), reducedMotion: false })).toBe(
			"pause",
		);
	});
});

describe("backing: the canvas buffer must always match its CSS box", () => {
	// Incident (live app, first run): the globe painted into a 640x420 corner of the window. The buffer was sized once at
	// mount, while the canvas still had its 300x150 default box, and never re-sized when layout settled.
	test("scales the CSS box by the device pixel ratio, capped at 2, to whole pixels", () => {
		expect(backing(1280, 820, 2)).toEqual({ w: 2560, h: 1640, dpr: 2 });
		expect(backing(1280, 820, 3)).toEqual({ w: 2560, h: 1640, dpr: 2 });
		expect(backing(333.4, 100.6, 1)).toEqual({ w: 333, h: 101, dpr: 1 });
	});
	test("a zero or missing box has no backing (nothing to paint yet)", () => {
		expect(backing(0, 820, 2)).toBeNull();
		expect(backing(1280, 0, 2)).toBeNull();
		expect(backing(Number.NaN, 820, 2)).toBeNull();
	});
	test("a bad pixel ratio falls back to 1", () => {
		expect(backing(100, 100, 0)?.dpr).toBe(1);
		expect(backing(100, 100, Number.NaN)?.dpr).toBe(1);
	});
	test("needsResize is true exactly when the buffer differs from what the box needs", () => {
		const b = backing(1280, 820, 2);
		expect(needsResize({ width: 2560, height: 1640 }, b)).toBe(false);
		expect(needsResize({ width: 300, height: 150 }, b)).toBe(true);
		expect(needsResize({ width: 2560, height: 1640 }, null)).toBe(false);
	});
});
