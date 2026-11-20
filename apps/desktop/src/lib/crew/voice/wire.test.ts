// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { beforeAll, describe, expect, test } from "bun:test";
import fixture from "../../__fixtures__/crew-voice.json" with { type: "json" };
import { hostRejection, hostText, loadHostMessages } from "#lib/hostText.ts";
import { render } from "#lib/hostMessages.ts";
import { errText } from "#lib/api.ts";

// The fixture is what hostd's serializer wrote for the dictation commands (`UPDATE_FIXTURES=1 cargo test -p
// atlas-host voice`), never a hand-written copy. These tests run it through the desktop's readers: the status and the
// transcript, the sentence of every coded refusal, and a whole session whose server answers are that JSON.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { VOICE_OFF, parseTranscript, parseVoiceStatus } = await import("./wire.ts");
const { voiceStatusText } = await import("./statusText.ts");
const { Dictation, PARTIAL_EVERY_MS } = await import("./dictation.svelte.ts");

beforeAll(loadHostMessages);

describe("crew_voice_status", () => {
	test("a server with its engine ready reads as on, with the label the apps show verbatim", () => {
		const s = parseVoiceStatus(fixture.status_on);
		expect(s.enabled).toBe(true);
		expect(s.engine).toEqual({
			kind: "whisper_cpp",
			label: fixture.status_on.engine.label,
			model: "small",
			ready: true,
			detail: null,
		});
		expect(s.maxSeconds).toBe(fixture.status_on.max_seconds);
		expect(voiceStatusText(s)).toBe(`${fixture.status_on.engine.label}, ready`);
	});

	test("the crew-wide switch off, and no engine installed, are told apart", () => {
		expect(voiceStatusText(parseVoiceStatus(fixture.status_off))).toBe("Off");
		expect(parseVoiceStatus(fixture.status_off).enabled).toBe(false);
		const none = parseVoiceStatus(fixture.status_enabled_no_engine);
		expect(none.enabled).toBe(true);
		expect(none.engine).toBeNull();
		expect(voiceStatusText(none)).toBe("No speech engine installed");
	});

	test("an engine that is installed but cannot run says not ready, not on", () => {
		const raw = {
			...fixture.status_on,
			engine: { ...fixture.status_on.engine, ready: false, detail: "model missing" },
		};
		expect(voiceStatusText(parseVoiceStatus(raw))).toBe(`${fixture.status_on.engine.label}, not ready`);
	});

	test("a field an older hostd does not send reads as voice off; an unknown field is ignored", () => {
		expect(parseVoiceStatus({})).toEqual(VOICE_OFF);
		expect(parseVoiceStatus(null)).toEqual(VOICE_OFF);
		expect(parseVoiceStatus("nope")).toEqual(VOICE_OFF);
		const s = parseVoiceStatus({ ...fixture.status_on, a_future_field: 1 });
		expect(s.enabled).toBe(true);
		expect(parseVoiceStatus({ enabled: true }).engine).toBeNull();
	});

	test("the line for a server that never answered, and for one that cannot", () => {
		expect(voiceStatusText(undefined)).toBe("Checking…");
		expect(voiceStatusText(null)).toBe("Needs a newer crew server");
	});
});

describe("crewmate_transcribe", () => {
	test("the text that goes into the composer is the one after the replacements, with the engine that ran", () => {
		const t = parseTranscript(fixture.transcript);
		expect(t.text).toBe(fixture.transcript.text);
		expect(t.raw).toBe(fixture.transcript.raw);
		expect(t.text).not.toBe(t.raw);
		expect(t.engineLabel).toBe(fixture.transcript.engine.label);
		expect(t.partial).toBe(false);
		expect(parseTranscript(fixture.transcript_partial).partial).toBe(true);
	});

	test("a reply that is not a transcript reads as no text", () => {
		expect(parseTranscript(null).text).toBe("");
		expect(parseTranscript({}).engineLabel).toBe("");
	});
});

describe("coded refusals (crew.voice.*)", () => {
	const errors = Object.entries(fixture.errors);

	test.each(errors)(
		"%s: the app has a sentence of its own for the code, built from the args hostd sent",
		(_name, e) => {
			const text = render(e.code, e.args);
			expect(text).toBeDefined();
			expect(text).not.toBe(e.code);
			expect(hostText(e)).toBe(text);
			// The args that carry something to say are in the sentence.
			if (e.args.engine) expect(text).toContain(e.args.engine);
			if (e.args.max_seconds) expect(text).toContain(String(e.args.max_seconds));
		},
	);

	test.each(errors)(
		"%s: a rejection of hostd reaches the dictation error as that sentence",
		async (_name, e) => {
			const rejection = await hostRejection({ code: e.code, args: e.args, message: e.message });
			expect(errText(rejection)).toBe(render(e.code, e.args));
		},
	);

	test("every code of the contract has a sentence (nothing falls back to a raw code)", () => {
		expect(errors.map(([, e]) => e.code).sort()).toEqual(
			[
				"crew.voice.bad_audio",
				"crew.voice.engine_failed",
				"crew.voice.forbidden",
				"crew.voice.not_installed",
				"crew.voice.off",
				"crew.voice.too_long",
			].sort(),
		);
	});
});

describe("a session against the server's real replies", () => {
	function session() {
		const file = Uint8Array.from({ length: 1000 }, (_, i) => i % 251);
		const native = {
			live: false,
			async start() {
				native.live = true;
			},
			async stop() {
				native.live = false;
				return { reason: "requested", bytes: file.length, seconds: 2 };
			},
			async cancel() {},
			async read(offset, max) {
				return { data: file.slice(offset, offset + max), total: file.length, live: native.live };
			},
			async openSettings() {},
			async onLevel() {
				return () => {};
			},
			async onStopped() {
				return () => {};
			},
		};
		const calls = [];
		const timers = [];
		let clock = 0;
		const d = new Dictation({
			native,
			status: () => parseVoiceStatus(fixture.status_on),
			coded: (code, args) => hostText({ code, args, message: code }),
			errorText: errText,
			mode: () => "hold",
			now: () => clock,
			later(fn, ms) {
				const t = { at: clock + ms, fn, live: true };
				timers.push(t);
				return () => (t.live = false);
			},
			async call(cmd, args) {
				calls.push({ cmd, args });
				if (cmd === "crewmate_voice_put") return args.last ? fixture.put_last : fixture.put_open;
				if (args.partial) return fixture.transcript_partial;
				return fixture.transcript;
			},
		});
		let draft = { value: "", start: 0, end: 0 };
		const target = {
			read: () => draft,
			write: (value, cursor) => (draft = { value, start: cursor, end: cursor }),
		};
		const settle = async () => {
			for (let i = 0; i < 40; i++) await Promise.resolve();
		};
		const advance = async (ms) => {
			clock += ms;
			for (const t of timers.filter((t) => t.live && t.at <= clock)) ((t.live = false), t.fn());
			await settle();
		};
		return { d, target, calls, draft: () => draft, settle, advance };
	}

	test("the dimmed preview is the partial transcript and the draft gets the final text, never the raw words", async () => {
		const s = session();
		s.d.press(s.target);
		await s.settle();
		await s.advance(PARTIAL_EVERY_MS);
		expect(s.d.partial).toBe(fixture.transcript_partial.text);
		expect(s.draft().value).toBe("");
		s.d.letGo();
		s.d.press(s.target);
		await s.settle();
		expect(s.draft().value).toBe(fixture.transcript.text);
		expect(s.d.engine).toBe(fixture.transcript.engine.label);
	});

	test("the upload id is the one hostd issued: the first piece asks for one, every later call names it", async () => {
		const s = session();
		s.d.press(s.target);
		await s.settle();
		await s.advance(PARTIAL_EVERY_MS);
		s.d.letGo();
		s.d.press(s.target);
		await s.settle();
		const puts = s.calls.filter((c) => c.cmd === "crewmate_voice_put");
		expect(puts[0].args.upload).toBe("");
		for (const p of puts.slice(1)) expect(p.args.upload).toBe(fixture.put_open.upload);
		for (const c of s.calls.filter((c) => c.cmd === "crewmate_transcribe")) {
			expect(c.args.upload).toBe(fixture.put_open.upload);
		}
	});

	test("hostd's refusals during a session show their coded sentence and keep the audio for a retry", async () => {
		for (const [, e] of Object.entries(fixture.errors)) {
			const s = session();
			const call = s.d.deps.call;
			const rejection = await hostRejection({ code: e.code, args: e.args, message: e.message });
			s.d.deps.call = async (cmd, args) => {
				if (cmd === "crewmate_transcribe" && !args.partial) throw rejection;
				return call(cmd, args);
			};
			s.d.press(s.target);
			await s.settle();
			s.d.letGo();
			s.d.press(s.target);
			await s.settle();
			expect(s.d.phase).toBe("error");
			// Only an engine that failed is worth another try with the same audio; the rest need a change first.
			const action = e.code === "crew.voice.engine_failed" ? "retry" : "none";
			expect(s.d.error).toEqual({ text: render(e.code, e.args), action });
			expect(s.draft().value).toBe("");
		}
	});
});
