// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { SEED, restSink, sinkText } from "./sink.ts";

// Incident (linux idle, #289): a focused editable keeps WebKit's caret animator running even when the element has no
// area, which arms the display link: ~80 wakeups a second (the UI process's VBlankMonitor thread, the web process's
// main and compositor threads, IPC both ways), 0.4-0.5 % of a core, with the terminal idle. A range selection has no
// caret to animate. So the sink always rests holding one selected placeholder; typed text replaces it.
function sink(value = "") {
	const el = document.createElement("textarea");
	document.body.append(el);
	el.value = value;
	return el;
}

test("a rested sink has a ranged selection over the placeholder, never a collapsed caret", () => {
	const el = sink("");
	restSink(el);
	expect(el.value).toBe(SEED);
	expect([el.selectionStart, el.selectionEnd]).toEqual([0, SEED.length]);
	el.remove();
});

test("resting leaves a sink that is already at rest alone", () => {
	const el = sink(SEED);
	el.setSelectionRange(0, SEED.length);
	let writes = 0;
	const desc = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value");
	Object.defineProperty(el, "value", {
		get: () => desc.get.call(el),
		set: (v) => {
			writes++;
			desc.set.call(el, v);
		},
	});
	restSink(el);
	expect(writes).toBe(0);
	el.remove();
});

test("text typed over the placeholder is read back whole and the sink rests again", () => {
	const el = sink(SEED);
	el.setSelectionRange(0, SEED.length);
	el.setRangeText("é", 0, SEED.length, "end"); // what typing does to a selected range
	expect(sinkText(el.value)).toBe("é");
	restSink(el);
	expect(el.value).toBe(SEED);
	expect(el.selectionEnd - el.selectionStart).toBe(SEED.length);
	el.remove();
});

// The placeholder must never reach the pty: a caret left collapsed beside it (an unhandled arrow key, an IME that
// cancelled) makes the next keystroke land next to it instead of over it.
test("the placeholder is stripped wherever the text landed, and real spaces stay", () => {
	expect(sinkText(`a${SEED}`)).toBe("a");
	expect(sinkText(`${SEED}a`)).toBe("a");
	expect(sinkText(`你${SEED}好`)).toBe("你好");
	expect(sinkText("hello world")).toBe("hello world");
	expect(sinkText(SEED)).toBe("");
	expect(sinkText("")).toBe("");
});

test("a sink whose selection collapsed is brought back to a range", () => {
	const el = sink(SEED);
	el.setSelectionRange(SEED.length, SEED.length);
	restSink(el);
	expect([el.selectionStart, el.selectionEnd]).toEqual([0, SEED.length]);
	el.remove();
});

test("a sink that lost its placeholder (cleared, or text left in it) gets it back", () => {
	for (const v of ["", "x", `x${SEED}`]) {
		const el = sink(v);
		restSink(el);
		expect(el.value).toBe(SEED);
		el.remove();
	}
});
