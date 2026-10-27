// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
	CONFIRM_MS,
	COPIED_MS,
	GIF_CANCELLED,
	duration,
	humanBytes,
	initialState,
	kindIcon,
	layout,
	reduce,
	textTitle,
	title,
} from "./quick.ts";

const entry = (over = {}) => ({
	path: "/tmp/a.png",
	kind: "screenshot",
	at: 1,
	width: 1440,
	height: 900,
	bytes: 1000,
	secs: null,
	...over,
});
/** A capture card the way Rust serializes it (`CaptureCard`). */
const cap = (path = "/tmp/a.png", over = {}, entryOver = {}) => ({
	kind: "capture",
	id: path,
	entry: entry({ path, ...entryOver }),
	thumb: "data:image/png;base64,xx",
	dismissSecs: 6,
	copied: false,
	...over,
});
/** A text card the way Rust serializes it (`TextCard`). */
const txt = (id = "text:1", over = {}) => ({
	kind: "text",
	id,
	preview: "hello\nworld",
	lines: 2,
	chars: 11,
	empty: false,
	dismissSecs: 6,
	...over,
});
const stackOf = (cards, position = "bottomRight") => ({ type: "stack", stack: { position, cards } });

const run = (s, ...evs) => {
	const effects = [];
	for (const ev of evs) {
		const r = reduce(s, ev);
		s = r.state;
		effects.push(...r.effects);
	}
	return { state: s, effects };
};
const card = (s, id) => s.cards.find((c) => c.id === id);
/** A stack with one capture. */
const one = (over = {}, entryOver = {}) =>
	run(initialState(), stackOf([cap("/tmp/a.png", over, entryOver)])).state;
const tick = (dtMs) => ({ type: "tick", dtMs });
const A = "/tmp/a.png";
const B = "/tmp/b.png";

describe("the stack from Rust", () => {
	test("an empty stack has no cards; a list creates one fresh card per entry, oldest first", () => {
		expect(initialState().cards).toEqual([]);
		const { state } = run(initialState(), stackOf([cap(A), cap(B), txt("text:1")]));
		expect(state.cards.map((c) => c.id)).toEqual([A, B, "text:1"]);
		expect(state.cards.map((c) => c.kind)).toEqual(["capture", "capture", "text"]);
		for (const c of state.cards) {
			expect(c.remainingMs).toBe(6000);
			expect(c.paused).toBe(false);
			expect(c.error).toBe(null);
		}
	});

	test("Rust's `copied` (copy-on-capture) starts the copied feedback of that card only", () => {
		const { state } = run(initialState(), stackOf([cap(A, { copied: true }), cap(B)]));
		expect(card(state, A).copied).toBe(true);
		expect(card(state, A).copiedMs).toBe(COPIED_MS);
		expect(card(state, B).copied).toBe(false);
	});

	test("a new capture arriving does NOT reset the timers or state of the cards already shown", () => {
		let r = run(
			one(),
			tick(4000),
			{ type: "action", id: A, name: "discard" },
			{ type: "hover", id: A, on: true },
		);
		expect(card(r.state, A).remainingMs).toBe(2000);
		r = run(r.state, stackOf([cap(A), cap(B)]));
		const a = card(r.state, A);
		expect(a.remainingMs).toBe(2000);
		expect(a.confirmDiscard).toBe(true);
		expect(a.paused).toBe(true);
		expect(card(r.state, B).remainingMs).toBe(6000);
	});

	test("a card Rust no longer lists is gone, and its local state with it", () => {
		const both = run(initialState(), stackOf([cap(A), cap(B)])).state;
		const r = run(both, stackOf([cap(B)]));
		expect(r.state.cards.map((c) => c.id)).toEqual([B]);
		const empty = run(r.state, stackOf([]));
		expect(empty.state.cards).toEqual([]);
	});

	test("the oldest card evicted by Rust disappears while the newer ones keep counting down", () => {
		let { state } = run(initialState(), stackOf([cap("/1"), cap("/2")]), tick(1000));
		state = run(state, stackOf([cap("/2"), cap("/3")])).state;
		expect(state.cards.map((c) => c.id)).toEqual(["/2", "/3"]);
		expect(card(state, "/2").remainingMs).toBe(5000);
		expect(card(state, "/3").remainingMs).toBe(6000);
	});

	test("the same file captured again (a new capture, new `at`) starts over; the pointer state carries over", () => {
		let { state } = run(one(), tick(4000), { type: "hover", id: A, on: true });
		state = run(state, stackOf([cap(A, {}, { at: 2 })])).state;
		const a = card(state, A);
		expect(a.remainingMs).toBe(6000);
		expect(a.paused).toBe(true);
		expect(a.confirmDiscard).toBe(false);
		expect(a.busy).toBe(false);
	});

	test("the corner follows Rust's setting", () => {
		expect(run(initialState(), stackOf([cap(A)], "topLeft")).state.position).toBe("topLeft");
	});
});

describe("layout: the newest card is nearest the corner", () => {
	const s = (position) => run(initialState(), stackOf([cap("/1"), cap("/2"), cap("/3")], position)).state;
	const ids = (position) => layout(s(position)).map((c) => c.id);

	test("bottom corners draw the oldest first, so the newest is lowest", () => {
		expect(ids("bottomRight")).toEqual(["/1", "/2", "/3"]);
		expect(ids("bottomLeft")).toEqual(["/1", "/2", "/3"]);
	});

	test("top corners draw the newest first, so the newest is highest", () => {
		expect(ids("topRight")).toEqual(["/3", "/2", "/1"]);
		expect(ids("topLeft")).toEqual(["/3", "/2", "/1"]);
	});

	test("layout never reorders the state (Rust's order stays the source of truth)", () => {
		const t = s("topLeft");
		layout(t);
		expect(t.cards.map((c) => c.id)).toEqual(["/1", "/2", "/3"]);
	});
});

describe("countdown is per card", () => {
	test("tick counts down and reaching 0 emits close exactly once, for that card", () => {
		const a = run(one(), tick(5999));
		expect(card(a.state, A).remainingMs).toBe(1);
		expect(a.effects).toEqual([]);
		const b = run(a.state, tick(10), tick(1000));
		expect(card(b.state, A).remainingMs).toBe(0);
		expect(b.effects).toEqual([{ type: "close", id: A, card: "capture" }]);
	});

	test("two cards with different remaining times close independently, each once", () => {
		let r = run(initialState(), stackOf([cap(A)]), tick(4000), stackOf([cap(A), cap(B)]));
		expect([card(r.state, A).remainingMs, card(r.state, B).remainingMs]).toEqual([2000, 6000]);
		r = run(r.state, tick(2000));
		expect(r.effects).toEqual([{ type: "close", id: A, card: "capture" }]);
		expect(card(r.state, B).remainingMs).toBe(4000);
		// A is still listed (Rust has not answered yet): no second close
		r = run(r.state, tick(1000));
		expect(r.effects).toEqual([]);
	});

	test("hovering one card pauses ONLY that card; leaving resumes with the REMAINING time", () => {
		let r = run(initialState(), stackOf([cap(A), cap(B)]), tick(2500));
		r = run(r.state, { type: "hover", id: A, on: true });
		expect(r.effects).toEqual([{ type: "hover", hovering: true }]);
		r = run(r.state, tick(3000));
		expect(card(r.state, A).remainingMs).toBe(3500);
		expect(card(r.state, B).remainingMs).toBe(500);
		expect(r.effects).toEqual([{ type: "close", id: B, card: "capture" }].slice(0, 0));
		r = run(r.state, tick(500));
		expect(r.effects).toEqual([{ type: "close", id: B, card: "capture" }]);
		expect(card(r.state, A).remainingMs).toBe(3500);
		r = run(r.state, { type: "hover", id: A, on: false });
		expect(r.effects).toEqual([{ type: "hover", hovering: false }]);
		r = run(r.state, tick(3500));
		expect(r.effects).toEqual([{ type: "close", id: A, card: "capture" }]);
	});

	test("no countdown while that card's action is busy; it resumes after actionOk", () => {
		let r = run(one(), tick(1000), { type: "action", id: A, name: "save" });
		expect(card(r.state, A).busy).toBe(true);
		r = run(r.state, tick(10_000));
		expect(card(r.state, A).remainingMs).toBe(5000);
		expect(r.effects.some((e) => e.type === "close")).toBe(false);
		r = run(r.state, { type: "actionOk", id: A }, tick(1000));
		expect(card(r.state, A).busy).toBe(false);
		expect(card(r.state, A).remainingMs).toBe(4000);
	});

	test("one card's busy action does not stop another card's countdown", () => {
		let r = run(
			initialState(),
			stackOf([cap(A), cap(B)]),
			{ type: "action", id: A, name: "save" },
			tick(2000),
		);
		expect(card(r.state, A).remainingMs).toBe(6000);
		expect(card(r.state, B).remainingMs).toBe(4000);
	});

	test("no countdown while the GIF sheet is open; closing it resumes", () => {
		const rec = one({}, { kind: "recording", secs: 12 });
		let r = run(rec, tick(1000), { type: "openGifSheet", id: A });
		expect(card(r.state, A).sheet).toBe("gif");
		r = run(r.state, tick(99_000));
		expect(card(r.state, A).remainingMs).toBe(5000);
		expect(r.effects).toEqual([]);
		r = run(r.state, { type: "closeSheet", id: A }, tick(1000));
		expect(card(r.state, A).sheet).toBe(null);
		expect(card(r.state, A).remainingMs).toBe(4000);
	});

	test("dismissSecs 0 never auto-dismisses", () => {
		const r = run(one({ dismissSecs: 0 }), tick(10 * 3600 * 1000));
		expect(r.effects).toEqual([]);
		expect(r.state.cards).toHaveLength(1);
	});

	test("a negative dt never adds time back", () => {
		const r = run(one(), tick(1000), tick(-5000));
		expect(card(r.state, A).remainingMs).toBe(5000);
	});

	test("tick on an empty stack is a no-op", () => {
		const r = reduce(initialState(), tick(1000));
		expect(r.state).toEqual(initialState());
		expect(r.effects).toEqual([]);
	});

	test("events for a card that is gone are ignored", () => {
		for (const ev of [
			{ type: "hover", id: "/gone", on: true },
			{ type: "action", id: "/gone", name: "copy" },
			{ type: "actionOk", id: "/gone" },
			{ type: "actionErr", id: "/gone", message: "x" },
		]) {
			const r = run(one(), ev);
			expect(r.effects).toEqual([]);
			expect(r.state.cards).toHaveLength(1);
		}
	});
});

describe("copy feedback", () => {
	test("copy asks Rust, then actionOk flips copied for 1.5 s WITHOUT restarting the dismiss timer", () => {
		let r = run(one(), tick(2000), { type: "action", id: A, name: "copy" });
		expect(r.effects).toEqual([{ type: "invoke", action: "copy", id: A, path: A }]);
		r = run(r.state, { type: "actionOk", id: A });
		expect(card(r.state, A).copied).toBe(true);
		expect(card(r.state, A).copiedMs).toBe(COPIED_MS);
		expect(card(r.state, A).remainingMs).toBe(4000);
		r = run(r.state, tick(1000));
		expect(card(r.state, A).copiedMs).toBe(500);
		expect(card(r.state, A).remainingMs).toBe(3000);
		r = run(r.state, tick(500));
		expect(card(r.state, A).copied).toBe(false);
		expect(card(r.state, A).remainingMs).toBe(2500);
	});

	test("the copied badge still expires while the pointer is over the card (dismiss timer stays paused)", () => {
		let r = run(
			one(),
			{ type: "hover", id: A, on: true },
			{ type: "action", id: A, name: "copy" },
			{
				type: "actionOk",
				id: A,
			},
		);
		r = run(r.state, tick(2000));
		expect(card(r.state, A).copied).toBe(false);
		expect(card(r.state, A).remainingMs).toBe(6000);
	});
});

describe("discard needs a second click, per card", () => {
	test("first click only arms; nothing is deleted", () => {
		const r = run(one(), { type: "action", id: A, name: "discard" });
		expect(card(r.state, A).confirmDiscard).toBe(true);
		expect(card(r.state, A).confirmMs).toBe(CONFIRM_MS);
		expect(r.effects).toEqual([]);
		expect(card(r.state, A).busy).toBe(false);
	});

	test("arming one card does not arm another", () => {
		const r = run(initialState(), stackOf([cap(A), cap(B)]), { type: "action", id: A, name: "discard" });
		expect(card(r.state, A).confirmDiscard).toBe(true);
		expect(card(r.state, B).confirmDiscard).toBe(false);
		// the second click on B only arms B: A's arming does not count for it
		const b = run(r.state, { type: "action", id: B, name: "discard" });
		expect(b.effects).toEqual([]);
		expect(card(b.state, B).confirmDiscard).toBe(true);
	});

	test("second click within 3 s deletes that card's file (busy, effect invoke discard)", () => {
		let r = run(one(), { type: "action", id: A, name: "discard" }, tick(2999));
		expect(card(r.state, A).confirmDiscard).toBe(true);
		r = run(r.state, { type: "action", id: A, name: "discard" });
		expect(r.effects).toEqual([{ type: "invoke", action: "discard", id: A, path: A }]);
		expect(card(r.state, A).busy).toBe(true);
		expect(card(r.state, A).confirmDiscard).toBe(false);
	});

	test("auto-reverts after 3 s; the next click arms again instead of deleting", () => {
		let r = run(one(), { type: "action", id: A, name: "discard" }, tick(3000));
		expect(card(r.state, A).confirmDiscard).toBe(false);
		r = run(r.state, { type: "action", id: A, name: "discard" });
		expect(card(r.state, A).confirmDiscard).toBe(true);
		expect(r.effects).toEqual([]);
	});

	test("the confirm window counts down even while hovered (the pointer is on the button)", () => {
		const r = run(
			one(),
			{ type: "hover", id: A, on: true },
			{ type: "action", id: A, name: "discard" },
			tick(3000),
		);
		expect(card(r.state, A).confirmDiscard).toBe(false);
	});

	test("any other action cancels the pending confirmation", () => {
		const r = run(one(), { type: "action", id: A, name: "discard" }, { type: "action", id: A, name: "copy" });
		expect(card(r.state, A).confirmDiscard).toBe(false);
	});
});

describe("capture actions", () => {
	test("send, save, annotate, reveal, pin and ocr go to Rust with the card's path and mark it busy", () => {
		for (const name of ["send", "save", "annotate", "reveal", "pin", "ocr"]) {
			const r = run(one(), { type: "action", id: A, name });
			expect(r.effects).toEqual([{ type: "invoke", action: name, id: A, path: A }]);
			expect(card(r.state, A).busy).toBe(true);
			expect(card(r.state, A).pending).toBe(name);
		}
	});

	test("the action targets the card it came from, not another one", () => {
		const r = run(initialState(), stackOf([cap(A), cap(B)]), { type: "action", id: B, name: "send" });
		expect(r.effects).toEqual([{ type: "invoke", action: "send", id: B, path: B }]);
		expect(card(r.state, A).busy).toBe(false);
	});

	test("annotate, pin and ocr are refused for recordings (no editor, no window of a video, no text)", () => {
		const rec = one({}, { kind: "recording", secs: 3 });
		for (const name of ["annotate", "pin", "ocr"]) {
			const r = run(rec, { type: "action", id: A, name });
			expect(r.effects).toEqual([]);
			expect(card(r.state, A).busy).toBe(false);
		}
	});

	test("GIFs cannot be annotated, pinned or read either; they can still be copied and sent", () => {
		const gif = one({}, { kind: "gif", secs: 3 });
		for (const name of ["annotate", "pin", "ocr"])
			expect(run(gif, { type: "action", id: A, name }).effects).toEqual([]);
		for (const name of ["copy", "send"])
			expect(run(gif, { type: "action", id: A, name }).effects).toHaveLength(1);
	});

	test("close emits close for that card, even while it is busy", () => {
		const busy = run(one(), { type: "action", id: A, name: "save" }).state;
		expect(run(busy, { type: "action", id: A, name: "close" }).effects).toEqual([
			{ type: "close", id: A, card: "capture" },
		]);
	});

	test("a second action on a busy card is ignored (no double send)", () => {
		const r = run(one(), { type: "action", id: A, name: "send" }, { type: "action", id: A, name: "send" });
		expect(r.effects).toHaveLength(1);
	});

	test("actions on an empty stack do nothing", () => {
		const r = reduce(initialState(), { type: "action", id: A, name: "copy" });
		expect(r.effects).toEqual([]);
	});

	test("actionOk of a non-copy action clears busy and keeps the card (Rust decides to remove it)", () => {
		const busy = run(one(), { type: "action", id: A, name: "send" });
		const r = run(busy.state, { type: "actionOk", id: A });
		expect(card(r.state, A).busy).toBe(false);
		expect(card(r.state, A).copied).toBe(false);
		expect(r.effects).toEqual([]);
	});
});

describe("errors", () => {
	test("actionErr keeps the card, shows the message, clears busy", () => {
		const r = run(
			one(),
			{ type: "action", id: A, name: "save" },
			{ type: "actionErr", id: A, message: "disk full" },
		);
		expect(r.state.cards).toHaveLength(1);
		expect(card(r.state, A).error).toBe("disk full");
		expect(card(r.state, A).busy).toBe(false);
	});

	test("an error stops THAT card's countdown (even for minutes) and hover-in alone does not release it", () => {
		let r = run(
			initialState(),
			stackOf([cap(A), cap(B)]),
			tick(1000),
			{ type: "action", id: A, name: "save" },
			{ type: "actionErr", id: A, message: "nope" },
		);
		r = run(r.state, tick(60_000));
		expect(card(r.state, A).remainingMs).toBe(5000);
		expect(r.effects).toEqual([{ type: "close", id: B, card: "capture" }]);
		r = run(r.state, { type: "hover", id: A, on: true }, tick(60_000));
		expect(card(r.state, A).remainingMs).toBe(5000);
		expect(card(r.state, A).error).toBe("nope");
	});

	test("hover-out releases the stop; the message stays and the countdown resumes with the remaining time", () => {
		let r = run(
			one(),
			tick(1000),
			{ type: "action", id: A, name: "save" },
			{ type: "actionErr", id: A, message: "nope" },
			{ type: "hover", id: A, on: true },
			{ type: "hover", id: A, on: false },
		);
		expect(card(r.state, A).error).toBe("nope");
		r = run(r.state, tick(1000));
		expect(card(r.state, A).remainingMs).toBe(4000);
		r = run(r.state, tick(4000));
		expect(r.effects).toEqual([{ type: "close", id: A, card: "capture" }]);
	});

	test("with dismissSecs 0 an error card still never auto-dismisses", () => {
		let r = run(
			one({ dismissSecs: 0 }),
			{ type: "action", id: A, name: "save" },
			{ type: "actionErr", id: A, message: "x" },
		);
		r = run(r.state, { type: "hover", id: A, on: false }, tick(3600_000));
		expect(r.effects).toEqual([{ type: "hover", hovering: false }]);
		expect(r.state.cards).toHaveLength(1);
	});

	test("a new action clears the previous error", () => {
		const r = run(
			one(),
			{ type: "action", id: A, name: "save" },
			{ type: "actionErr", id: A, message: "nope" },
			{ type: "action", id: A, name: "save" },
		);
		expect(card(r.state, A).error).toBe(null);
		expect(card(r.state, A).busy).toBe(true);
	});

	test("an error of the discard delete reverts confirmDiscard and keeps the card", () => {
		const r = run(
			one(),
			{ type: "action", id: A, name: "discard" },
			{ type: "action", id: A, name: "discard" },
			{ type: "actionErr", id: A, message: "read-only volume" },
		);
		expect(card(r.state, A).error).toBe("read-only volume");
		expect(card(r.state, A).confirmDiscard).toBe(false);
	});
});

describe("GIF sheet", () => {
	const rec = () => one({}, { kind: "recording", secs: 8 });

	test("only recordings open the sheet", () => {
		expect(card(run(one(), { type: "openGifSheet", id: A }).state, A).sheet).toBe(null);
		expect(card(run(rec(), { type: "openGifSheet", id: A }).state, A).sheet).toBe("gif");
	});

	test("progress is tracked and clamped on the card whose sheet is open; closing the sheet resets it", () => {
		const two = run(initialState(), stackOf([cap(A, {}, { kind: "recording", secs: 8 }), cap(B)])).state;
		let r = run(two, { type: "openGifSheet", id: A }, { type: "gifProgress", done: 3, total: 10 });
		expect(card(r.state, A).progress).toEqual({ done: 3, total: 10 });
		expect(card(r.state, B).progress).toBe(null);
		r = run(r.state, { type: "gifProgress", done: 99, total: 10 });
		expect(card(r.state, A).progress).toEqual({ done: 10, total: 10 });
		r = run(r.state, { type: "closeSheet", id: A });
		expect(card(r.state, A).progress).toBe(null);
	});

	test("a progress event with total 0 is ignored (no division by zero in the bar)", () => {
		const r = run(rec(), { type: "openGifSheet", id: A }, { type: "gifProgress", done: 0, total: 0 });
		expect(card(r.state, A).progress).toBe(null);
	});
});

describe("GIF export cancel", () => {
	const rec = () => one({}, { kind: "recording", secs: 8 });
	const exporting = () => run(rec(), { type: "openGifSheet", id: A }, { type: "exportStart", id: A }).state;

	test("an export is tracked per card; only an open GIF sheet can start one", () => {
		expect(card(exporting(), A).exporting).toBe(true);
		expect(card(run(rec(), { type: "exportStart", id: A }).state, A).exporting).toBe(false);
		expect(card(run(one(), { type: "exportStart", id: A }).state, A).exporting).toBe(false);
	});

	test("Escape while exporting cancels the encode AND closes the sheet", () => {
		const r = run(exporting(), { type: "escape" });
		expect(r.effects).toEqual([{ type: "cancelExport" }]);
		expect(card(r.state, A).sheet).toBe(null);
		expect(card(r.state, A).exporting).toBe(false);
		expect(card(r.state, A).progress).toBe(null);
		// the card itself stays: one Esc cancels, the next closes the card
		expect(r.state.cards).toHaveLength(1);
		expect(run(r.state, { type: "escape" }).effects).toEqual([{ type: "close", id: A, card: "capture" }]);
	});

	test("Escape on a sheet that is NOT exporting only closes the sheet (nothing to cancel)", () => {
		const r = run(rec(), { type: "openGifSheet", id: A }, { type: "escape" });
		expect(r.effects).toEqual([]);
		expect(card(r.state, A).sheet).toBe(null);
	});

	test("a cancelled export that rejects later closes the sheet quietly: no error, not busy, countdown resumes", () => {
		const r = run(exporting(), { type: "exportCancelled", id: A }, tick(1000));
		expect(card(r.state, A).sheet).toBe(null);
		expect(card(r.state, A).exporting).toBe(false);
		expect(card(r.state, A).error).toBe(null);
		expect(card(r.state, A).remainingMs).toBe(5000);
	});

	test("a finished export clears the flag but leaves the sheet open for its 'Saved' line", () => {
		const r = run(exporting(), { type: "exportEnd", id: A });
		expect(card(r.state, A).exporting).toBe(false);
		expect(card(r.state, A).sheet).toBe("gif");
	});

	test("closing the sheet by any route clears the exporting flag", () => {
		expect(card(run(exporting(), { type: "closeSheet", id: A }).state, A).exporting).toBe(false);
	});

	test("only the card whose sheet is exporting is cancelled", () => {
		const two = run(initialState(), stackOf([cap(A, {}, { kind: "recording", secs: 8 }), cap(B)])).state;
		const r = run(two, { type: "openGifSheet", id: A }, { type: "exportStart", id: A }, { type: "escape" });
		expect(r.effects).toEqual([{ type: "cancelExport" }]);
		expect(card(r.state, B).sheet).toBe(null);
	});

	// Contract with `gif.rs`: the page recognizes the cancel rejection by this exact sentence.
	test("GIF_CANCELLED is the exact message Rust rejects a cancelled export with", () => {
		const gif = readFileSync(
			join(import.meta.dir, "../../../../../crates/atlas-desktop/src/shots/gif.rs"),
			"utf8",
		);
		expect(gif).toContain(`pub const CANCELLED: &str = "${GIF_CANCELLED}";`);
	});
});

describe("escape", () => {
	test("Escape closes the sheet first, then the newest card", () => {
		let r = run(rec2(), { type: "openGifSheet", id: A }, { type: "escape" });
		expect(card(r.state, A).sheet).toBe(null);
		expect(r.effects).toEqual([]);
		r = run(r.state, { type: "escape" });
		expect(r.effects).toEqual([{ type: "close", id: B, card: "capture" }]);
	});

	function rec2() {
		return run(initialState(), stackOf([cap(A, {}, { kind: "recording", secs: 8 }), cap(B)])).state;
	}

	test("Escape cancels a pending discard confirmation before closing anything", () => {
		let r = run(one(), { type: "action", id: A, name: "discard" }, { type: "escape" });
		expect(card(r.state, A).confirmDiscard).toBe(false);
		expect(r.effects).toEqual([]);
		r = run(r.state, { type: "escape" });
		expect(r.effects).toEqual([{ type: "close", id: A, card: "capture" }]);
	});

	test("Escape closes the newest card: the one nearest the corner (the last Rust lists)", () => {
		const r = run(initialState(), stackOf([cap(A), cap(B), txt("text:9")]), { type: "escape" });
		expect(r.effects).toEqual([{ type: "close", id: "text:9", card: "text" }]);
	});

	test("Escape on an empty stack does nothing", () => {
		expect(reduce(initialState(), { type: "escape" }).effects).toEqual([]);
	});
});

describe("text cards", () => {
	const text = () => run(initialState(), stackOf([txt()])).state;

	test("copy again and send go to Rust by id and mark the card busy", () => {
		for (const name of ["copy", "send"]) {
			const r = run(text(), { type: "textAction", id: "text:1", name });
			expect(r.effects).toEqual([{ type: "invokeText", action: name, id: "text:1" }]);
			expect(card(r.state, "text:1").busy).toBe(true);
		}
	});

	test("copying again flashes the copied badge like a capture's copy", () => {
		const r = run(
			text(),
			{ type: "textAction", id: "text:1", name: "copy" },
			{ type: "actionOk", id: "text:1" },
		);
		expect(card(r.state, "text:1").copied).toBe(true);
		expect(card(r.state, "text:1").copiedMs).toBe(COPIED_MS);
	});

	test("an empty read offers nothing to copy or send, so those actions never reach Rust (the clipboard is left alone)", () => {
		const empty = run(
			initialState(),
			stackOf([txt("text:2", { empty: true, preview: "", lines: 0, chars: 0 })]),
		).state;
		for (const name of ["copy", "send"]) {
			const r = run(empty, { type: "textAction", id: "text:2", name });
			expect(r.effects).toEqual([]);
			expect(card(r.state, "text:2").busy).toBe(false);
		}
		// closing it still works
		expect(run(empty, { type: "textAction", id: "text:2", name: "close" }).effects).toEqual([
			{ type: "close", id: "text:2", card: "text" },
		]);
	});

	test("a text card auto-closes after its own countdown, and hover pauses it", () => {
		let r = run(text(), { type: "hover", id: "text:1", on: true }, tick(60_000));
		// Rust is told about the hover (like for a capture card), and 60 s of hover closes nothing.
		expect(r.effects).toEqual([{ type: "hover", hovering: true }]);
		r = run(r.state, { type: "hover", id: "text:1", on: false }, tick(6000));
		expect(r.effects).toContainEqual({ type: "close", id: "text:1", card: "text" });
	});

	test("a capture action sent to a text card (or the reverse) is ignored", () => {
		expect(run(text(), { type: "action", id: "text:1", name: "copy" }).effects).toEqual([]);
		expect(run(one(), { type: "textAction", id: A, name: "copy" }).effects).toEqual([]);
	});

	test("a text card has no GIF sheet and no discard confirmation to cancel", () => {
		expect(run(text(), { type: "openGifSheet", id: "text:1" }).state.cards[0].sheet).toBeUndefined();
		const r = run(text(), { type: "escape" });
		expect(r.effects).toEqual([{ type: "close", id: "text:1", card: "text" }]);
	});

	test("textTitle", () => {
		expect(textTitle({ empty: true, lines: 0, chars: 0 })).toBe("No text found");
		expect(textTitle({ empty: false, lines: 1, chars: 5 })).toBe("Text copied · 1 line · 5 characters");
		expect(textTitle({ empty: false, lines: 3, chars: 40 })).toBe("Text copied · 3 lines · 40 characters");
	});
});

describe("humanBytes", () => {
	test.each([
		[0, "0 B"],
		[812, "812 B"],
		[1023, "1023 B"],
		[1024, "1.0 KB"],
		[1434, "1.4 KB"],
		[1024 * 1024 - 1, "1024 KB"],
		[1024 * 1024, "1.0 MB"],
		[12.3 * 1024 * 1024, "12.3 MB"],
		[1024 ** 3, "1.00 GB"],
		[1.02 * 1024 ** 3, "1.02 GB"],
	])("%p -> %p", (n, out) => expect(humanBytes(n)).toBe(out));

	test("garbage never prints NaN", () => {
		expect(humanBytes(-5)).toBe("0 B");
		expect(humanBytes(Number.NaN)).toBe("0 B");
	});
});

describe("duration", () => {
	test.each([
		[0, "0:00"],
		[7, "0:07"],
		[59, "0:59"],
		[60, "1:00"],
		[65, "1:05"],
		[3599, "59:59"],
		[3600, "1:00:00"],
		[3723, "1:02:03"],
	])("%p s -> %p", (s, out) => expect(duration(s)).toBe(out));

	test("fractions round down, negatives and NaN clamp to 0:00", () => {
		expect(duration(7.9)).toBe("0:07");
		expect(duration(-3)).toBe("0:00");
		expect(duration(Number.NaN)).toBe("0:00");
	});
});

describe("title", () => {
	test("screenshot shows the pixel size with a real multiplication sign", () => {
		expect(title(entry())).toBe("Screenshot 1440 × 900");
	});
	test("recording shows its duration", () => {
		expect(title(entry({ kind: "recording", secs: 12 }))).toBe("Recording 0:12");
	});
	test("gif shows duration and size", () => {
		expect(title(entry({ kind: "gif", secs: 8, bytes: 2.1 * 1024 * 1024 }))).toBe("GIF 0:08 · 2.1 MB");
	});
	test("a video without a known duration omits it instead of printing null", () => {
		expect(title(entry({ kind: "recording", secs: null }))).toBe("Recording");
		expect(title(entry({ kind: "gif", secs: null, bytes: 2048 }))).toBe("GIF · 2.0 KB");
	});
});

describe("kindIcon", () => {
	test("one icon name per kind, all different", () => {
		const icons = ["screenshot", "recording", "gif"].map(kindIcon);
		expect(new Set(icons).size).toBe(3);
		for (const i of icons) expect(i).toMatch(/^[a-z-]+$/);
	});
});
