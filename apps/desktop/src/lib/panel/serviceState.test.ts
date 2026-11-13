// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fold } from "./activity.ts";
import { bandOf } from "./metrics.ts";
import { byAttention } from "./panel.ts";
import { isAlive, isGreen, needsAttention, problemText } from "./serviceState.ts";

/**
 * The REAL `state`/`rows` entries the runner sends (`service_json`, written by the Rust test
 * `service_states_fixture_is_the_real_wire_output`): a degraded, an unhealthy and a running service.
 * Never a hand-written mock: the keys here are the ones the producer serializes.
 */
const rows = JSON.parse(readFileSync(join(import.meta.dir, "..", "__fixtures__", "service-states.json"), "utf8"));
const [api, web, db] = rows;

describe("the real runner payload", () => {
	test("carries the state strings and probe keys the panel reads", () => {
		expect([api.state, web.state, db.state]).toEqual(["degraded", "unhealthy", "running"]);
		expect(api.ready).toBe(false);
		expect(api.probe_error).toContain("GET /ready");
		expect(web.unhealthy_misses).toBe(3);
		expect(web.unhealthy_lines).toEqual(["booting", "stuck in render"]);
		expect(typeof web.unhealthy_since).toBe("number");
		expect(db.ready).toBeNull();
	});
});

describe("classification", () => {
	test("only `running` is green: a hung or degraded service never reads as up-and-well", () => {
		expect(rows.filter(isGreen).map((r) => r.name)).toEqual(["db"]);
	});

	test("a degraded or unhealthy service still has a live process: restart/stop stay available", () => {
		for (const s of ["running", "starting", "degraded", "unhealthy"]) expect(isAlive({ state: s })).toBe(true);
		for (const s of ["failed", "stopped", "disabled", "suspended", "nope"]) expect(isAlive({ state: s })).toBe(false);
	});

	test("failed, unhealthy and degraded need attention; the rest do not", () => {
		expect(rows.filter(needsAttention).map((r) => r.name)).toEqual(["api", "web"]);
		expect(needsAttention({ state: "failed" })).toBe(true);
		for (const s of ["running", "starting", "stopped", "disabled", "suspended"])
			expect(needsAttention({ state: s })).toBe(false);
	});

	/** The activity drawer paints each state change by its tone; a new state must not fall to "info". */
	test("activity rows: unhealthy is a failure, degraded a warning, from the same state strings", () => {
		const st = (state: string) => ({ ts: 1, kind: "state", text: `web → ${state}` });
		expect(fold([st("unhealthy")])[0]).toMatchObject({ tone: "fail", type: "state" });
		expect(fold([st("degraded")])[0]).toMatchObject({ tone: "warn" });
		expect(fold([st("running")])[0]).toMatchObject({ tone: "ok" });
		expect(fold([st("something-new")])[0]).toMatchObject({ tone: "info" });
	});
});

describe("card banner", () => {
	test("degraded shows the probe error from the real payload", () => {
		expect(problemText(api)).toContain("GET /ready → 500");
	});

	test("unhealthy says how long and how many probes it missed, from the real payload", () => {
		const now = web.unhealthy_since * 1000 + 42_000; // `unhealthy_since` is epoch seconds, `now` ms
		const t = problemText(web, now);
		expect(t).toContain("42s");
		expect(t).toContain("3");
		expect(t.toLowerCase()).toContain("not responding");
	});

	test("failed keeps its note, else the exit code, else the generic text", () => {
		expect(problemText({ state: "failed", note: "gave up after 5 restarts", exit_code: 1 })).toBe("gave up after 5 restarts");
		expect(problemText({ state: "failed", note: null, exit_code: 3 })).toBe("Exited with code 3");
		expect(problemText({ state: "failed", note: null, exit_code: null })).toBe("Failed to start");
	});

	test("healthy and stopped services have no banner", () => {
		expect(problemText(db)).toBeNull();
		expect(problemText({ state: "stopped" })).toBeNull();
	});

	test("a degraded service without a probe error falls back to its note, then to a plain label", () => {
		expect(problemText({ state: "degraded", probe_error: null, note: "repeating error: db timeout" })).toBe("repeating error: db timeout");
		expect(problemText({ state: "degraded", probe_error: null, note: null })).toBe("Degraded");
	});

	test("clock skew never prints a negative duration", () => {
		expect(problemText(web, web.unhealthy_since * 1000 - 5_000)).toContain("0s");
	});
});

describe("ordering and bands", () => {
	test("attention order: failed, unhealthy, degraded, starting, running, the rest", () => {
		const mixed = [
			{ name: "run", state: "running" },
			{ name: "stop", state: "stopped" },
			{ name: "deg", state: "degraded" },
			{ name: "fail", state: "failed" },
			{ name: "start", state: "starting" },
			{ name: "hung", state: "unhealthy" },
		];
		expect(byAttention(mixed).map((r) => r.name)).toEqual(["fail", "hung", "deg", "start", "run", "stop"]);
	});

	test("history bands: a hung service is a failed band, a degraded one stays up", () => {
		expect(bandOf("unhealthy")).toBe("failed");
		expect(bandOf("degraded")).toBe("running");
		expect(bandOf("running")).toBe("running");
		expect(bandOf("failed")).toBe("failed");
		expect(bandOf("stopped")).toBe("stopped");
		expect(bandOf("suspended")).toBe("stopped");
	});
});
