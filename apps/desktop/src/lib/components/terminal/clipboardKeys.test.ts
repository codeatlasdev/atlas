// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { copyText, keyAction } from "./clipboardKeys.ts";

// Incident (issue #28): on Linux the terminal is xterm.js and nothing in it wrote the clipboard, so neither
// Ctrl+Shift+C nor Ctrl+C copied, and there was no Ctrl+Shift+V. Ctrl+C must keep meaning SIGINT.
const key = (over) => ({
	type: "keydown",
	key: "",
	ctrlKey: false,
	shiftKey: false,
	altKey: false,
	metaKey: false,
	...over,
});
const linux = (hasSelection) => ({ mac: false, hasSelection });

test("Ctrl+Shift+C copies on Linux, with or without a selection check, in either letter case", () => {
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "C" }), linux(true))).toBe("copy");
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "c" }), linux(true))).toBe("copy");
	// No selection: the chord is still ours (it never means SIGINT), so it must not reach the program as ^C.
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "C" }), linux(false))).toBe("swallow");
});

test("Ctrl+C copies only while text is selected; otherwise it passes through as SIGINT", () => {
	expect(keyAction(key({ ctrlKey: true, key: "c" }), linux(true))).toBe("copy");
	expect(keyAction(key({ ctrlKey: true, key: "c" }), linux(false))).toBeNull();
});

test("Ctrl+Shift+V pastes on Linux, and plain Ctrl+V stays with the program (readline quoted-insert)", () => {
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "V" }), linux(false))).toBe("paste");
	expect(keyAction(key({ ctrlKey: true, key: "v" }), linux(false))).toBeNull();
});

test("Alt, Meta or another letter never trigger a copy or paste", () => {
	expect(keyAction(key({ ctrlKey: true, altKey: true, key: "c" }), linux(true))).toBeNull();
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, metaKey: true, key: "C" }), linux(true))).toBeNull();
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "X" }), linux(true))).toBeNull();
	expect(keyAction(key({ type: "keyup", ctrlKey: true, shiftKey: true, key: "C" }), linux(true))).toBeNull();
});

test("on macOS ⌘C/⌘V are not handled here (the native Edit menu owns them) and Ctrl+C is always SIGINT", () => {
	const mac = { mac: true, hasSelection: true };
	expect(keyAction(key({ metaKey: true, key: "c" }), mac)).toBeNull();
	expect(keyAction(key({ metaKey: true, key: "v" }), mac)).toBeNull();
	expect(keyAction(key({ ctrlKey: true, key: "c" }), mac)).toBeNull();
	expect(keyAction(key({ ctrlKey: true, shiftKey: true, key: "C" }), mac)).toBeNull();
});

// --- writing the clipboard -------------------------------------------------------------------------------------

const realClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const realExec = document.execCommand;
afterEach(() => {
	if (realClipboard) Object.defineProperty(navigator, "clipboard", realClipboard);
	else delete navigator.clipboard;
	document.execCommand = realExec;
});

test("copyText uses the async clipboard when it works", async () => {
	const written = [];
	Object.defineProperty(navigator, "clipboard", {
		configurable: true,
		value: { writeText: async (t) => void written.push(t) },
	});
	expect(await copyText("hello-atlas")).toBe(true);
	expect(written).toEqual(["hello-atlas"]);
});

// WebKitGTK rejects (or lacks) navigator.clipboard.writeText depending on the secure-context/gesture state:
// the textarea + execCommand("copy") path is the one that works inside a key or mouse handler.
test("copyText falls back to execCommand('copy') of the real text when the async clipboard rejects", async () => {
	Object.defineProperty(navigator, "clipboard", {
		configurable: true,
		value: { writeText: () => Promise.reject(new DOMException("denied", "NotAllowedError")) },
	});
	let copied = null;
	document.execCommand = (cmd) => {
		// execCommand copies the selected range of the focused/selected textarea.
		const ta = document.querySelector("textarea");
		copied = { cmd, text: ta.value.slice(ta.selectionStart, ta.selectionEnd) };
		return true;
	};
	expect(await copyText("line1\nline2")).toBe(true);
	expect(copied).toEqual({ cmd: "copy", text: "line1\nline2" });
	expect(document.querySelector("textarea")).toBeNull(); // the scratch element is removed again
});

test("copyText falls back when navigator.clipboard does not exist, and reports false when both paths fail", async () => {
	Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
	document.execCommand = () => true;
	expect(await copyText("x")).toBe(true);
	document.execCommand = () => false;
	expect(await copyText("x")).toBe(false);
	expect(document.querySelector("textarea")).toBeNull();
});

test("copyText ignores an empty selection and never touches the clipboard", async () => {
	let touched = false;
	Object.defineProperty(navigator, "clipboard", {
		configurable: true,
		value: { writeText: async () => (touched = true) },
	});
	expect(await copyText("")).toBe(false);
	expect(touched).toBe(false);
});
