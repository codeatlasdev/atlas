// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	CATEGORIES,
	DEFAULT_SOUND_PREFS,
	decide,
	gain,
	newMemory,
	parseSoundPrefs,
	record,
	seedMemory,
	type Ctx,
} from "./policy.ts";

const base: Ctx = {
	now: 1_000_000,
	foreground: true,
	systemUiSounds: true,
	prefs: DEFAULT_SOUND_PREFS,
	reducedMotion: false,
};
const play = (cat, over = {}, mem = newMemory()) => decide(cat, { ...base, ...over }, mem);

describe("parseSoundPrefs", () => {
	test("missing or garbage settings give the defaults", () => {
		expect(parseSoundPrefs(undefined)).toEqual(DEFAULT_SOUND_PREFS);
		expect(parseSoundPrefs("loud")).toEqual(DEFAULT_SOUND_PREFS);
		expect(parseSoundPrefs([])).toEqual(DEFAULT_SOUND_PREFS);
	});

	test("volume is clamped to 0..1 and a non-number falls back", () => {
		expect(parseSoundPrefs({ volume: 7 }).volume).toBe(1);
		expect(parseSoundPrefs({ volume: -1 }).volume).toBe(0);
		expect(parseSoundPrefs({ volume: "max" }).volume).toBe(DEFAULT_SOUND_PREFS.volume);
		expect(parseSoundPrefs({ volume: Number.NaN }).volume).toBe(DEFAULT_SOUND_PREFS.volume);
	});

	test("per-category switches keep only known categories with boolean values", () => {
		const p = parseSoundPrefs({ categories: { boot: false, nope: false, update: "no" } });
		expect(p.categories.boot).toBe(false);
		expect(p.categories.update).toBe(true);
		expect("nope" in p.categories).toBe(false);
		expect(Object.keys(p.categories).sort()).toEqual([...CATEGORIES].sort());
	});

	test("`enabled` and `reduce` need real booleans", () => {
		expect(parseSoundPrefs({ enabled: "off" }).enabled).toBe(true);
		expect(parseSoundPrefs({ enabled: false }).enabled).toBe(false);
		expect(parseSoundPrefs({ reduce: 1 }).reduce).toBe(false);
	});
});

describe("preference precedence", () => {
	test("master off silences everything, including attention sounds", () => {
		const prefs = { ...DEFAULT_SOUND_PREFS, enabled: false };
		for (const c of CATEGORIES) expect(play(c, { prefs }).play).toBe(false);
	});

	test("macOS 'Play user interface sound effects' off silences everything the user did not ask for", () => {
		for (const c of CATEGORIES) {
			const d = play(c, { systemUiSounds: false });
			expect(d.play).toBe(false);
			expect(d.why).toBe("system-ui-sounds-off");
		}
	});

	test("a user-initiated preview ignores the system switch and cooldowns but not volume 0", () => {
		const mem = newMemory();
		record("stack-up", base.now, mem);
		expect(decide("stack-up", { ...base, systemUiSounds: false }, mem, { preview: true }).play).toBe(true);
		const mute = { ...DEFAULT_SOUND_PREFS, volume: 0 };
		expect(decide("stack-up", { ...base, prefs: mute }, mem, { preview: true }).play).toBe(false);
	});

	test("a category switched off stays silent while the others play", () => {
		const prefs = parseSoundPrefs({ categories: { "stack-up": false } });
		expect(play("stack-up", { prefs }).why).toBe("category-off");
		expect(play("update", { prefs }).play).toBe(true);
	});

	test("volume 0 plays nothing", () => {
		const prefs = { ...DEFAULT_SOUND_PREFS, volume: 0 };
		expect(play("update", { prefs }).why).toBe("muted");
	});
});

describe("background suppression", () => {
	test("in the background only the notification categories may play", () => {
		const bg = { foreground: false };
		for (const c of ["agent-needs-you", "agent-done", "service-failed", "update"])
			expect(play(c, bg).play).toBe(true);
		for (const c of ["boot", "stack-up", "discovery-done", "error", "success", "toggle"])
			expect(play(c, bg)).toMatchObject({ play: false, why: "background" });
	});
});

describe("cooldowns", () => {
	test("boot plays once, then not again for an hour, then again", () => {
		const mem = newMemory();
		expect(decide("boot", base, mem).play).toBe(true);
		record("boot", base.now, mem);
		expect(decide("boot", { ...base, now: base.now + 59 * 60_000 }, mem)).toMatchObject({
			play: false,
			why: "cooldown",
		});
		expect(decide("boot", { ...base, now: base.now + 60 * 60_000 }, mem).play).toBe(true);
	});

	test("a burst of the same category collapses into one sound", () => {
		const mem = newMemory();
		record("stack-up", base.now, mem);
		expect(decide("stack-up", { ...base, now: base.now + 400 }, mem).why).toBe("cooldown");
		expect(decide("stack-up", { ...base, now: base.now + 5_000 }, mem).play).toBe(true);
	});

	test("cooldowns are per category: a failure right after a success still sounds", () => {
		const mem = newMemory();
		record("stack-up", base.now, mem);
		expect(decide("service-failed", { ...base, now: base.now + 10 }, mem).play).toBe(true);
	});

	test("a suppressed sound does not start a cooldown (only played sounds are recorded)", () => {
		const mem = newMemory();
		const d = decide("boot", { ...base, systemUiSounds: false }, mem);
		expect(d.play).toBe(false);
		expect(mem.last.boot).toBeUndefined();
	});

	test("a clock that went backwards does not lock a category forever", () => {
		const mem = newMemory();
		record("boot", base.now, mem);
		expect(decide("boot", { ...base, now: base.now - 10 * 60 * 60_000 }, mem).play).toBe(true);
	});
});

describe("reduce sounds", () => {
	test("reduce keeps attention categories only", () => {
		const prefs = { ...DEFAULT_SOUND_PREFS, reduce: true };
		for (const c of ["agent-needs-you", "service-failed", "agent-done", "update"])
			expect(play(c, { prefs }).play).toBe(true);
		for (const c of ["boot", "stack-up", "discovery-done", "success", "toggle", "error"])
			expect(play(c, { prefs })).toMatchObject({ play: false, why: "reduced" });
	});

	test("reduce also lowers the level of the ones that still play", () => {
		const prefs = { ...DEFAULT_SOUND_PREFS, reduce: true };
		expect(gain("update", prefs)).toBeLessThan(gain("update", DEFAULT_SOUND_PREFS));
	});

	test("reduced motion alone never silences sound (separate accessibility axes)", () => {
		expect(play("stack-up", { reducedMotion: true }).play).toBe(true);
	});
});

describe("gain", () => {
	test("is volume times the category's mix level, never above 1", () => {
		for (const c of CATEGORIES) {
			const g = gain(c, { ...DEFAULT_SOUND_PREFS, volume: 1 });
			expect(g).toBeGreaterThan(0);
			expect(g).toBeLessThanOrEqual(1);
		}
		expect(gain("boot", { ...DEFAULT_SOUND_PREFS, volume: 0.5 })).toBeCloseTo(
			gain("boot", { ...DEFAULT_SOUND_PREFS, volume: 1 }) / 2,
			5,
		);
	});

	test("the default volume is low, not full scale", () => {
		expect(DEFAULT_SOUND_PREFS.volume).toBeLessThanOrEqual(0.5);
		expect(DEFAULT_SOUND_PREFS.volume).toBeGreaterThan(0);
	});
});

describe("boot cooldown survives restarts", () => {
	// The app is closed and reopened constantly; an in-memory cooldown resets on every launch, which would play the
	// signature on every restart. The last play time is persisted and seeded back into memory at start.
	test("seeding memory with the persisted time keeps the hour", () => {
		const mem = seedMemory({ boot: base.now - 10 * 60_000 });
		expect(decide("boot", base, mem)).toMatchObject({ play: false, why: "cooldown" });
	});
	test("an hour later it plays again", () => {
		const mem = seedMemory({ boot: base.now - 61 * 60_000 });
		expect(decide("boot", base, mem).play).toBe(true);
	});
	test("garbage persisted values are ignored", () => {
		const mem = seedMemory({ boot: "yesterday", nope: 5, stack: -1 });
		expect(mem.last).toEqual({});
	});
});
