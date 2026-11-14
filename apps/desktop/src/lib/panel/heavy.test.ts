// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../__fixtures__/heavy-status.json";
import { active, describe as describeJob, human, summary } from "./heavy.ts";

// Contract: __fixtures__/heavy-status.json is the REAL serialization of what the `heavy_status` command returns
// (crates/atlas-desktop/src/cmds/heavy.rs regenerates it with UPDATE_FIXTURES=1). The drawer reads it as is.
describe("the real heavy_status payload", () => {
	test("has the keys the drawer reads, in the shapes it expects", () => {
		expect(fixture.slots).toBeNumber();
		expect(fixture.mem_available_pct).toBeNumber();
		for (const j of [...fixture.running, ...fixture.queued]) {
			for (const k of ["id", "pid", "label", "cwd", "project", "state", "queued_at", "age_s"]) {
				expect(j[k], k).not.toBeUndefined();
			}
			expect(["queued", "running"]).toContain(j.state);
		}
		expect(fixture.running[0].slot).toBe(0);
		expect(fixture.queued[0].slot).toBeNull();
		expect(fixture.queued[1].eta_s).toBeNull();
	});

	test("summary says how many slots are busy and how many wait", () => {
		expect(summary(fixture)).toBe("1 of 2 slots busy · 2 waiting");
		expect(summary({ ...fixture, slots: 1, queued: [] })).toBe("1 of 1 slot busy");
	});

	test("a running row shows its time left, a queued row when it starts, an unknown ETA nothing", () => {
		expect(describeJob(fixture.running[0])).toEqual({
			title: "gate",
			detail: "atlas-wt-a · running 1m01s · ~2m59s left",
		});
		expect(describeJob(fixture.queued[0]).detail).toBe("atlas-wt-a · waiting 1m01s · starts in ~2m59s");
		expect(describeJob(fixture.queued[1]).detail).toBe("atlas-wt-a · waiting 1m01s");
	});
});

describe("active", () => {
	test("an empty or missing queue is not shown", () => {
		expect(active(null)).toBe(false);
		expect(active({ slots: 2, load: null, mem_available_pct: null, running: [], queued: [] })).toBe(false);
		expect(active(fixture)).toBe(true);
	});
});

describe("human", () => {
	test("seconds, minutes and hours", () => {
		expect([human(0), human(59), human(60), human(65), human(3599), human(3600), human(7380)]).toEqual([
			"0s",
			"59s",
			"1m00s",
			"1m05s",
			"59m59s",
			"1h00m",
			"2h03m",
		]);
	});
});
