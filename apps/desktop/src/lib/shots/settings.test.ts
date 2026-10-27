// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
	KEY_FIELDS,
	activePresetId,
	applyPreset,
	clashes,
	historyGroups,
	CORNER_OPTIONS,
	SCALE_OPTIONS,
	TIMER_OPTIONS,
	historyLabel,
	keyLabel,
	presetAvailable,
	recordingResult,
	sameStroke,
	systemKeysUsed,
	verdictText,
} from "./settings.ts";

/** The REAL serialization of the defaults and presets (`prefs.rs` fails when this file is stale). */
const real = JSON.parse(readFileSync(new URL("../__fixtures__/capture-prefs.json", import.meta.url), "utf8"));

const keys = (o = {}) => ({
	allInOne: "",
	region: "",
	previous: "",
	window: "",
	fullscreen: "",
	ocr: "",
	record: "",
	gif: "",
	...o,
});
const ev = (init) => ({
	key: "",
	code: "",
	metaKey: false,
	ctrlKey: false,
	altKey: false,
	shiftKey: false,
	...init,
});

describe("sameStroke", () => {
	const MODS = ["ctrl", "alt", "shift", "cmd"];
	const perms = (xs) =>
		xs.length < 2
			? [xs]
			: xs.flatMap((x, i) => perms([...xs.slice(0, i), ...xs.slice(i + 1)]).map((p) => [x, ...p]));

	test("every permutation of the modifiers is the same stroke", () => {
		for (const n of [2, 3, 4]) {
			const mods = MODS.slice(0, n);
			const base = [...mods, "s"].join("-");
			for (const p of perms(mods)) expect(sameStroke(base, [...p, "s"].join("-")), p.join()).toBe(true);
		}
	});
	test("the documented case: shift-cmd-s == cmd-shift-s", () => {
		expect(sameStroke("shift-cmd-s", "cmd-shift-s")).toBe(true);
	});
	test("case and aliases do not matter", () => {
		expect(sameStroke("Shift-CMD-S", "cmd-shift-s")).toBe(true);
		expect(sameStroke("meta-shift-s", "cmd-shift-s")).toBe(true);
	});
	test("a different key or a different modifier set is a different stroke", () => {
		expect(sameStroke("cmd-shift-s", "cmd-shift-d")).toBe(false);
		expect(sameStroke("cmd-shift-s", "cmd-s")).toBe(false);
		expect(sameStroke("cmd-shift-s", "ctrl-shift-s")).toBe(false);
	});
	test("empty strokes are never the same as anything, not even each other", () => {
		expect(sameStroke("", "")).toBe(false);
		expect(sameStroke("", "cmd-s")).toBe(false);
		expect(sameStroke("  ", "  ")).toBe(false);
	});
	test("an unparseable stroke only equals itself case-insensitively", () => {
		expect(sameStroke("hyper-s", "hyper-s")).toBe(true);
		expect(sameStroke("hyper-s", "cmd-s")).toBe(false);
	});
});

describe("recordingResult", () => {
	test("a modifier alone waits", () => {
		expect(recordingResult(ev({ key: "Shift", code: "ShiftLeft", shiftKey: true }), true)).toEqual({
			kind: "wait",
		});
	});
	test("a full chord is a canonical stroke", () => {
		expect(recordingResult(ev({ key: "s", code: "KeyS", metaKey: true, shiftKey: true }), true)).toEqual({
			kind: "stroke",
			stroke: "shift-cmd-s",
		});
	});
	test("Escape cancels", () => {
		expect(recordingResult(ev({ key: "Escape", code: "Escape" }), true)).toEqual({ kind: "cancel" });
	});
	test("a key without cmd/ctrl/alt is refused with a reason", () => {
		const r = recordingResult(ev({ key: "s", code: "KeyS", shiftKey: true }), true);
		expect(r.kind).toBe("refuse");
		expect(r.reason).toContain("⌘");
	});
});

describe("keyLabel", () => {
	test("glyphs for a stroke, order-insensitive input", () => {
		expect(keyLabel("shift-cmd-s", true)).toBe("⇧⌘S");
		expect(keyLabel("cmd-shift-s", true)).toBe("⇧⌘S");
	});
	test("empty stroke reads Not set", () => {
		expect(keyLabel("", true)).toBe("Not set");
		expect(keyLabel("   ", true)).toBe("Not set");
	});
	test("off macOS it is spelled with words", () => {
		expect(keyLabel("shift-cmd-s", false)).toBe("Ctrl+Shift+S");
	});
});

describe("applyPreset", () => {
	const prefs = {
		folder: "/tmp/x",
		keys: keys({ region: "cmd-1" }),
		after: { copy: true, quickAccess: false, dismissSecs: 7, sound: true },
		highlightCursor: true,
		enabled: false,
		history: 42,
	};
	const preset = {
		id: "p",
		label: "P",
		note: "n",
		keys: keys({ region: "cmd-shift-4", gif: "cmd-shift-g" }),
	};
	test("replaces only keys", () => {
		const next = applyPreset(prefs, preset);
		expect(next.keys).toEqual(preset.keys);
		expect({ ...next, keys: 0 }).toEqual({ ...prefs, keys: 0 });
	});
	test("does not mutate the input and does not alias the preset's keys", () => {
		const before = structuredClone(prefs);
		const next = applyPreset(prefs, preset);
		expect(prefs).toEqual(before);
		expect(next.keys).not.toBe(preset.keys);
		next.keys.region = "changed";
		expect(preset.keys.region).toBe("cmd-shift-4");
	});
});

describe("activePresetId", () => {
	const presets = [
		{
			id: "atlas",
			label: "Atlas",
			note: null,
			keys: keys({
				allInOne: "ctrl-alt-1",
				previous: "ctrl-alt-2",
				ocr: "ctrl-alt-8",
				region: "ctrl-alt-4",
				window: "ctrl-alt-5",
				fullscreen: "ctrl-alt-3",
				record: "ctrl-alt-6",
				gif: "ctrl-alt-7",
			}),
		},
		{
			id: "macos",
			label: "macOS",
			note: "n",
			keys: keys({
				allInOne: "shift-cmd-5",
				previous: "shift-cmd-2",
				ocr: "shift-cmd-8",
				region: "shift-cmd-4",
				window: "shift-cmd-7",
				fullscreen: "shift-cmd-3",
				record: "shift-cmd-6",
				gif: "shift-cmd-9",
			}),
		},
		{
			id: "cleanshot",
			label: "CleanShot",
			note: null,
			keys: keys({
				allInOne: "shift-cmd-a",
				previous: "shift-cmd-p",
				ocr: "shift-cmd-t",
				region: "shift-cmd-s",
				window: "shift-cmd-w",
				fullscreen: "shift-cmd-f",
				record: "shift-cmd-r",
				gif: "shift-cmd-g",
			}),
		},
	];
	test("each preset's own keys select it", () => {
		for (const p of presets) expect(activePresetId(p.keys, presets), p.id).toBe(p.id);
	});
	test("order of modifiers does not matter", () => {
		const swapped = keys({
			allInOne: "cmd-shift-a",
			previous: "cmd-shift-p",
			ocr: "cmd-shift-t",
			region: "cmd-shift-s",
			window: "cmd-shift-w",
			fullscreen: "cmd-shift-f",
			record: "cmd-shift-r",
			gif: "cmd-shift-g",
		});
		expect(activePresetId(swapped, presets)).toBe("cleanshot");
	});
	test("editing a single key is custom (null), for every field of every preset", () => {
		for (const p of presets)
			for (const f of Object.keys(p.keys)) {
				expect(activePresetId({ ...p.keys, [f]: "cmd-alt-9" }, presets), `${p.id}.${f}`).toBeNull();
				expect(activePresetId({ ...p.keys, [f]: "" }, presets), `${p.id}.${f} cleared`).toBeNull();
			}
	});
	test("all empty strings is custom, even if a preset were also empty", () => {
		expect(activePresetId(keys(), presets)).toBeNull();
		expect(activePresetId(keys(), [{ id: "none", label: "None", note: null, keys: keys() }])).toBeNull();
	});
	test("no presets loaded yet is custom", () => {
		expect(activePresetId(presets[0].keys, [])).toBeNull();
	});
});

describe("verdictText", () => {
	test("ok is empty, warn and refused carry the reason", () => {
		expect(verdictText({ kind: "ok" })).toBe("");
		expect(verdictText({ kind: "warn", reason: "Used by Spotlight" })).toBe("Used by Spotlight");
		expect(verdictText({ kind: "refused", reason: "Taken" })).toBe("Taken");
	});
	test("null (not checked yet) is empty", () => {
		expect(verdictText(null)).toBe("");
	});
});

describe("clashes", () => {
	test("no clash among distinct keys", () => {
		expect(clashes(keys({ region: "cmd-1", window: "cmd-2" }), [])).toBeNull();
	});
	test("empties never clash with each other", () => {
		expect(clashes(keys(), ["", ""])).toBeNull();
		expect(clashes(keys({ region: "cmd-1" }), [""])).toBeNull();
	});
	test("two capture keys that are the same stroke clash, in any case and modifier order", () => {
		const s = clashes(keys({ region: "shift-cmd-s", record: "CMD-Shift-S" }), []);
		expect(s).toBeString();
		expect(s).toContain("Region");
		expect(s).toContain("Record");
	});
	test("names the FIRST duplicate in field order", () => {
		const s = clashes(keys({ region: "cmd-1", window: "cmd-2", fullscreen: "cmd-2", record: "cmd-1" }), []);
		expect(s).toContain("Region");
		expect(s).toContain("Record");
		expect(s).not.toContain("Window");
	});
	test("a capture key equal to a dock key clashes and says it is the dock's", () => {
		const s = clashes(keys({ gif: "ctrl-alt-space" }), ["alt-ctrl-space"]);
		expect(s).toContain("GIF");
		expect(s.toLowerCase()).toContain("dock");
	});
	test("a dock key that is empty or equals another dock key is not our business", () => {
		expect(clashes(keys({ gif: "cmd-1" }), ["cmd-2", "cmd-2", ""])).toBeNull();
	});
});

describe("historyGroups", () => {
	const e = (at, name = "a") => ({
		path: `/s/${name}-${at}.png`,
		kind: "screenshot",
		at,
		width: 1,
		height: 1,
		bytes: 1,
		secs: null,
	});
	const now = new Date(2026, 9, 21, 15, 30).getTime(); // Wed 21 Oct 2026, local
	const at = (y, m, d, h = 12, mi = 0, s = 0) => new Date(y, m, d, h, mi, s).getTime();
	const titles = (gs) => gs.map((g) => g.title);

	test("empty is no groups", () => {
		expect(historyGroups([], now)).toEqual([]);
	});
	test("each bucket, in order, newest first inside, empty buckets omitted", () => {
		const entries = [
			e(at(2026, 9, 21, 9), "today-old"),
			e(at(2026, 9, 21, 15, 29), "today-new"),
			e(at(2026, 9, 20, 23), "yest"),
			e(at(2026, 9, 17, 8), "week"),
			e(at(2026, 8, 1), "earlier"),
		];
		const gs = historyGroups(entries, now);
		expect(titles(gs)).toEqual(["Today", "Yesterday", "This week", "Earlier"]);
		expect(gs[0].entries.map((x) => x.path)).toEqual([entries[1].path, entries[0].path]);
		expect(titles(historyGroups([entries[4]], now))).toEqual(["Earlier"]);
	});
	test("23:59:59 yesterday is Yesterday, 00:00:00 today is Today", () => {
		const gs = historyGroups([e(at(2026, 9, 20, 23, 59, 59)), e(at(2026, 9, 21, 0, 0, 0))], now);
		expect(titles(gs)).toEqual(["Today", "Yesterday"]);
		expect(gs[0].entries).toHaveLength(1);
		expect(gs[1].entries).toHaveLength(1);
	});
	test("one ms before local midnight is still the day before", () => {
		const midnight = at(2026, 9, 21, 0, 0, 0);
		expect(titles(historyGroups([e(midnight - 1)], now))).toEqual(["Yesterday"]);
		expect(titles(historyGroups([e(midnight)], now))).toEqual(["Today"]);
	});
	test("This week is the 7 local days before Yesterday's start; day 8 is Earlier", () => {
		// Yesterday = 20th, so "This week" covers 14th..19th (the 6 days before yesterday) .. boundary: start of the 14th.
		expect(titles(historyGroups([e(at(2026, 9, 14, 0, 0, 0))], now))).toEqual(["This week"]);
		expect(titles(historyGroups([e(at(2026, 9, 13, 23, 59, 59))], now))).toEqual(["Earlier"]);
	});
	test("across a DST change the day boundaries stay local midnight (23 h and 25 h days)", () => {
		// Spring forward (US): 8 Mar 2026 has 23 h; fall back: 1 Nov 2026 has 25 h. Whatever TZ the runner has,
		// the boundaries are built from local calendar dates, never from "now - 24 h".
		for (const [y, m, d] of [
			[2026, 2, 8],
			[2026, 2, 9],
			[2026, 10, 1],
			[2026, 10, 2],
			[2026, 2, 29],
			[2026, 9, 25],
		]) {
			const n = at(y, m, d, 12);
			const todayStart = at(y, m, d, 0);
			const prevDayLate = at(y, m, d - 1, 23, 59, 59);
			expect(titles(historyGroups([e(todayStart), e(prevDayLate)], n)), `${y}-${m + 1}-${d}`).toEqual([
				"Today",
				"Yesterday",
			]);
			// Two local days back is This week, never Yesterday, even if only 47 h or 49 h of wall time elapsed.
			expect(titles(historyGroups([e(at(y, m, d - 2, 0, 0, 0))], n)), `${y}-${m + 1}-${d} -2`).toEqual([
				"This week",
			]);
		}
	});
	test("a timestamp in the future (clock skew) is Today and never throws", () => {
		const future = now + 3 * 86_400_000;
		const gs = historyGroups([e(future), e(now - 1000)], now);
		expect(titles(gs)).toEqual(["Today"]);
		expect(gs[0].entries[0].at).toBe(future);
	});
	test("non-finite `at` or `now` does not throw", () => {
		expect(() => historyGroups([e(Number.NaN)], now)).not.toThrow();
		expect(() => historyGroups([e(now)], Number.NaN)).not.toThrow();
	});
	test("input is not mutated", () => {
		const entries = [e(at(2026, 9, 20)), e(at(2026, 9, 21))];
		const copy = structuredClone(entries);
		historyGroups(entries, now);
		expect(entries).toEqual(copy);
	});
});

describe("historyLabel", () => {
	const base = {
		path: "/Users/me/Shots/Atlas 2026-10-21 at 15.30.png",
		kind: "screenshot",
		at: 0,
		width: 1920,
		height: 1080,
		bytes: 2_500_000,
		secs: null,
	};
	test("filename, kind, size and dimensions for a still", () => {
		const l = historyLabel(base);
		expect(l.name).toBe("Atlas 2026-10-21 at 15.30.png");
		expect(l.kind).toBe("Screenshot");
		expect(l.size).toBe("2.4 MB");
		expect(l.detail).toBe("1920 × 1080 · 2.4 MB");
	});
	test("recordings and GIFs show the duration", () => {
		expect(historyLabel({ ...base, kind: "recording", secs: 75, path: "/a/b.mov" }).detail).toBe(
			"1920 × 1080 · 1:15 · 2.4 MB",
		);
		expect(historyLabel({ ...base, kind: "gif", secs: 4, path: "/a/b.gif" }).kind).toBe("GIF");
	});
	test("small sizes and a Windows-style path", () => {
		expect(historyLabel({ ...base, bytes: 512 }).size).toBe("512 B");
		expect(historyLabel({ ...base, bytes: 20_480 }).size).toBe("20 KB");
		expect(historyLabel({ ...base, path: "C:\\Users\\me\\a.png" }).name).toBe("a.png");
	});
});

describe("KEY_FIELDS", () => {
	test("every capture action in page order, each with a description", () => {
		expect(KEY_FIELDS.map((f) => f.field)).toEqual([
			"allInOne",
			"region",
			"previous",
			"window",
			"fullscreen",
			"ocr",
			"record",
			"gif",
		]);
		expect(KEY_FIELDS.map((f) => f.label)).toEqual([
			"All-in-One",
			"Region",
			"Previous area",
			"Window",
			"Screen",
			"Text",
			"Record",
			"GIF",
		]);
		for (const f of KEY_FIELDS) {
			expect(f.description).toMatch(/^[A-Z].*\.$/);
		}
	});
});

describe("contract with Rust: capture-prefs.json", () => {
	test("every key field the page lists is a field Rust serializes, and nothing is missing", () => {
		expect(KEY_FIELDS.map((f) => f.field).sort()).toEqual(Object.keys(real.prefs.keys).sort());
		for (const p of real.presets) {
			expect(Object.keys(p.keys).sort(), p.id).toEqual(Object.keys(real.prefs.keys).sort());
		}
	});
	test("the default preset is the active one for the default keys", () => {
		expect(activePresetId(real.prefs.keys, real.presets)).toBe("default");
	});
	test("each real preset selects itself and the defaults have no clash", () => {
		for (const p of real.presets) expect(activePresetId(p.keys, real.presets), p.id).toBe(p.id);
		expect(clashes(real.prefs.keys, [])).toBeNull();
		for (const p of real.presets) expect(clashes(p.keys, []), p.id).toBeNull();
	});
	test("Rust marks the preset that uses macOS's screenshot keys, and only that one", () => {
		const need = real.presets.filter((p) => p.needsSystemKeys).map((p) => p.id);
		expect(need).toEqual(["cleanshot"]);
		for (const p of real.presets) expect(systemKeysUsed(p.keys).length > 0, p.id).toBe(p.needsSystemKeys);
	});
});

describe("system screenshot keys", () => {
	const cleanshot = real.presets.find((p) => p.id === "cleanshot");
	const def = real.presets.find((p) => p.id === "default");
	test("a preset that needs them is unavailable until the person says they freed them", () => {
		expect(presetAvailable(cleanshot, { ...real.prefs, systemKeysFree: false })).toBe(false);
		expect(presetAvailable(cleanshot, { ...real.prefs, systemKeysFree: true })).toBe(true);
	});
	test("a preset that does not need them is always available", () => {
		expect(presetAvailable(def, { ...real.prefs, systemKeysFree: false })).toBe(true);
	});
	test("names which of the four macOS strokes a key set uses, in any modifier order", () => {
		expect(systemKeysUsed(cleanshot.keys).sort()).toEqual(["⇧⌘3", "⇧⌘4", "⇧⌘5", "⇧⌘6"]);
		expect(systemKeysUsed({ ...real.prefs.keys, region: "shift-cmd-4" })).toEqual(["⇧⌘4"]);
		expect(systemKeysUsed(real.prefs.keys)).toEqual([]);
		expect(systemKeysUsed({ ...real.prefs.keys, region: "cmd-shift-7" })).toEqual([]);
	});
});

describe("option lists", () => {
	/** Rust refuses a timer other than off/3/5/10 (`TIMER_VALUES`): the page must offer exactly those. */
	test("the timer offers only the values Rust accepts", () => {
		expect(TIMER_OPTIONS.map((o) => o.value)).toEqual(["0", "3", "5", "10"]);
		expect(TIMER_OPTIONS[0].label).toBe("Off");
		expect(TIMER_OPTIONS[1].label).toBe("3 seconds");
	});
	test("scale and corner options are exactly Rust's enum words", () => {
		expect(SCALE_OPTIONS.map((o) => o.value)).toEqual(["native", "1x"]);
		expect(CORNER_OPTIONS.map((o) => o.value)).toEqual(["bottomRight", "bottomLeft", "topRight", "topLeft"]);
		expect(real.prefs.still.scale).toBe("native");
		expect(real.prefs.quick.position).toBe("bottomRight");
	});
});
