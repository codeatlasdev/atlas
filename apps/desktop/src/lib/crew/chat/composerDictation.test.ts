// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, describe, expect, test } from "bun:test";

// The message box with a microphone, against the real component and the real `Dictation`; only the shell's
// microphone and the crew server are fakes. What is asserted is what a person (or a screen reader) meets: the
// buttons and their names, the live indicator with the engine's name, the text landing at the caret, Escape taking it
// back, the chord, and the microphone being given back when the box goes away.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load
// (a static import is hoisted above it and cannot compile a .svelte file).
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../../test/ComposerDictationHarness.svelte");
const { default: Composer } = await import("./Composer.svelte");
const { Dictation } = await import("../voice/dictation.svelte.ts");
const { IS_MAC } = await import("#lib/keys.ts");

const STATUS = {
	enabled: true,
	engine: { kind: "whisper_cpp", label: "whisper.cpp (small)", model: "small", ready: true, detail: null },
	maxSeconds: 300,
};

// The clock of the session, turned by the test: a hold longer than a tap is a number, not a wait.
let clock = 1000;

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
});

const settle = async () => {
	for (let i = 0; i < 30; i++) await Promise.resolve();
	flushSync();
	await tick();
};

function setup({ text = "", status = STATUS, mode = "hold", unsupported = false } = {}) {
	const native = {
		file: new Uint8Array(100).fill(1),
		live: false,
		started: 0,
		cancelled: 0,
		async start() {
			native.started++;
			native.live = true;
		},
		async stop() {
			if (!native.live) return null;
			native.live = false;
			return { reason: "requested", bytes: 100, seconds: 1 };
		},
		async cancel() {
			native.cancelled++;
			native.live = false;
		},
		async read(offset, max) {
			return { data: native.file.slice(offset, offset + max), total: native.file.length, live: native.live };
		},
		async openSettings() {},
		async onLevel() {
			return () => {};
		},
		async onStopped() {
			return () => {};
		},
	};
	const dictation = new Dictation({
		native,
		status: () => status,
		coded: async (code) => code,
		errorText: String,
		mode: () => mode,
		now: () => clock,
		// The live preview is covered in voice/dictation.test.ts: no timer here.
		later: () => () => {},
		async call(cmd, args) {
			if (cmd === "crewmate_voice_put") return { upload: "u1", bytes: 0 };
			if (cmd === "crewmate_transcribe") return { text: "open src slash main", engine: { label: "whisper.cpp (small)" } };
			throw new Error(cmd);
		},
	});
	const target = document.createElement("div");
	document.body.append(target);
	const props = { value: text };
	let sends = 0;
	const app = flushSync(() =>
		mount(Harness, {
			target,
			props: {
				initial: text,
				dictation,
				unsupported,
				ondraft: (v: string) => (props.value = v),
				onsend: () => sends++,
			},
		}),
	);
	done = () => (unmount(app), target.remove());
	return { target, dictation, native, props, sends: () => sends };
}

const box = (t) => t.querySelector("textarea");
const mic = (t) => t.querySelector("button.pill-mic");
const chord = (el, type = "keydown") =>
	el.dispatchEvent(
		new KeyboardEvent(type, {
			code: "KeyM",
			key: "M",
			shiftKey: true,
			metaKey: IS_MAC,
			ctrlKey: !IS_MAC,
			bubbles: true,
			cancelable: true,
		}),
	);
const esc = (el) =>
	el.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }));

describe("the microphone button", () => {
	test("is named for what it does, and says why it is off on a crew server that cannot transcribe", async () => {
		const { target } = setup();
		expect(mic(target).getAttribute("aria-label")).toBe("Dictate");
		expect(mic(target).disabled).toBe(false);
		done();
		const off = setup({ unsupported: true });
		expect(mic(off.target).disabled).toBe(true);
		expect(mic(off.target).getAttribute("aria-label")).toContain("Update it to dictate");
	});

	test("a message box without a dictation has no microphone", async () => {
		const target = document.createElement("div");
		document.body.append(target);
		const app = flushSync(() =>
			mount(Composer, { target, props: { value: "", placeholder: "m", label: "l", onsend: () => {} } }),
		);
		done = () => (unmount(app), target.remove());
		expect(target.querySelector("button.pill-mic")).toBeNull();
	});
});

describe("dictating with the chord", () => {
	test("while it listens: a live indicator of its own, the engine's name, a stop button, and the start is announced", async () => {
		const { target } = setup();
		chord(box(target));
		await settle();
		expect(target.querySelector("[data-mic-live]")?.textContent).toContain("Microphone is on");
		expect(target.querySelector("[data-dictation-engine]")?.textContent).toContain("whisper.cpp (small)");
		expect(mic(target).getAttribute("aria-label")).toBe("Stop dictating");
		const status = target.querySelector("[role=status]");
		expect(status.getAttribute("aria-live")).toBe("polite");
		expect(status.textContent).toContain("Dictation started");
		// The meter is decoration: not read out.
		expect(target.querySelector(".meter").getAttribute("aria-hidden")).toBe("true");
	});

	test("releasing a held chord puts the text at the caret and does not send it", async () => {
		const { target, props, sends, dictation } = setup({ text: "fix the" });
		box(target).setSelectionRange(7, 7);
		chord(box(target));
		await settle();
		clock += 400; // held longer than a tap: the release means stop
		chord(window, "keyup");
		await settle();
		expect(props.value).toBe("fix the open src slash main");
		expect(box(target).value).toBe("fix the open src slash main");
		expect(sends()).toBe(0);
		expect(dictation.dictated).toBe(true);
		expect(target.querySelector("[data-mic-live]")).toBeNull();
	});

	test("Escape takes the dictated text back out and still does not send", async () => {
		const { target, props, sends, dictation } = setup({ text: "fix the foo bug" });
		box(target).setSelectionRange(8, 11);
		chord(box(target));
		await settle();
		chord(window, "keyup"); // a quick tap: latched
		chord(box(target)); // the second press stops it
		await settle();
		expect(props.value).toBe("fix the open src slash main bug");
		esc(box(target));
		await settle();
		expect(props.value).toBe("fix the foo bug");
		expect(dictation.dictated).toBe(false);
		expect(sends()).toBe(0);
	});

	test("Escape while listening cancels and the microphone is given back", async () => {
		const { target, native, props } = setup({ text: "keep" });
		chord(box(target));
		await settle();
		esc(box(target));
		await settle();
		expect(native.cancelled).toBe(1);
		expect(props.value).toBe("keep");
		expect(target.querySelector("[data-mic-live]")).toBeNull();
		expect(target.querySelector("[role=status]").textContent).toContain("cancelled");
	});

	test("typing while the draft holds dictated text ends the undo: Escape then leaves the draft alone", async () => {
		const { target, props } = setup();
		chord(box(target));
		await settle();
		chord(window, "keyup");
		chord(box(target));
		await settle();
		box(target).value = props.value + " please";
		box(target).dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		esc(box(target));
		await settle();
		expect(props.value).toBe("open src slash main please");
	});
});

describe("the microphone is given back", () => {
	test("when the message box goes away while listening", async () => {
		const { target, native } = setup();
		chord(box(target));
		await settle();
		expect(native.live).toBe(true);
		done();
		done = undefined;
		await settle();
		expect(native.cancelled).toBe(1);
		expect(native.live).toBe(false);
	});
});

describe("refusals", () => {
	test("a crew with dictation switched off says so in the app's words and never opens the microphone", async () => {
		const { target, native } = setup({ status: { ...STATUS, enabled: false } });
		chord(box(target));
		await settle();
		expect(target.querySelector("[role=alert]")?.textContent).toContain("crew.voice.off");
		expect(native.started).toBe(0);
		// Escape dismisses it.
		esc(box(target));
		await settle();
		expect(target.querySelector("[role=alert]")).toBeNull();
	});
});
