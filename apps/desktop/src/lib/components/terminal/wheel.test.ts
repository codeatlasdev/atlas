// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { wheelArgs } from "./wheel.ts";

const cell = { w: 8, h: 20 };
const ev = (over = {}) => ({
	deltaY: -40,
	deltaMode: 0,
	shiftKey: false,
	altKey: false,
	ctrlKey: false,
	...over,
});
const at = { col: 4, row: 2 };

// The page sends rows of ITS cell height (it knows its CSS cell; the renderer knows its physical one) and who the
// device is. Natural scrolling: a negative deltaY is "scroll up" = positive rows.
test("pixel deltas are rows of the cell height, from a precise device", () => {
	const a = wheelArgs(ev({ deltaY: -30 }), cell, at, 24, false);
	expect(a.rows).toBeCloseTo(1.5);
	expect(a.precise).toBe(true);
	expect(wheelArgs(ev({ deltaY: 10 }), cell, at, 24, false).rows).toBeCloseTo(-0.5);
});

// A notch wheel reports lines (deltaMode 1): not precise, so the host applies Ghostty's ×3 multiplier.
test("line deltas are notches of a non-precise wheel; page deltas are a screen each", () => {
	const l = wheelArgs(ev({ deltaY: -2, deltaMode: 1 }), cell, at, 24, false);
	expect(l).toMatchObject({ rows: 2, precise: false });
	const p = wheelArgs(ev({ deltaY: 1, deltaMode: 2 }), cell, at, 24, false);
	expect(p).toMatchObject({ rows: -24, precise: true });
});

// Before the renderer has measured a cell the page falls back to 16 px per row, as it always did.
test("an unknown cell falls back to 16 px rows", () => {
	expect(wheelArgs(ev({ deltaY: -32 }), null, at, 24, false).rows).toBe(2);
});

test("modifiers, the cell under the pointer and read-only travel with the event", () => {
	const a = wheelArgs(
		ev({ shiftKey: true, altKey: true, ctrlKey: true }),
		cell,
		{ col: 9, row: 7 },
		24,
		true,
	);
	expect(a).toMatchObject({ col: 9, row: 7, shift: true, alt: true, ctrl: true, readonly: true });
});

// Contract with `term/wheel.rs::WheelArgs`: the fixture is the producer's real output and the Rust test
// `wheel_args_fixture_is_what_the_page_sends` deserializes it with the real consumer.
test("the payload matches the committed fixture the Rust side deserializes", () => {
	const fixture = JSON.parse(
		readFileSync(new URL("../../__fixtures__/term-wheel-args.json", import.meta.url), "utf8"),
	);
	const made = wheelArgs(ev({ deltaY: -30, shiftKey: true }), cell, at, 24, false);
	expect(made).toEqual(fixture);
	expect(Object.keys(made).sort()).toEqual([
		"alt",
		"col",
		"ctrl",
		"precise",
		"readonly",
		"row",
		"rows",
		"shift",
	]);
});
