// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	clampTrim,
	countdownDigit,
	encodeFraction,
	formatEstimate,
	formatSecs,
	formatTimer,
	fractionOf,
	keyAction,
	moveHandle,
	saveArgs,
	secsAt,
	view,
} from "./record.ts";

const st = (phase, elapsedMs = 0, extra = {}) => ({
	phase,
	elapsedMs,
	maxMs: 600_000,
	kind: "recording",
	...extra,
});

describe("formatTimer", () => {
	test.each([
		[0, "0:00"],
		[999, "0:00"],
		[1000, "0:01"],
		[59_999, "0:59"],
		[60_000, "1:00"],
		[61_000, "1:01"],
		[599_000, "9:59"],
		[3_599_999, "59:59"],
		[3_600_000, "1:00:00"],
		[3_661_000, "1:01:01"],
		[99 * 60_000 + 59_000, "1:39:59"],
		[100 * 60_000, "1:40:00"],
		[10 * 3_600_000, "10:00:00"],
	])("%d ms -> %s", (ms, out) => expect(formatTimer(ms)).toBe(out));

	test("negative and non-finite input is 0:00", () => {
		expect(formatTimer(-5)).toBe("0:00");
		expect(formatTimer(Number.NaN)).toBe("0:00");
	});
});

describe("countdownDigit", () => {
	test("3,3,2,1,1 across 0..3500 ms", () => {
		expect([0, 500, 1000, 2000, 3000].map(countdownDigit)).toEqual([3, 3, 2, 1, 1]);
		expect(countdownDigit(3500)).toBe(1);
	});
	test("boundaries", () => {
		expect(countdownDigit(999)).toBe(3);
		expect(countdownDigit(1000)).toBe(2);
		expect(countdownDigit(1999)).toBe(2);
		expect(countdownDigit(2000)).toBe(1);
		expect(countdownDigit(2999)).toBe(1);
	});
	test("never 0, never above 3, whatever the input", () => {
		for (let ms = -500; ms <= 10_000; ms += 7) {
			const d = countdownDigit(ms);
			expect(d).toBeGreaterThanOrEqual(1);
			expect(d).toBeLessThanOrEqual(3);
		}
		expect(countdownDigit(Number.NaN)).toBe(3);
	});
});

describe("remaining", () => {
	test("hidden until the last 30 s", () => {
		expect(view(st("recording", 0)).remaining).toBeNull();
		expect(view(st("recording", 569_999)).remaining).toBeNull();
		expect(view(st("recording", 569_999)).tone).toBe("rec");
	});
	test("shown from exactly 30 s left, with warn tone", () => {
		const v = view(st("recording", 570_000));
		expect(v.remaining).toBe("0:30 left");
		expect(v.tone).toBe("warn");
	});
	test("0:23 left", () => {
		expect(view(st("recording", 577_000)).remaining).toBe("0:23 left");
	});
	test("rounds up so the last second reads 0:01, not 0:00", () => {
		expect(view(st("recording", 599_001)).remaining).toBe("0:01 left");
	});
	test("clamped at 0 past the limit", () => {
		expect(view(st("recording", 600_000)).remaining).toBe("0:00 left");
		expect(view(st("recording", 650_000)).remaining).toBe("0:00 left");
	});
	test("also while paused (the clock is frozen, the limit is not)", () => {
		const v = view(st("paused", 580_000));
		expect(v.remaining).toBe("0:20 left");
		expect(v.tone).toBe("warn");
	});
	test("never outside recording/paused", () => {
		for (const p of ["idle", "countdown", "saving", "done", "error"])
			expect(view(st(p, 590_000)).remaining).toBeNull();
	});
	test("no limit (maxMs 0) means no remaining", () => {
		expect(view(st("recording", 590_000, { maxMs: 0 })).remaining).toBeNull();
	});
	test("a short limit shows remaining from the start", () => {
		expect(view(st("recording", 0, { maxMs: 20_000 })).remaining).toBe("0:20 left");
	});
});

describe("labels, timer, tone, pulse, progress", () => {
	test("recording: timer, rec tone, pulses", () => {
		const v = view(st("recording", 65_000));
		expect(v.timer).toBe("1:05");
		expect(v.tone).toBe("rec");
		expect(v.pulse).toBe(true);
		expect(v.label).toBe("Recording");
		expect(v.progress).toBe(false);
	});
	test("gif kind is labelled GIF", () => {
		expect(view(st("recording", 0, { kind: "gif" })).label).toBe("Recording GIF");
	});
	test("paused: idle tone, Paused, frozen timer, never pulses", () => {
		const v = view(st("paused", 65_000));
		expect(v.label).toBe("Paused");
		expect(v.tone).toBe("idle");
		expect(v.pulse).toBe(false);
		expect(v.timer).toBe("1:05");
		expect(view(st("paused", 65_000)).timer).toBe(v.timer);
	});
	test("pulse only while recording", () => {
		for (const p of ["idle", "countdown", "paused", "saving", "done", "error"])
			expect(view(st(p)).pulse, p).toBe(false);
		expect(view(st("recording")).pulse).toBe(true);
	});
	test("pulse stays on in the warn window (still recording)", () => {
		expect(view(st("recording", 590_000)).pulse).toBe(true);
	});
	test("countdown shows the big digit", () => {
		expect(view(st("countdown", 0)).digit).toBe(3);
		expect(view(st("countdown", 1500)).digit).toBe(2);
		expect(view(st("countdown", 3400)).digit).toBe(1);
		expect(view(st("recording", 0)).digit).toBeNull();
	});
	test("saving is indeterminate", () => {
		const v = view(st("saving", 12_000));
		expect(v.progress).toBe(true);
		expect(v.label).toBe("Saving…");
		expect(v.tone).toBe("idle");
	});
	test("done and error carry the message", () => {
		const d = view(st("done", 1000, { message: "Saved to Desktop" }));
		expect(d.label).toBe("Saved to Desktop");
		expect(d.tone).toBe("ok");
		const e = view(st("error", 0, { message: "Disk full" }));
		expect(e.label).toBe("Disk full");
		expect(e.tone).toBe("error");
	});
	test("done and error fall back to a default sentence without a message", () => {
		expect(view(st("done")).label).toBe("Saved");
		expect(view(st("error")).label).toBe("Recording failed");
	});
});

// phase -> [showStop, showPause, showResume, showCancel, showOpen, showClose, disabled]
describe("button visibility matrix", () => {
	const rows = [
		["idle", [false, false, false, false, false, false, false]],
		["countdown", [false, false, false, true, false, false, false]],
		["recording", [true, true, false, true, false, false, false]],
		["paused", [true, false, true, true, false, false, false]],
		["saving", [false, false, false, false, false, false, true]],
		["done", [false, false, false, false, true, true, false]],
		["error", [false, false, false, false, false, true, false]],
	];
	test.each(rows)("%s", (phase, [stop, pause, resume, cancel, open, close, disabled]) => {
		const v = view(st(phase, 1000, phase === "done" ? { entry: { path: "/a.mov" } } : {}));
		expect({
			showStop: v.showStop,
			showPause: v.showPause,
			showResume: v.showResume,
			showCancel: v.showCancel,
			showOpen: v.showOpen,
			showClose: v.showClose,
			disabled: v.disabled,
		}).toEqual({
			showStop: stop,
			showPause: pause,
			showResume: resume,
			showCancel: cancel,
			showOpen: open,
			showClose: close,
			disabled,
		});
	});
	test("done without an entry cannot Open", () => {
		const v = view(st("done"));
		expect(v.showOpen).toBe(false);
		expect(v.showClose).toBe(true);
	});
	test("pause and resume are never both shown", () => {
		for (const p of ["idle", "countdown", "recording", "paused", "saving", "done", "error"]) {
			const v = view(st(p));
			expect(v.showPause && v.showResume, p).toBe(false);
		}
	});
});

describe("keyAction matrix", () => {
	// [key, phase, expected]
	const rows = [
		["Escape", "countdown", "cancel"],
		["Escape", "recording", "cancel"],
		["Escape", "paused", "cancel"],
		["Escape", "saving", null],
		["Escape", "idle", null],
		["Escape", "done", "close"],
		["Escape", "error", "close"],
		["Enter", "recording", "stop"],
		["Enter", "paused", "stop"],
		["Enter", "countdown", null],
		["Enter", "saving", null],
		["Enter", "idle", null],
		["Enter", "done", "open"],
		["Enter", "error", "close"],
		["p", "recording", "pause"],
		["P", "recording", "pause"],
		["p", "paused", "resume"],
		["p", "countdown", null],
		["p", "saving", null],
		["p", "done", null],
		[" ", "recording", null],
		["a", "recording", null],
	];
	test.each(rows)("%p in %s -> %p", (key, phase, out) => expect(keyAction(key, phase)).toBe(out));

	test("saving ignores every key", () => {
		for (const k of ["Escape", "Enter", "p", "P", " ", "x"]) expect(keyAction(k, "saving")).toBeNull();
	});
	test("modifier-less only: Cmd/Ctrl/Alt chords are not ours", () => {
		expect(keyAction("p", "recording", { meta: true })).toBeNull();
		expect(keyAction("Enter", "recording", { ctrl: true })).toBeNull();
		expect(keyAction("Escape", "recording", { alt: true })).toBeNull();
	});
});

describe("size estimate", () => {
	test.each([
		[0, "≈ 0 B"],
		[999, "≈ 999 B"],
		[1000, "≈ 1 KB"],
		[250_000, "≈ 250 KB"],
		[1_000_000, "≈ 1.0 MB"],
		[12_400_000, "≈ 12.4 MB"],
		[2_500_000_000, "≈ 2.50 GB"],
	])("%d bytes -> %s", (n, out) => expect(formatEstimate(n)).toBe(out));
	test("junk and missing are no estimate, never NaN on the HUD", () => {
		for (const v of [null, undefined, -1, Number.NaN, Number.POSITIVE_INFINITY])
			expect(formatEstimate(v)).toBeNull();
	});
	test("shown while recording, paused and saving an mp4; never for a GIF (Rust sends null) or before it starts", () => {
		expect(view(st("recording", 5000, { estimateBytes: 6_000_000 })).estimate).toBe("≈ 6.0 MB");
		expect(view(st("paused", 5000, { estimateBytes: 6_000_000 })).estimate).toBe("≈ 6.0 MB");
		expect(view(st("recording", 5000, { kind: "gif", estimateBytes: null })).estimate).toBeNull();
		expect(view(st("countdown", 0, { estimateBytes: 5 })).estimate).toBeNull();
		expect(view(st("done", 0, { estimateBytes: 5 })).estimate).toBeNull();
	});
});

describe("notes of skipped options", () => {
	test("are passed through, empty when absent", () => {
		expect(view(st("recording", 0, { notes: ["Microphone skipped: no access"] })).notes).toEqual([
			"Microphone skipped: no access",
		]);
		expect(view(st("recording", 0)).notes).toEqual([]);
	});
});

describe("GIF encode progress", () => {
	test("a saving GIF with frame counts is determinate, with a percentage label", () => {
		const v = view(st("saving", 0, { kind: "gif", progress: { done: 15, total: 60 } }));
		expect(v.encode).toBe(0.25);
		expect(v.progress).toBe(false);
		expect(v.label).toBe("Encoding GIF 25%");
	});
	test("without counts (or total 0) saving stays indeterminate", () => {
		for (const p of [undefined, null, { done: 0, total: 0 }]) {
			const v = view(st("saving", 0, { progress: p }));
			expect(v.encode).toBeNull();
			expect(v.progress).toBe(true);
			expect(v.label).toBe("Saving…");
		}
	});
	test("the fraction is clamped to 0..1", () => {
		expect(encodeFraction({ done: 9, total: 3 })).toBe(1);
		expect(encodeFraction({ done: -2, total: 3 })).toBe(0);
		expect(encodeFraction(null)).toBe(0);
	});
	/** Esc must stop the encode, but only while one runs: a plain save has nothing to cancel. */
	test("Esc cancels only while encoding; every other key is still ignored", () => {
		expect(keyAction("Escape", "saving", {}, { encoding: true })).toBe("cancel");
		expect(keyAction("Escape", "saving", {}, { encoding: false })).toBeNull();
		expect(keyAction("Escape", "saving")).toBeNull();
		for (const k of ["Enter", "p", " "]) expect(keyAction(k, "saving", {}, { encoding: true })).toBeNull();
		expect(keyAction("Escape", "saving", { meta: true }, { encoding: true })).toBeNull();
	});
});

describe("trim step", () => {
	const trim = { secs: 12.5, posters: ["data:image/png;base64,AAA"] };
	test("shows the timeline only with a trim payload", () => {
		expect(view(st("trim", 12_500, { trim })).showTrim).toBe(true);
		expect(view(st("trim", 12_500)).showTrim).toBe(false);
		expect(view(st("recording", 0, { trim })).showTrim).toBe(false);
	});
	test("offers no recording buttons while trimming", () => {
		const v = view(st("trim", 12_500, { trim }));
		expect([v.showStop, v.showPause, v.showResume, v.showCancel, v.showOpen, v.showClose]).toEqual([
			false,
			false,
			false,
			false,
			false,
			false,
		]);
	});
	/** A finished clip is never lost to a stray Esc; Enter saves it as it is. */
	test("Enter saves, Esc and every other key do nothing", () => {
		expect(keyAction("Enter", "trim")).toBe("save");
		for (const k of ["Escape", "p", " ", "x"]) expect(keyAction(k, "trim"), k).toBeNull();
		expect(keyAction("Enter", "trim", { meta: true })).toBeNull();
	});
});

describe("clampTrim (mirror of clamp_trim in recopts.rs)", () => {
	test("keeps a range inside the clip as dragged", () => expect(clampTrim(10, 2, 7.5)).toEqual([2, 7.5]));
	test("snaps to the bounds within 0.15 s, not beyond", () => {
		expect(clampTrim(10, 0.1, 9.9)).toEqual([0, 10]);
		expect(clampTrim(10, 0.16, 9.84)).toEqual([0.16, 9.84]);
		expect(clampTrim(10, -5, 99)).toEqual([0, 10]);
	});
	test("puts crossed handles in order", () => expect(clampTrim(10, 8, 3)).toEqual([3, 8]));
	test("never keeps less than half a second and stays inside the clip", () => {
		const [s, e] = clampTrim(10, 5, 5.1);
		expect(e - s).toBeCloseTo(0.5, 9);
		expect(clampTrim(10, 0, 0.2)).toEqual([0, 0.5]);
		expect(clampTrim(10, 9.8, 10)).toEqual([9.5, 10]);
	});
	test("nothing to trim in a too-short clip or from junk", () => {
		expect(clampTrim(0.4, 0, 0.4)).toBeNull();
		expect(clampTrim(Number.NaN, 0, 1)).toBeNull();
		expect(clampTrim(10, Number.NaN, 1)).toBeNull();
		expect(clampTrim(0.5, 0, 0.5)).toEqual([0, 0.5]);
	});
});

describe("trim handles", () => {
	test("a pointer maps to seconds across the track and clamps at both ends", () => {
		expect(secsAt(150, 50, 200, 10)).toBe(5);
		expect(secsAt(0, 50, 200, 10)).toBe(0);
		expect(secsAt(999, 50, 200, 10)).toBe(10);
		expect(secsAt(100, 50, 0, 10)).toBe(0);
		expect(secsAt(Number.NaN, 50, 200, 10)).toBe(0);
	});
	test("a handle cannot cross the other or leave less than 0.5 s", () => {
		expect(moveHandle("start", 9, [2, 6], 10)).toEqual([5.5, 6]);
		expect(moveHandle("end", 1, [2, 6], 10)).toEqual([2, 2.5]);
		expect(moveHandle("start", -3, [2, 6], 10)).toEqual([0, 6]);
		expect(moveHandle("end", 99, [2, 6], 10)).toEqual([2, 10]);
	});
	test("fractions and labels", () => {
		expect(fractionOf(5, 10)).toBe(0.5);
		expect(fractionOf(50, 10)).toBe(1);
		expect(fractionOf(1, 0)).toBe(0);
		expect(formatSecs(0)).toBe("0:00.0");
		expect(formatSecs(7.26)).toBe("0:07.2");
		expect(formatSecs(65.04)).toBe("1:05.0");
		expect(formatSecs(Number.NaN)).toBe("0:00.0");
	});
});

describe("saveArgs", () => {
	/** Keeping everything must not rewrite the file: Rust renames it when it gets no range. */
	test("the whole clip (or a drag that snaps to it) sends nothing", () => {
		expect(saveArgs(10, [0, 10])).toEqual({});
		expect(saveArgs(10, [0.1, 9.9])).toEqual({});
	});
	test("a real cut sends the clamped range", () => {
		expect(saveArgs(10, [2, 7])).toEqual({ start: 2, end: 7 });
		expect(saveArgs(10, [0, 6])).toEqual({ start: 0, end: 6 });
	});
	test("a clip too short to trim sends nothing", () => expect(saveArgs(0.4, [0, 0.4])).toEqual({}));
});
