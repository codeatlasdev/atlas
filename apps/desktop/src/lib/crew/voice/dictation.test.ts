// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import { b64bytes } from "#lib/b64.ts";
import { CHUNK } from "#lib/crew/attachments.ts";

// The dictation session against fakes of everything outside it: a microphone that holds a WAV, a crew server that
// answers `crewmate_voice_put` / `crewmate_transcribe`, a clock the test turns by hand. What is asserted is what the
// person and the server would see: the draft, the dimmed preview, the order and size of the uploads, the errors.
// Dynamic import on purpose: the Svelte loader registered by test/setup.ts must exist before this module loads (a
// static import is hoisted above it and cannot compile a `.svelte.ts` file).
const { Dictation, LATCH_MS, PARTIAL_EVERY_MS } = await import("./dictation.svelte.ts");

const STATUS = {
	enabled: true,
	engine: { kind: "whisper_cpp", label: "whisper.cpp (small)", model: "small", ready: true, detail: null },
	maxSeconds: 300,
};

/** A WAV-shaped byte string: distinguishable bytes, so a lost or repeated piece shows. */
const audio = (n: number) => Uint8Array.from({ length: n }, (_, i) => (i * 7 + 3) % 251);

function rig(opts = {}) {
	let clock = 1000;
	const timers: { at: number; fn: () => void; live: boolean }[] = [];
	const calls: { cmd: string; args: Record<string, unknown> }[] = [];
	const native = {
		file: audio(2000),
		live: true,
		started: 0,
		cancelled: 0,
		startError: null as unknown,
		stopTake: { reason: "requested", bytes: 2000, seconds: 3 },
		levelCb: null as ((l: number) => void) | null,
		stoppedCb: null as ((t: unknown) => void) | null,
		opened: [] as string[],
		async start() {
			native.started++;
			if (native.startError) throw native.startError;
			native.live = true;
		},
		async stop() {
			if (!native.live) return null;
			native.live = false;
			return { ...native.stopTake, bytes: native.file.length };
		},
		async cancel() {
			native.cancelled++;
			native.live = false;
		},
		async read(offset: number, max: number) {
			return { data: native.file.slice(offset, offset + max), total: native.file.length, live: native.live };
		},
		async openSettings() {
			native.opened.push("settings");
		},
		async onLevel(cb) {
			native.levelCb = cb;
			return () => (native.levelCb = null);
		},
		async onStopped(cb) {
			native.stoppedCb = cb;
			return () => (native.stoppedCb = null);
		},
	};
	const server = {
		partialText: "open src",
		finalText: "open src slash main",
		failFinal: null as unknown,
		failPartial: null as unknown,
		uploads: 0,
	};
	const deps = {
		native,
		status: () => opts.status ?? STATUS,
		coded: async (code: string) => `coded:${code}`,
		errorText: (e: unknown) => (e instanceof Error ? e.message : String(e)),
		mode: () => opts.mode ?? "hold",
		now: () => clock,
		later(fn: () => void, ms: number) {
			const t = { at: clock + ms, fn, live: true };
			timers.push(t);
			return () => (t.live = false);
		},
		async call(cmd: string, args: Record<string, unknown>) {
			calls.push({ cmd, args: structuredClone(args) });
			if (cmd === "crewmate_voice_put") {
				if (args.upload === "") server.uploads++;
				return { upload: `u${server.uploads}`, bytes: 0 };
			}
			if (cmd === "crewmate_transcribe") {
				if (args.partial) {
					if (server.failPartial) throw server.failPartial;
					return { text: server.partialText, raw: server.partialText, engine: { kind: "whisper_cpp", label: "whisper.cpp (small)" }, partial: true };
				}
				if (server.failFinal) throw server.failFinal;
				return { text: server.finalText, raw: server.finalText, engine: { kind: "whisper_cpp", label: "whisper.cpp (small)" }, language: "en", duration_ms: 3000, partial: false };
			}
			throw new Error(`unexpected ${cmd}`);
		},
	};
	let draft = { value: "", start: 0, end: 0 };
	const writes: { value: string; cursor: number }[] = [];
	const target = {
		read: () => ({ ...draft }),
		write(value: string, cursor: number) {
			draft = { value, start: cursor, end: cursor };
			writes.push({ value, cursor });
		},
	};
	const d = new Dictation(deps);
	const advance = async (ms: number) => {
		clock += ms;
		for (const t of timers.filter((t) => t.live && t.at <= clock)) {
			t.live = false;
			t.fn();
		}
		await settle();
	};
	return {
		d, native, server, calls, target, writes, advance,
		/** The microphone opens only when the test says so (the system's permission prompt is up). */
		gateStart() {
			const gate = Promise.withResolvers<void>();
			native.start = () => gate.promise;
			return gate;
		},
		/** The server answers the final transcription only when the test says so. */
		gateFinal(reply: unknown) {
			const gate = Promise.withResolvers<unknown>();
			const call = deps.call;
			deps.call = (cmd, args) =>
				cmd === "crewmate_transcribe" && !args.partial
					? gate.promise.then(() => reply)
					: call(cmd, args);
			return gate;
		},
		setDraft: (value: string, start = value.length, end = start) => (draft = { value, start, end }),
		draft: () => draft,
		puts: () => calls.filter((c) => c.cmd === "crewmate_voice_put"),
		transcribes: () => calls.filter((c) => c.cmd === "crewmate_transcribe"),
	};
}

const settle = async () => {
	for (let i = 0; i < 40; i++) await Promise.resolve();
};

/** Press, let the microphone open, hold past the latch window, release: the whole hold-to-talk gesture. */
async function holdAndSay(r, ms = 5000) {
	r.d.press(r.target);
	await settle();
	r.d.letGo(); // too early: held 0 ms, so it latches
	r.d.press(r.target); // the next press stops (a latched dictation ends on a second press)
	await settle();
}

describe("a dictation session: states and the draft", () => {
	test("hold past the latch window: it listens while held and the text goes into the draft on release", async () => {
		const r = rig();
		r.setDraft("fix the", 7);
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("listening");
		expect(r.d.engine).toBe("whisper.cpp (small)");
		await r.advance(LATCH_MS + 100);
		r.d.letGo();
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.draft().value).toBe("fix the open src slash main");
		expect(r.native.live).toBe(false);
	});

	test("a quick tap in hold mode latches: it keeps listening after the release and a second press stops it", async () => {
		const r = rig();
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		await settle();
		expect(r.d.phase).toBe("listening");
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.draft().value).toBe("open src slash main");
	});

	test("tap mode: one press starts, the release means nothing, the next press stops", async () => {
		const r = rig({ mode: "tap" });
		r.d.press(r.target);
		await settle();
		await r.advance(LATCH_MS + 100);
		r.d.letGo();
		await settle();
		expect(r.d.phase).toBe("listening");
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.draft().value).toBe("open src slash main");
	});

	test("a key released while the microphone is still opening still ends the dictation (held long) or latches it (a tap)", async () => {
		const r = rig();
		const opened = r.gateStart(); // the permission prompt is up
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("starting");
		await r.advance(LATCH_MS + 500);
		r.d.letGo(); // held through the prompt: let go means stop
		opened.resolve();
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.draft().value).toBe("open src slash main");

		const t = rig();
		const opened2 = t.gateStart();
		t.d.press(t.target);
		await settle();
		t.d.letGo(); // a tap during the prompt
		opened2.resolve();
		await settle();
		expect(t.d.phase).toBe("listening");
	});

	test("the dictated text goes over the selection, with the spaces fixed, and the caret lands after it", async () => {
		const r = rig();
		r.setDraft("fix the foo bug", 8, 11);
		await holdAndSay(r);
		expect(r.draft().value).toBe("fix the open src slash main bug");
		expect(r.writes.at(-1).cursor).toBe("fix the open src slash main".length);
	});

	test("what is typed during the transcription is kept: the text goes where the caret is NOW", async () => {
		const r = rig();
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		r.d.press(r.target);
		// The server is still working; the person types.
		r.setDraft("please ", 7);
		await settle();
		expect(r.draft().value).toBe("please open src slash main");
	});
});

describe("the live preview", () => {
	test("is refreshed at most every 2 s, shown apart from the draft, and replaced by the final text", async () => {
		const r = rig();
		r.setDraft("note: ", 6);
		r.d.press(r.target);
		await settle();
		expect(r.transcribes()).toHaveLength(0);
		await r.advance(PARTIAL_EVERY_MS - 1);
		expect(r.transcribes()).toHaveLength(0);
		await r.advance(1);
		expect(r.transcribes().map((c) => c.args.partial)).toEqual([true]);
		expect(r.d.partial).toBe("open src");
		// The preview never touches the draft.
		expect(r.draft().value).toBe("note: ");
		expect(r.writes).toHaveLength(0);
		// No new audio since: no new request (the engine is not asked about what it already heard).
		await r.advance(PARTIAL_EVERY_MS);
		expect(r.transcribes()).toHaveLength(1);
		// New audio: another one.
		r.native.file = audio(5000);
		await r.advance(PARTIAL_EVERY_MS);
		expect(r.transcribes()).toHaveLength(2);

		r.d.letGo();
		r.d.press(r.target);
		await settle();
		expect(r.d.partial).toBe("");
		expect(r.draft().value).toBe("note: open src slash main");
		const last = r.transcribes().at(-1);
		expect(last.args.partial).toBeUndefined();
	});

	test("the preview's requests keep the upload open, the final one closes it, one upload id throughout", async () => {
		const r = rig();
		r.d.press(r.target);
		await settle();
		await r.advance(PARTIAL_EVERY_MS);
		r.d.letGo();
		r.d.press(r.target);
		await settle();
		const puts = r.puts();
		expect(puts[0].args.upload).toBe("");
		expect(puts.slice(1).every((p) => p.args.upload === "u1")).toBe(true);
		expect(puts.slice(0, -1).every((p) => p.args.last === false)).toBe(true);
		expect(puts.at(-1).args.last).toBe(true);
		const t = r.transcribes();
		expect(t.every((c) => c.args.upload === "u1")).toBe(true);
	});
});

describe("the upload", () => {
	test("pieces are at most CHUNK bytes, in order, and rebuild the recording exactly; only the last closes the upload", async () => {
		const r = rig();
		r.native.file = audio(CHUNK * 2 + 777);
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		r.d.press(r.target);
		await settle();
		const puts = r.puts();
		const bytes = puts.map((p) => b64bytes(p.args.piece_b64));
		expect(bytes.every((b) => b.length <= CHUNK)).toBe(true);
		const joined = new Uint8Array(bytes.reduce((n, b) => n + b.length, 0));
		let at = 0;
		for (const b of bytes) (joined.set(b, at), (at += b.length));
		expect(joined).toEqual(r.native.file);
		expect(puts.map((p) => p.args.last)).toEqual([false, false, true]);
	});

	test("when the preview already sent everything, the close is an empty last piece, not a re-send", async () => {
		const r = rig();
		r.d.press(r.target);
		await settle();
		await r.advance(PARTIAL_EVERY_MS);
		const sentBefore = r.puts().length;
		r.d.letGo();
		r.d.press(r.target);
		await settle();
		const closing = r.puts().slice(sentBefore);
		expect(closing).toHaveLength(1);
		expect(closing[0].args.piece_b64).toBe("");
		expect(closing[0].args.last).toBe(true);
	});

	test("it never sends the task: the draft is the person's until they press Send", async () => {
		const r = rig();
		await holdAndSay(r);
		expect(r.draft().value).not.toBe("");
		expect(r.calls.map((c) => c.cmd).filter((c) => c.startsWith("crewmate_task"))).toEqual([]);
		expect(new Set(r.calls.map((c) => c.cmd))).toEqual(new Set(["crewmate_voice_put", "crewmate_transcribe"]));
	});
});

describe("Escape and the dictated flag", () => {
	test("restore puts back the draft exactly as it was, caret where the dictation started", async () => {
		const r = rig();
		r.setDraft("fix the foo bug", 8, 11);
		await holdAndSay(r);
		expect(r.d.restore()).toBe(true);
		expect(r.draft().value).toBe("fix the foo bug");
		expect(r.writes.at(-1).cursor).toBe(8);
		expect(r.d.restore()).toBe(false);
	});

	test("once the person types, Escape no longer takes anything back", async () => {
		const r = rig();
		await holdAndSay(r);
		r.setDraft(r.draft().value + "!");
		r.d.draftChanged(r.draft().value);
		expect(r.d.restore()).toBe(false);
		expect(r.draft().value).toBe("open src slash main!");
	});

	test("the draft is flagged dictated after an insertion, and a restore gives the old flag back", async () => {
		const r = rig();
		expect(r.d.dictated).toBe(false);
		await holdAndSay(r);
		expect(r.d.dictated).toBe(true);
		r.d.restore();
		expect(r.d.dictated).toBe(false);
	});

	test("typed text stays unflagged; flagged text stays flagged after a second dictation and an edit", async () => {
		const r = rig();
		r.setDraft("typed only");
		r.d.draftChanged("typed only");
		expect(r.d.dictated).toBe(false);
		await holdAndSay(r);
		r.d.draftChanged(r.draft().value + " more typing");
		expect(r.d.dictated).toBe(true);
	});

	test("emptying the draft (a send, or deleting it all) ends the flag", async () => {
		const r = rig();
		await holdAndSay(r);
		expect(r.d.dictated).toBe(true);
		r.d.draftChanged("");
		expect(r.d.dictated).toBe(false);
	});

	test("Escape while listening cancels: the microphone is released, nothing is uploaded as final, nothing inserted", async () => {
		const r = rig();
		r.setDraft("keep");
		r.d.press(r.target);
		await settle();
		r.d.cancel();
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.native.cancelled).toBe(1);
		expect(r.draft().value).toBe("keep");
		expect(r.d.dictated).toBe(false);
		expect(r.transcribes()).toHaveLength(0);
	});

	test("Escape while transcribing drops the answer when it arrives", async () => {
		const r = rig();
		const answer = r.gateFinal({ text: "late words", engine: { label: "x" } });
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("transcribing");
		r.d.cancel();
		answer.resolve();
		await settle();
		expect(r.draft().value).toBe("");
		expect(r.d.phase).toBe("idle");
	});
});

describe("the microphone is released", () => {
	test("when the composer goes away mid-dictation: released, audio dropped, no text arrives afterwards", async () => {
		const r = rig();
		r.d.press(r.target);
		await settle();
		r.d.dispose();
		await settle();
		expect(r.native.cancelled).toBe(1);
		expect(r.native.live).toBe(false);
		await r.advance(PARTIAL_EVERY_MS * 3);
		expect(r.transcribes()).toHaveLength(0);
		expect(r.draft().value).toBe("");
	});

	test("when it goes away while the microphone is still opening: it is handed back the moment it opens", async () => {
		const r = rig();
		const opened = r.gateStart();
		r.d.press(r.target);
		await settle();
		r.d.dispose();
		opened.resolve();
		await settle();
		expect(r.native.cancelled).toBeGreaterThanOrEqual(1);
		expect(r.d.phase).toBe("idle");
	});

	test("when the window goes to the background: the shell stops it, what was said is transcribed and kept", async () => {
		const r = rig();
		await r.d.attach();
		r.setDraft("so ");
		r.d.press(r.target);
		await settle();
		r.native.live = false; // the shell released the device
		r.native.stoppedCb({ reason: "background", bytes: 2000, seconds: 3 });
		await settle();
		expect(r.draft().value).toBe("so open src slash main");
		expect(r.d.phase).toBe("idle");
	});

	test("at the 300 s cap the shell ends it: everything recorded is transcribed, none discarded", async () => {
		const r = rig();
		await r.d.attach();
		const answer = r.gateFinal({ text: "open src slash main", engine: { label: "x" } });
		r.native.file = audio(CHUNK + 5);
		r.d.press(r.target);
		await settle();
		r.native.live = false;
		r.native.stoppedCb({ reason: "cap", bytes: CHUNK + 5, seconds: 300 });
		await settle();
		// What a screen reader hears the moment the cap stopped it, before the text arrives.
		expect(r.d.announce).toContain("5 minute");
		answer.resolve();
		await settle();
		const joined = r.puts().reduce((n, p) => n + b64bytes(p.args.piece_b64).length, 0);
		expect(joined).toBe(CHUNK + 5);
		expect(r.puts().at(-1).args.last).toBe(true);
		expect(r.draft().value).toBe("open src slash main");
		expect(r.d.phase).toBe("idle");
	});

	test("the stop the page asked for is not handled twice when the shell's event for it arrives too", async () => {
		const r = rig();
		await r.d.attach();
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		r.d.press(r.target);
		r.native.stoppedCb({ reason: "requested", bytes: 2000, seconds: 3 });
		await settle();
		expect(r.transcribes().filter((c) => !c.args.partial)).toHaveLength(1);
	});

	test("the levels move the meter and count the time only while listening", async () => {
		const r = rig();
		await r.d.attach();
		r.native.levelCb(0.9);
		expect(r.d.level).toBe(0);
		r.d.press(r.target);
		await settle();
		r.native.levelCb(0.5);
		r.native.levelCb(0.7);
		expect(r.d.level).toBe(0.7);
		expect(r.d.elapsedMs).toBe(100);
	});
});

describe("refusals and failures: the engine is named, nothing falls back", () => {
	test("crew switch off: the coded refusal is shown and the microphone never opens", async () => {
		const r = rig({ status: { ...STATUS, enabled: false } });
		r.d.press(r.target);
		await settle();
		expect(r.d.error).toEqual({ text: "coded:crew.voice.off", action: "none" });
		expect(r.native.started).toBe(0);
	});

	test("no engine installed: the coded refusal, microphone never opened", async () => {
		const r = rig({ status: { ...STATUS, engine: null } });
		r.d.press(r.target);
		await settle();
		expect(r.d.error.text).toBe("coded:crew.voice.not_installed");
		expect(r.native.started).toBe(0);
	});

	test("an engine that is not ready is refused with its label, never replaced by another", async () => {
		const status = { ...STATUS, engine: { ...STATUS.engine, ready: false, detail: "model missing" } };
		const r = rig({ status });
		let args;
		r.d.deps.coded = async (code, a) => ((args = a), code);
		r.d.press(r.target);
		await settle();
		expect(r.d.error.text).toBe("crew.voice.engine_failed");
		expect(args).toEqual({ engine: "whisper.cpp (small)", detail: "model missing" });
		expect(r.native.started).toBe(0);
	});

	test("a refused microphone is explained with a way to the system settings", async () => {
		const r = rig();
		r.native.startError = { code: "denied" };
		r.d.press(r.target);
		await settle();
		expect(r.d.phase).toBe("error");
		expect(r.d.error.action).toBe("settings");
		r.d.openSettings();
		await settle();
		expect(r.native.opened).toEqual(["settings"]);
	});

	test("no microphone, a busy recorder and a failed device each have their own message", async () => {
		const texts = new Set();
		for (const startError of [{ code: "no_device" }, { code: "busy" }, { code: "unavailable" }, { code: "failed", detail: "stream: x" }]) {
			const r = rig();
			r.native.startError = startError;
			r.d.press(r.target);
			await settle();
			expect(r.d.phase).toBe("error");
			texts.add(r.d.error.text);
		}
		expect(texts.size).toBe(4);
	});

	test("the engine failing on the final text is an error with Try again; the audio is still there to retry with", async () => {
		const r = rig();
		r.server.failFinal = new Error("engine failed");
		await holdAndSay(r);
		expect(r.d.phase).toBe("error");
		expect(r.d.error).toEqual({ text: "engine failed", action: "retry" });
		expect(r.draft().value).toBe("");

		r.server.failFinal = null;
		const before = r.puts().length;
		r.d.retry();
		await settle();
		expect(r.d.phase).toBe("idle");
		expect(r.draft().value).toBe("open src slash main");
		// A new upload of the whole recording (the failed one was deleted by the server).
		const again = r.puts().slice(before);
		expect(again[0].args.upload).toBe("");
		expect(b64bytes(again[0].args.piece_b64)).toEqual(r.native.file);
		expect(again.at(-1).args.last).toBe(true);
	});

	test("the engine failing on the live preview ends the recording now: microphone released, error shown, audio kept", async () => {
		const r = rig();
		r.server.failPartial = new Error("engine failed");
		r.d.press(r.target);
		await settle();
		await r.advance(PARTIAL_EVERY_MS);
		expect(r.d.phase).toBe("error");
		expect(r.native.live).toBe(false);
		expect(r.d.error.action).toBe("retry");
		await r.advance(PARTIAL_EVERY_MS * 2);
		expect(r.transcribes()).toHaveLength(1);
	});

	test("a blank transcript says so and leaves the draft alone", async () => {
		const r = rig();
		r.server.finalText = "  ";
		r.setDraft("keep me");
		await holdAndSay(r);
		expect(r.draft().value).toBe("keep me");
		expect(r.d.error.text).toContain("No speech");
		expect(r.d.dictated).toBe(false);
	});

	test("a press while it is transcribing is ignored, not a second microphone", async () => {
		const r = rig();
		const answer = r.gateFinal({ text: "x y", engine: { label: "x" } });
		r.d.press(r.target);
		await settle();
		r.d.letGo();
		r.d.press(r.target);
		await settle();
		r.d.press(r.target);
		await settle();
		expect(r.native.started).toBe(1);
		answer.resolve();
		await settle();
		expect(r.draft().value).toBe("x y");
	});
});
