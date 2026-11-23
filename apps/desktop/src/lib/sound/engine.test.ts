// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { createEngine, earconFormats } from "./engine.ts";
import { DEFAULT_SOUND_PREFS, parseSoundPrefs } from "./policy.ts";

/** A fake Web Audio graph that records what the engine builds, with the real control flow around it. */
function rig(over = {}) {
	const log = { contexts: 0, started: [], gains: [], fetched: [], resumed: 0 };
	class Ctx {
		state = "suspended";
		currentTime = 0;
		destination = {};
		constructor() {
			log.contexts++;
		}
		resume() {
			log.resumed++;
			this.state = "running";
			return Promise.resolve();
		}
		decodeAudioData(buf) {
			return Promise.resolve({ duration: 0.3, bytes: buf.byteLength });
		}
		createBufferSource() {
			const n = { buffer: null, connect: (x) => x, start: () => log.started.push(n.buffer), onended: null };
			return n;
		}
		createGain() {
			const g = { gain: { value: 1, setValueAtTime() {} }, connect: (x) => x };
			log.gains.push(g);
			return g;
		}
	}
	const state = {
		now: 1_000_000,
		foreground: true,
		systemUiSounds: true,
		prefs: DEFAULT_SOUND_PREFS,
		reducedMotion: false,
		...over,
	};
	delete state.output;
	const engine = createEngine({
		AudioContext: Ctx,
		output: over.output,
		fetchAsset: async (name) => {
			log.fetched.push(name);
			return new ArrayBuffer(16);
		},
		context: () => state,
	});
	return { engine, log, state };
}

describe("audio engine", () => {
	test("no AudioContext exists until the first user gesture (autoplay rule)", async () => {
		const { engine, log } = rig();
		expect(log.contexts).toBe(0);
		await engine.play("success");
		// Nothing was gestured yet: the sound is dropped, not queued, and no context was created.
		expect(log.contexts).toBe(0);
		engine.gesture();
		expect(log.contexts).toBe(1);
	});

	test("one context for the whole session, resumed on every gesture while suspended", async () => {
		const { engine, log } = rig();
		engine.gesture();
		engine.gesture();
		engine.gesture();
		expect(log.contexts).toBe(1);
		expect(log.resumed).toBeGreaterThanOrEqual(1);
	});

	// #87: on Linux without a usable audio device WebKitGTK rejects resume() with "Failed to start the audio
	// device". The engine fired it with `void`, so every click/key raised an unhandled rejection that the error
	// reporter showed to the user (×5 per second while editing). A device that failed to start is not retried.
	test("a resume() that rejects is swallowed and not retried on later gestures", async () => {
		const returned = [];
		const engine = createEngine({
			AudioContext: class {
				state = "suspended";
				destination = {};
				resume() {
					const p = Promise.reject(new Error("Failed to start the audio device"));
					const entry = { handled: false };
					const then = p.then.bind(p);
					p.then = (ok, fail) => {
						if (fail) entry.handled = true;
						return then(ok, fail);
					};
					returned.push(entry);
					p.catch(() => {}); // keep the test runner itself quiet; `handled` records the engine's own handler
					entry.handled = false;
					return p;
				}
			},
			fetchAsset: async () => new ArrayBuffer(16),
			context: () => ({ now: 0, foreground: true, systemUiSounds: true, prefs: DEFAULT_SOUND_PREFS }),
		});
		engine.gesture();
		await Promise.resolve();
		await Promise.resolve();
		engine.gesture();
		engine.gesture();
		expect(returned.map((r) => r.handled)).toEqual([true]);
	});

	test("plays a decoded buffer with the category's gain; an asset is fetched and decoded once", async () => {
		const { engine, log } = rig({ prefs: { ...DEFAULT_SOUND_PREFS, volume: 0.5 } });
		engine.gesture();
		await engine.play("update");
		await engine.play("update", { preview: true });
		expect(log.fetched).toEqual(["update"]);
		expect(log.started.length).toBe(2);
		expect(log.gains.at(-1).gain.value).toBeCloseTo(0.5 * 0.8, 5);
	});

	test("a suppressed sound neither fetches nor starts a node", async () => {
		const { engine, log } = rig({ systemUiSounds: false });
		engine.gesture();
		await engine.play("stack-up");
		expect(log.fetched).toEqual([]);
		expect(log.started).toEqual([]);
	});

	test("a played sound starts its cooldown; a second call right after is dropped", async () => {
		const { engine, log, state } = rig();
		engine.gesture();
		await engine.play("stack-up");
		state.now += 200;
		await engine.play("stack-up");
		expect(log.started.length).toBe(1);
		state.now += 10_000;
		await engine.play("stack-up");
		expect(log.started.length).toBe(2);
	});

	test("a missing or undecodable asset fails quietly and does not poison the cooldown", async () => {
		const { engine, log, state } = rig();
		engine.gesture();
		const bad = createEngine({
			AudioContext: class {
				state = "running";
				destination = {};
				resume() {}
				decodeAudioData() {
					return Promise.reject(new Error("bad data"));
				}
				createBufferSource() {
					throw new Error("must not be reached");
				}
				createGain() {
					return { gain: { value: 1 }, connect: (x) => x };
				}
			},
			fetchAsset: async () => new ArrayBuffer(4),
			context: () => state,
		});
		bad.gesture();
		await bad.play("update");
		expect(log.started).toEqual([]);
		// the same category still works on the healthy engine
		await engine.play("update");
		expect(log.started.length).toBe(1);
	});

	test("preview works even when the system switch is off, and plays at the chosen volume", async () => {
		const { engine, log } = rig({ systemUiSounds: false, prefs: parseSoundPrefs({ volume: 0.3 }) });
		engine.gesture();
		await engine.play("agent-done", { preview: true });
		expect(log.started.length).toBe(1);
	});

	test("without Web Audio at all the engine is a silent no-op", async () => {
		const e = createEngine({
			AudioContext: undefined,
			fetchAsset: async () => new ArrayBuffer(1),
			context: () => ({}),
		});
		e.gesture();
		await e.play("update");
	});
});

/**
 * Regression (#265): the earcons were AAC-in-.m4a only. WebKitGTK decodes through the host's (or the AppImage's)
 * GStreamer, and neither has an AAC decoder (`Missing decoder: MPEG-4 AAC`: gst-libav/faad is not in the AppImage's
 * Ubuntu plugin set nor in a bare Arch install), so every `decodeAudioData` failed and Linux never made a sound.
 * Every other format is decoded with plugins-base/good, so an Ogg copy ships and the engine falls back to it.
 */
describe("earcon formats", () => {
	/** A graph whose decoder only understands the formats in `decodes`: the bytes fetched say which format they are. */
	function formatRig(decodes, formats) {
		const log = { fetched: [], started: 0 };
		const engine = createEngine({
			AudioContext: class {
				state = "running";
				destination = {};
				resume() {}
				decodeAudioData(buf) {
					const format = new TextDecoder().decode(buf);
					return decodes.includes(format)
						? Promise.resolve({ format })
						: Promise.reject(new Error("EncodingError"));
				}
				createBufferSource() {
					return { buffer: null, connect: (x) => x, start: () => log.started++ };
				}
				createGain() {
					return { gain: { value: 1 }, connect: (x) => x };
				}
			},
			formats,
			fetchAsset: async (name, format) => {
				log.fetched.push(`${name}.${format}`);
				return new TextEncoder().encode(format).buffer;
			},
			context: () => ({ now: 1_000_000, foreground: true, systemUiSounds: true, prefs: DEFAULT_SOUND_PREFS }),
		});
		engine.gesture();
		return { engine, log };
	}

	test("a decoder without AAC plays the Ogg copy, and remembers which format worked", async () => {
		const { engine, log } = formatRig(["ogg"], ["m4a", "ogg"]);
		await engine.play("agent-done");
		expect(log.started).toBe(1);
		expect(log.fetched).toEqual(["agent-done.m4a", "agent-done.ogg"]);
		await engine.play("agent-done", { preview: true }); // (past the cooldown) the decoded buffer is cached: no second fetch of either file
		expect(log.started).toBe(2);
		expect(log.fetched.length).toBe(2);
	});

	test("the first format in the list that decodes wins, and later ones are never fetched", async () => {
		const { engine, log } = formatRig(["m4a", "ogg"], ["ogg", "m4a"]);
		await engine.play("toggle");
		expect(log.fetched).toEqual(["toggle.ogg"]);
		expect(log.started).toBe(1);
	});

	test("when no format decodes the earcon is a silent miss, retried (not cached) next time", async () => {
		const { engine, log } = formatRig([], ["m4a", "ogg"]);
		await engine.play("update");
		await engine.play("update");
		expect(log.started).toBe(0);
		expect(log.fetched).toEqual(["update.m4a", "update.ogg", "update.m4a", "update.ogg"]);
	});

	// canPlayType is WebKit's view of GStreamer's registry: the AppImage answered "" for audio/mp4 and "maybe" for
	// audio/ogg (probed 2026-11-15 with the bundled plugins), macOS answers "maybe"/"probably" for mp4 and "" for ogg.
	test("earconFormats puts AAC first only where the webview says it can play it", () => {
		const says = (table) => (mime) => table[mime] ?? "";
		expect(
			earconFormats(says({ 'audio/mp4; codecs="mp4a.40.2"': "probably", "audio/ogg; codecs=vorbis": "" })),
		).toEqual(["m4a", "ogg"]);
		expect(earconFormats(says({ "audio/ogg; codecs=vorbis": "maybe" }))).toEqual(["ogg", "m4a"]);
		expect(earconFormats(says({}))).toEqual(["m4a", "ogg"]);
	});
});

describe("persisted cooldown", () => {
	test("a boot played in a previous run blocks this run's boot, and a new play is written back", async () => {
		const saved = [];
		const mk = (load) => {
			const r = rig();
			const e = createEngine({
				AudioContext: class {
					state = "running";
					destination = {};
					resume() {}
					decodeAudioData = async () => ({});
					createBufferSource = () => ({
						buffer: null,
						connect: (x) => x,
						start: () => r.log.started.push(1),
					});
					createGain = () => ({ gain: { value: 1 }, connect: (x) => x });
				},
				fetchAsset: async () => new ArrayBuffer(4),
				context: () => r.state,
				persisted: { load, save: (l) => saved.push(l) },
			});
			e.gesture();
			return { e, r };
		};
		const recent = mk(() => ({ boot: 1_000_000 - 5 * 60_000 }));
		await recent.e.play("boot");
		expect(recent.r.log.started).toEqual([]);
		const fresh = mk(() => ({}));
		await fresh.e.play("boot");
		expect(fresh.r.log.started.length).toBe(1);
		expect(saved).toEqual([{ boot: 1_000_000 }]);
	});

	test("non-persisted categories are never written", async () => {
		const saved = [];
		const r = rig();
		const e = createEngine({
			AudioContext: class {
				state = "running";
				destination = {};
				resume() {}
				decodeAudioData = async () => ({});
				createBufferSource = () => ({ buffer: null, connect: (x) => x, start() {} });
				createGain = () => ({ gain: { value: 1 }, connect: (x) => x });
			},
			fetchAsset: async () => new ArrayBuffer(4),
			context: () => r.state,
			persisted: { load: () => ({}), save: (l) => saved.push(l) },
		});
		e.gesture();
		await e.play("stack-up");
		expect(saved).toEqual([]);
	});
});

describe("playOnGesture (the cold-start signature)", () => {
	// Incident-in-waiting: boot is requested at startup, before any gesture exists, and the engine rightly drops
	// sounds without a context. The signature must wait for the first gesture instead of being lost.
	test("is held until the first gesture, then plays exactly once", async () => {
		const { engine, log } = rig();
		engine.playOnGesture("boot");
		await Promise.resolve();
		expect(log.started).toEqual([]);
		engine.gesture();
		await new Promise((r) => setTimeout(r, 0));
		expect(log.started.length).toBe(1);
		engine.gesture();
		await new Promise((r) => setTimeout(r, 0));
		expect(log.started.length).toBe(1);
	});

	test("with a context already running it plays immediately", async () => {
		const { engine, log } = rig();
		engine.gesture();
		engine.playOnGesture("boot");
		await new Promise((r) => setTimeout(r, 0));
		expect(log.started.length).toBe(1);
	});

	test("a hold that the policy rejects when the gesture finally comes stays silent", async () => {
		const { engine, log, state } = rig();
		engine.playOnGesture("boot");
		state.systemUiSounds = false;
		engine.gesture();
		await new Promise((r) => setTimeout(r, 0));
		expect(log.started).toEqual([]);
	});

	test("only the latest held category plays (no stacked startup sounds)", async () => {
		const { engine, log } = rig();
		engine.playOnGesture("boot");
		engine.playOnGesture("update");
		engine.gesture();
		await new Promise((r) => setTimeout(r, 0));
		expect(log.started.length).toBe(1);
		expect(log.fetched).toEqual(["update"]);
	});
});

// Incident (LinuxFixes item 3): on a Linux with no GStreamer audio sink (`autoaudiosink` missing: gst-plugins-good not
// installed) WebKit's AudioContext.resume() logs `GLib-GObject-CRITICAL` twice from the web process, on the first click
// of every session. The context must not even be created when the shell says there is no output.
describe("no audio output on this machine", () => {
	test("a gesture creates no AudioContext and never resumes one, and sounds are silent misses", async () => {
		const { engine, log } = rig({ output: () => false });
		engine.gesture();
		engine.playOnGesture("boot");
		engine.gesture();
		await engine.play("success", { preview: true });
		expect(log.contexts).toBe(0);
		expect(log.resumed).toBe(0);
		expect(log.fetched).toEqual([]);
	});

	test("the answer is read at gesture time: an output that appears later works from then on", async () => {
		let output = false;
		const { engine, log } = rig({ output: () => output });
		engine.gesture();
		expect(log.contexts).toBe(0);
		output = true;
		engine.gesture();
		expect(log.contexts).toBe(1);
		expect(log.resumed).toBe(1);
	});

	test("an engine told nothing about the output behaves as before", () => {
		const { engine, log } = rig();
		engine.gesture();
		expect(log.contexts).toBe(1);
	});
});
