import { describe, expect, test } from "bun:test";
import { CATEGORIES } from "../../../apps/desktop/src/lib/sound/policy.ts";
import { EARCONS, FAMILY, capFade, decayDb, isCurrentTake, passes, pickBest, promptFor, takeName, type Stats } from "./spec.ts";

const ok = (over: Partial<Stats> = {}): Stats => ({
	duration: 0.3,
	peakDb: -3.5,
	rmsDb: -22,
	centroidHz: 900,
	clipped: false,
	silentHead: 0.004,
	...over,
});

describe("earcon spec", () => {
	test("every category the app knows has a spec, and no spec is orphaned", () => {
		expect(Object.keys(EARCONS).sort()).toEqual([...CATEGORIES].sort());
	});

	test("prompts share one timbre family so the set reads as one brand", () => {
		for (const [id, e] of Object.entries(EARCONS)) {
			expect(promptFor(e)).toContain(FAMILY);
			expect(promptFor(e), id).toContain("no reverb tail");
		}
	});

	test("feedback earcons are capped at 300 ms and signatures at 3 s", () => {
		for (const [id, e] of Object.entries(EARCONS)) {
			expect(e.maxSeconds, id).toBeLessThanOrEqual(e.kind === "signature" ? 3 : 0.3);
			expect(e.maxSeconds).toBeGreaterThan(0.05);
		}
	});

	test("the boot signature is the one non-feedback sound, still within the 3 s signature cap", () => {
		const signatures = Object.entries(EARCONS).filter(([, e]) => e.kind === "signature").map(([id]) => id);
		expect(signatures).toEqual(["boot"]);
	});

	test("the ElevenLabs API minimum duration (0.5 s) is respected when requesting, trimming happens after", () => {
		for (const e of Object.values(EARCONS)) expect(e.requestSeconds).toBeGreaterThanOrEqual(0.5);
	});
});

describe("passes: a candidate must meet the spec before it is considered", () => {
	const e = EARCONS["service-failed"];
	test("accepts a clean candidate", () => expect(passes(e, ok({ duration: 0.28 })).ok).toBe(true));
	// Incident: the first pick rejected 14 of 40 real takes as "clipping" because the raw mp3 overshoots 0 dBFS; the
	// take is re-scaled afterwards, so only a flat-topped (hard-clipped) take is a real defect.
	test("an mp3 that merely overshoots full scale is fine: it is normalised afterwards", () =>
		expect(passes(e, ok({ peakDb: 2.9, clipped: false })).ok).toBe(true));
	test("a flat-topped take (real clipping distortion) is rejected", () =>
		expect(passes(e, ok({ clipped: true }))).toMatchObject({ ok: false, why: "clipping" }));
	test("rejects too long", () => expect(passes(e, ok({ duration: e.maxSeconds + 0.2 }))).toMatchObject({ ok: false, why: "too-long" }));
	test("rejects too short to be heard", () => expect(passes(e, ok({ duration: 0.04 }))).toMatchObject({ ok: false, why: "too-short" }));
	test("rejects harsh sounds: centroid above the ceiling", () =>
		expect(passes(e, ok({ centroidHz: 4200 }))).toMatchObject({ ok: false, why: "too-bright" }));
	test("rejects dull sounds: centroid below the floor", () =>
		expect(passes(e, ok({ centroidHz: 250 }))).toMatchObject({ ok: false, why: "too-dull" }));
	// Incident: the floor was 800 Hz, but a mallet tone's energy-weighted centroid is 500-800 Hz (fundamental-heavy),
	// so every on-brand take was rejected as "too-dull".
	test("a fundamental-heavy mallet tone around 600 Hz is on brand, not dull", () =>
		expect(passes(e, ok({ centroidHz: 600 })).ok).toBe(true));
	test("rejects a late attack (leading silence would feel laggy)", () =>
		expect(passes(e, ok({ silentHead: 0.09 }))).toMatchObject({ ok: false, why: "late-attack" }));
	test("targets sit inside the accepted brightness window for every earcon", () => {
		for (const [id, x] of Object.entries(EARCONS)) expect(passes(x, ok({ centroidHz: x.targetCentroidHz, duration: Math.max(x.maxSeconds / 2, ("minSeconds" in x ? x.minSeconds : 0) + 0.1) })).ok, id).toBe(true);
	});
});

describe("pickBest", () => {
	const e = EARCONS["stack-up"];
	test("takes the candidate closest to the target brightness among those that pass", () => {
		const a = { id: "a", stats: ok({ centroidHz: e.targetCentroidHz + 900 }) };
		const b = { id: "b", stats: ok({ centroidHz: e.targetCentroidHz + 50 }) };
		const c = { id: "c", stats: ok({ centroidHz: e.targetCentroidHz, clipped: true }) };
		expect(pickBest(e, [a, b, c])?.id).toBe("b");
	});
	test("returns undefined when nothing passes (the script reports it instead of shipping a bad sound)", () => {
		expect(pickBest(e, [{ id: "x", stats: ok({ clipped: true }) }])).toBeUndefined();
		expect(pickBest(e, [])).toBeUndefined();
	});
	test("ties break by id so the choice is deterministic", () => {
		const s = ok();
		expect(pickBest(e, [{ id: "b", stats: s }, { id: "a", stats: s }])?.id).toBe("a");
	});
});

describe("a take longer than its cap may be faded out only where the note has already died away", () => {
	// Incident: 3 of 10 categories had no passing take because mallet notes ring past the 120-300 ms budget. The cap is
	// a product rule (UI sounds stay short), so the picker caps with a fade, but never chops a note that is still loud.
	const e = EARCONS["agent-done"];
	test("long take, quiet at the cap: accepted (it will be faded)", () =>
		expect(passes(e, ok({ duration: 0.6, levelAtCapDb: -22 })).ok).toBe(true));
	test("long take, still loud at the cap: rejected as cut-off", () =>
		expect(passes(e, ok({ duration: 0.6, levelAtCapDb: -8 }))).toMatchObject({ ok: false, why: "cut-off" }));
	test("a take that already fits needs no level at the cap", () =>
		expect(passes(e, ok({ duration: 0.2 })).ok).toBe(true));
	test("a long take with no level measured is rejected, not assumed fine", () =>
		expect(passes(e, ok({ duration: 0.6 }))).toMatchObject({ ok: false, why: "too-long" }));
	test("even a quiet-at-cap take may not be absurdly long (the model padded it)", () =>
		expect(passes(e, ok({ duration: 1.4, levelAtCapDb: -30 }))).toMatchObject({ ok: false, why: "too-long" }));
});

describe("takes are only valid for the prompt that made them", () => {
	// Incident: editing a gesture changed the cache key but left the old files in .cache, and `pick` measured both,
	// so a take from a superseded prompt could ship (the toggle report listed takes 0-3 twice).
	test("a file name carries the key of the prompt it was generated from", () => {
		const a = takeName("toggle", 0);
		expect(a).toMatch(/^toggle\.0\.[0-9a-f]{16}\.mp3$/);
		expect(takeName("toggle", 1)).not.toBe(a);
		expect(takeName("toggle", 0)).toBe(a);
	});
	test("only files of the current prompt are current", () => {
		const cur = takeName("toggle", 2);
		const stale = "toggle.2.0123456789abcdef.mp3";
		expect(isCurrentTake("toggle", cur)).toBe(true);
		expect(isCurrentTake("toggle", stale)).toBe(false);
		expect(isCurrentTake("toggle", takeName("success", 2))).toBe(false);
	});
});

describe("capFade", () => {
	// A 30 ms fade on a 120 ms tick is a quarter of the sound; it must scale with the earcon.
	test("is the standard fade for long earcons and a quarter of the cap for tiny ones", () => {
		expect(capFade(0.3)).toBeCloseTo(0.03, 5);
		expect(capFade(0.9)).toBeCloseTo(0.03, 5);
		expect(capFade(0.12)).toBeCloseTo(0.03, 5);
		expect(capFade(0.08)).toBeCloseTo(0.02, 5);
	});
	test("never exceeds the sound it fades", () => {
		for (const x of [0.05, 0.1, 0.3]) expect(capFade(x)).toBeLessThan(x);
	});
});

describe("product caps follow the research (<= 300 ms feedback), and the picker can actually meet them", () => {
	test("the most frequent sound is the shortest feedback sound", () => {
		const fb = Object.entries(EARCONS).filter(([, e]) => e.kind === "feedback");
		const shortest = Math.min(...fb.map(([, e]) => e.maxSeconds));
		expect(Number(EARCONS.toggle.maxSeconds)).toBe(shortest);
	});
});

describe("signatures may build before they strike", () => {
	// A signature may be a swell then a strike; the 50 ms late-attack rule is for feedback ticks, not for it.
	test("a signature with a long lead-in is not rejected as a late attack", () => {
		expect(passes(EARCONS.boot, ok({ duration: 0.5, silentHead: 0.4 })).ok).toBe(true);
	});
	test("a feedback earcon with the same lead-in still is", () => {
		expect(passes(EARCONS.toggle, ok({ duration: 0.15, silentHead: 0.4 }))).toMatchObject({ ok: false, why: "late-attack" });
	});
});

describe("a sonic logo must be long enough to be one", () => {
	test("a signature shorter than its minimum is rejected, a feedback earcon has no minimum beyond audibility", () => {
		const sig = { ...EARCONS.boot, maxSeconds: 2.6, minSeconds: 1.5 };
		expect(passes(sig, ok({ duration: 1.0, levelAtCapDb: undefined }))).toMatchObject({ ok: false, why: "too-short" });
		expect(passes(sig, ok({ duration: 1.6 })).ok).toBe(true);
		expect(passes(EARCONS.toggle, ok({ duration: 0.1 })).ok).toBe(true);
	});
});

describe("decayDb: how far a note must fall before it is over", () => {
	test("a signature's tail rings longer (40 dB) than a feedback tick's (30 dB)", () => {
		expect(decayDb(EARCONS.boot)).toBe(40);
		expect(decayDb(EARCONS.toggle)).toBe(30);
	});
});
