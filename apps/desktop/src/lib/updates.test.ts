// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	CHECK_EVERY_MS,
	EMPTY_PREFS,
	compareVersions,
	isSkipped,
	justUpdated,
	parsePrefs,
	selectNotes,
	shouldNotify,
	shouldRecheck,
	showPill,
	stepsFromLog,
	stripVersionHeading,
} from "./updates.ts";

const rel = (version) => ({ version, url: "", published_at: "", notes: "" });
const prefs = (p = {}) => ({ ...EMPTY_PREFS, ...p });

describe("compareVersions", () => {
	test("numeric, not lexical; pre-release before release", () => {
		expect(compareVersions("1.10.0", "1.9.9")).toBe(1);
		expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
		expect(compareVersions("1.0.0-rc.1", "1.0.0")).toBe(-1);
		expect(compareVersions("v1.0.1", "1.0.0")).toBe(1);
	});
});

describe("selectNotes", () => {
	const all = [rel("1.0.0"), rel("1.0.2"), rel("0.9.0"), rel("1.0.1"), rel("1.0.2"), rel("1.0.3")];
	test("every version in (installed, latest], newest first, deduplicated", () => {
		expect(selectNotes(all, "1.0.0", "1.0.2").map((r) => r.version)).toEqual(["1.0.2", "1.0.1"]);
	});
	test("installed == latest yields just that release", () => {
		expect(selectNotes(all, "1.0.1", "1.0.1").map((r) => r.version)).toEqual(["1.0.1"]);
	});
	test("nothing newer yields nothing", () => {
		expect(selectNotes(all, "1.0.3", "1.0.3").map((r) => r.version)).toEqual(["1.0.3"]);
		expect(selectNotes([rel("1.0.0")], "1.0.0", "1.0.2")).toEqual([]);
	});
});

describe("skip", () => {
	test("skips exactly that version; a newer one comes back", () => {
		const p = prefs({ skipped: "1.0.2" });
		expect(isSkipped(p, "1.0.2")).toBe(true);
		expect(isSkipped(p, "1.0.3")).toBe(false);
		expect(isSkipped(p, null)).toBe(false);
	});
	test("pill hides when skipped, linked, or nothing is available", () => {
		expect(showPill(true, false, prefs(), "1.0.2")).toBe(true);
		expect(showPill(true, false, prefs({ skipped: "1.0.2" }), "1.0.2")).toBe(false);
		expect(showPill(true, true, prefs(), "1.0.2")).toBe(false);
		expect(showPill(false, false, prefs(), "1.0.2")).toBe(false);
	});
	test("persisted shape survives a round trip and garbage", () => {
		const p = prefs({ skipped: "1.0.2", notified: "1.0.2", lastSeen: "1.0.1", auto: false });
		expect(parsePrefs(JSON.parse(JSON.stringify(p)))).toEqual(p);
		expect(parsePrefs(undefined)).toEqual(EMPTY_PREFS);
		expect(parsePrefs({ skipped: 5, auto: "no" })).toEqual({ ...EMPTY_PREFS, auto: true });
	});
});

describe("shouldNotify", () => {
	const on = { notificationsOn: true, linked: false };
	test("once per version", () => {
		expect(shouldNotify(prefs(), "1.0.2", on)).toBe(true);
		expect(shouldNotify(prefs({ notified: "1.0.2" }), "1.0.2", on)).toBe(false);
		expect(shouldNotify(prefs({ notified: "1.0.2" }), "1.0.3", on)).toBe(true);
	});
	test("not when notifications are off, dev build, skipped or no version", () => {
		expect(shouldNotify(prefs(), "1.0.2", { ...on, notificationsOn: false })).toBe(false);
		expect(shouldNotify(prefs(), "1.0.2", { ...on, linked: true })).toBe(false);
		expect(shouldNotify(prefs({ skipped: "1.0.2" }), "1.0.2", on)).toBe(false);
		expect(shouldNotify(prefs(), null, on)).toBe(false);
	});
});

describe("justUpdated", () => {
	test("only when the running version is newer than the last seen one", () => {
		expect(justUpdated(prefs({ lastSeen: "1.0.1" }), "1.0.2")).toBe(true);
		expect(justUpdated(prefs({ lastSeen: "1.0.2" }), "1.0.2")).toBe(false);
		expect(justUpdated(prefs({ lastSeen: "1.0.3" }), "1.0.2")).toBe(false);
		expect(justUpdated(prefs(), "1.0.2")).toBe(false);
	});
});

describe("shouldRecheck", () => {
	test("on focus at most every 6h, and never with auto-check off", () => {
		const now = 10 * CHECK_EVERY_MS;
		expect(shouldRecheck(prefs(), null, now)).toBe(true);
		expect(shouldRecheck(prefs(), now - 1000, now)).toBe(false);
		expect(shouldRecheck(prefs(), now - CHECK_EVERY_MS, now)).toBe(true);
		expect(shouldRecheck(prefs({ auto: false }), null, now)).toBe(false);
	});
});

describe("stepsFromLog", () => {
	const states = (s) => s.map((x) => x.state);
	test("starts on download before any output", () => {
		expect(states(stepsFromLog([], "running"))).toEqual(["active", "pending", "pending"]);
	});
	test("advances with the CLI's phases", () => {
		const log = ["◆ Checking for updates...", "◆ Downloading v1.0.2..."];
		expect(states(stepsFromLog(log, "running"))[0]).toBe("active");
		const more = [...log, "◆ Installing...", "✓ Updated to v1.0.2"];
		expect(states(stepsFromLog(more, "running"))).toEqual(["done", "done", "active"]);
	});
	test("done completes all, failed marks the active step", () => {
		expect(states(stepsFromLog(["Downloading"], "done"))).toEqual(["done", "done", "done"]);
		expect(states(stepsFromLog(["Downloading", "Installing..."], "failed"))).toEqual([
			"done",
			"failed",
			"pending",
		]);
	});
});

test("stripVersionHeading drops the leading release heading only", () => {
	expect(stripVersionHeading("## [1.0.1] — 2026-10-11\n\n### Changed\n\n- x")).toBe("### Changed\n\n- x");
	expect(stripVersionHeading("### Changed\n- x")).toBe("### Changed\n- x");
});
