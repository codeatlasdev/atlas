// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, beforeEach, expect, jest, test } from "bun:test";
import { Terminal } from "@xterm/xterm";
import { imeStagingKey, keepTextareaEmpty } from "./xtermInput.ts";

// Incident (issue #23, Linux = xterm.js on WebKitGTK + IBus, pt-BR dead keys): typing `´` then `e` replayed
// everything typed earlier plus runs of spaces ("sóáá      é   ç    é  ó"). xterm.js never empties its hidden
// textarea: capitals and spaces stay in it (it only `preventDefault`s the other keys), and the dead key reaches
// it as a keyCode-229 keydown outside a composition, whose `CompositionHelper._handleAnyTextareaChanges`
// diffs the WHOLE textarea against its previous value and emits that as input
// (https://github.com/xtermjs/xterm.js/issues/6078, #6012). These tests drive the real @xterm/xterm with the
// event order WebKitGTK/IBus produces and assert the exact bytes that would reach the PTY. xterm defers its
// textarea reads with `setTimeout(0)`, so time is driven with fake timers instead of sleeping.

interface Rig {
	ta: HTMLTextAreaElement;
	/** Every `onData` payload, i.e. what `Terminal.svelte` writes to the PTY. */
	sent: string[];
	key(type: string, k: string, keyCode: number, init?: KeyboardEventInit): KeyboardEvent;
	/** A key the keyboard types itself (no IME): keydown, default insert into the textarea, keypress, keyup. */
	type(ch: string): void;
	/** `´` + vowel on WebKitGTK/IBus: both keydowns are keyCode 229 and the IM edits the textarea in place. */
	deadAccent(accented: string): void;
}

const terms: Terminal[] = [];

function rig(withFix: boolean): Rig {
	const host = document.createElement("div");
	document.body.append(host);
	const term = new Terminal({ allowProposedApi: true });
	terms.push(term);
	term.open(host);
	const sent: string[] = [];
	term.onData((d) => sent.push(d));
	const ta = term.textarea as HTMLTextAreaElement;
	ta.focus();
	if (withFix) {
		const input = keepTextareaEmpty(term);
		// What Terminal.svelte's single custom key handler does with it.
		term.attachCustomKeyEventHandler((e) => !imeStagingKey(e, input.composing()));
	}

	const key: Rig["key"] = (type, k, keyCode, init = {}) => {
		const ev = new KeyboardEvent(type, { key: k, bubbles: true, cancelable: true, ...init });
		Object.defineProperty(ev, "keyCode", { get: () => keyCode });
		ta.dispatchEvent(ev);
		return ev;
	};
	return {
		ta,
		sent,
		key,
		type(ch) {
			const keyCode = ch === " " ? 32 : ch.toUpperCase().charCodeAt(0);
			const down = key("keydown", ch, keyCode, { shiftKey: ch !== ch.toLowerCase() });
			if (!down.defaultPrevented) ta.value += ch;
			const press = new KeyboardEvent("keypress", { key: ch, bubbles: true, cancelable: true });
			Object.defineProperty(press, "charCode", { get: () => ch.charCodeAt(0) });
			ta.dispatchEvent(press);
			key("keyup", ch, keyCode);
			jest.advanceTimersByTime(1);
		},
		deadAccent(accented) {
			key("keydown", "Dead", 229);
			ta.value += "´"; // staged preedit: no input event, nothing committed yet
			key("keyup", "Dead", 229);
			jest.advanceTimersByTime(1);
			key("keydown", "Process", 229);
			// The IM replaces the staged `´` with the composed character and WebKit reports the commit as an input.
			ta.value = ta.value.slice(0, -1) + accented;
			ta.dispatchEvent(new InputEvent("input", { data: accented, inputType: "insertText", bubbles: true }));
			key("keyup", "e", 229);
			jest.advanceTimersByTime(1);
		},
	};
}

beforeEach(() => jest.useFakeTimers());
afterEach(() => {
	terms.splice(0).forEach((t) => t.dispose());
	jest.useRealTimers();
});

test("dead acute + e after typed words reaches the PTY as the accent alone, not the textarea residue", () => {
	const r = rig(true);
	for (const c of "Foi so ") r.type(c);
	const before = r.sent.join("");
	r.deadAccent("é");
	// Only the composed accent may follow what was already sent; the dead key itself sends nothing.
	expect(r.sent.join("").slice(before.length)).toBe("é");
});

test("three accents in a row never replay earlier ones (the accumulation in the report)", () => {
	const r = rig(true);
	for (const c of "Ele ") r.type(c);
	const before = r.sent.join("");
	r.deadAccent("é");
	r.type(" ");
	r.deadAccent("á");
	r.type(" ");
	r.deadAccent("ó");
	expect(r.sent.join("").slice(before.length)).toBe("é á ó");
});

test("the hidden textarea holds no residue once a key has been delivered (nothing left to replay)", () => {
	const r = rig(true);
	for (const c of "Hi There ") r.type(c);
	expect(r.ta.value).toBe("");
});

test("without the fix the same sequence replays the textarea (guards the harness: it must reproduce the bug)", () => {
	const r = rig(false);
	for (const c of "Foi so ") r.type(c);
	const before = r.sent.join("");
	r.deadAccent("é");
	expect(r.sent.join("").slice(before.length)).not.toBe("é");
});

test("a composition in flight is never cleared under the IME, and is emptied only after xterm sent it", () => {
	const r = rig(true);
	r.ta.dispatchEvent(new CompositionEvent("compositionstart", { data: "", bubbles: true }));
	r.ta.value = "ni";
	r.ta.dispatchEvent(new CompositionEvent("compositionupdate", { data: "ni", bubbles: true }));
	r.key("keyup", "i", 229);
	jest.advanceTimersByTime(1);
	expect(r.ta.value).toBe("ni");
	r.ta.value = "你";
	r.ta.dispatchEvent(new CompositionEvent("compositionend", { data: "你", bubbles: true }));
	jest.advanceTimersByTime(1);
	expect(r.sent.join("")).toBe("你");
	expect(r.ta.value).toBe("");
});

test("a dead-key + vowel commit through insertText sends the composed character exactly once", () => {
	const r = rig(true);
	r.key("keydown", "Dead", 229);
	r.key("keyup", "Dead", 229);
	r.key("keydown", "e", 229);
	r.ta.value += "é";
	r.ta.dispatchEvent(new InputEvent("input", { data: "é", inputType: "insertText", bubbles: true }));
	r.key("keyup", "e", 229);
	jest.advanceTimersByTime(1);
	expect(r.sent.join("")).toBe("é");
});

test("only keyCode-229 keydowns outside a composition are IM staging (every other key still reaches xterm)", () => {
	const ev = (type: string, keyCode: number) => ({ type, keyCode });
	expect(imeStagingKey(ev("keydown", 229), false)).toBe(true);
	expect(imeStagingKey(ev("keydown", 229), true)).toBe(false); // xterm's CompositionHelper owns it
	expect(imeStagingKey(ev("keyup", 229), false)).toBe(false);
	expect(imeStagingKey(ev("keydown", 13), false)).toBe(false); // Enter
	expect(imeStagingKey(ev("keydown", 8), false)).toBe(false); // Backspace
});
