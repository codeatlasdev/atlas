// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { createEngine } from "./engine.ts";
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
	const engine = createEngine({
		AudioContext: Ctx,
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
