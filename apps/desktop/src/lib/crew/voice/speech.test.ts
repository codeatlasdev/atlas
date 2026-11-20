// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { describe, expect, test } from "bun:test";

// Reading a reply aloud (#648): what is said, in which language, and what silences it. The shell's synthesizer is a
// fake that records; the window is a plain EventTarget. The rules under test: nothing is spoken for an empty text,
// any key or click stops it at once, a stop that races a start wins, and a synthesizer that is missing says nothing.
const { Speech } = await import("./speech.svelte.ts");

function rig({ available = true, lang = "en" } = {}) {
	const log: string[] = [];
	let ended: ((id: number) => void) | null = null;
	let nextId = 1;
	let gate: Promise<void> | null = null;
	const native = {
		async available() {
			return available;
		},
		async speak(text: string, l: string) {
			log.push(`speak:${l}:${text}`);
			if (gate) await gate;
			return nextId++;
		},
		async stop() {
			log.push("stop");
		},
		async onEnded(cb) {
			ended = cb;
			return () => {
				ended = null;
			};
		},
	};
	const win = new EventTarget();
	const speech = new Speech({ native, lang: () => lang, errorText: String });
	return {
		speech,
		win,
		log,
		end: (id: number) => ended?.(id),
		listening: () => ended !== null,
		hold(): () => void {
			let open!: () => void;
			gate = new Promise((r) => (open = r));
			return () => {
				gate = null;
				open();
			};
		},
	};
}

const settle = async () => {
	for (let i = 0; i < 20; i++) await Promise.resolve();
};

describe("what is spoken", () => {
	test("the text goes to the synthesizer in the app's language, and the indicator is on", async () => {
		const r = rig({ lang: "pt-BR" });
		await r.speech.attach(r.win);
		await r.speech.speak("  Terminei a tarefa.  ");
		expect(r.log).toEqual(["speak:pt-BR:Terminei a tarefa."]);
		expect(r.speech.speaking).toBe(true);
	});

	test("an empty or blank text says nothing and cuts nothing", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		await r.speech.speak("first");
		r.log.length = 0;
		for (const t of ["", "   ", "\n\t"]) await r.speech.speak(t);
		expect(r.log).toEqual([]);
		expect(r.speech.speaking).toBe(true);
	});

	test("a system with no speech engine never tries", async () => {
		const r = rig({ available: false });
		await r.speech.attach(r.win);
		expect(r.speech.available).toBe(false);
		await r.speech.speak("hello");
		expect(r.log).toEqual([]);
		expect(r.speech.speaking).toBe(false);
	});

	test("a refusal of the synthesizer is shown and nothing is marked as speaking", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		const bad = new Speech({
			native: {
				available: async () => true,
				speak: async () => {
					throw "no voice for pt-BR";
				},
				stop: async () => {},
				onEnded: async () => () => {},
			},
			lang: () => "pt-BR",
			errorText: String,
		});
		await bad.attach(r.win);
		await bad.speak("oi");
		expect(bad.speaking).toBe(false);
		expect(bad.error).toContain("no voice for pt-BR");
	});
});

describe("what silences it", () => {
	for (const ev of ["keydown", "pointerdown", "mousedown"]) {
		test(`a ${ev} anywhere in the window stops it at once`, async () => {
			const r = rig();
			await r.speech.attach(r.win);
			await r.speech.speak("hello");
			r.log.length = 0;
			r.win.dispatchEvent(new Event(ev));
			expect(r.speech.speaking).toBe(false);
			expect(r.log).toEqual(["stop"]);
		});
	}

	test("input while nothing is being said does not touch the synthesizer", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		r.win.dispatchEvent(new Event("keydown"));
		expect(r.log).toEqual([]);
	});

	test("a stop that arrives while the start is still in flight wins", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		const open = r.hold();
		const said = r.speech.speak("hello");
		await settle();
		r.speech.stop();
		open();
		await said;
		expect(r.speech.speaking).toBe(false);
	});

	test("the end of an utterance clears the indicator; the end of an older one does not", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		await r.speech.speak("one");
		await r.speech.speak("two");
		r.end(1);
		expect(r.speech.speaking).toBe(true);
		r.end(2);
		expect(r.speech.speaking).toBe(false);
	});

	test("leaving the chat silences it and stops listening to the window and the shell", async () => {
		const r = rig();
		await r.speech.attach(r.win);
		await r.speech.speak("hello");
		r.log.length = 0;
		r.speech.dispose();
		expect(r.speech.speaking).toBe(false);
		expect(r.log).toEqual(["stop"]);
		expect(r.listening()).toBe(false);
		r.log.length = 0;
		await r.speech.speak("again");
		r.win.dispatchEvent(new Event("keydown"));
		expect(r.log).toEqual(["speak:en:again"]);
	});
});
