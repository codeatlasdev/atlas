// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { agentSound, discoverySound, stackSound } from "./triggers.ts";

const svc = (name, state, extra = {}) => ({
	name,
	state,
	exit_code: null,
	restarts: 0,
	note: null,
	...extra,
});

describe("stackSound: services of one project", () => {
	test("the last service turning running after a start is 'stack-up'", () => {
		const prev = [svc("db", "running"), svc("api", "starting")];
		const next = [svc("db", "running"), svc("api", "running")];
		expect(stackSound(prev, next)).toBe("stack-up");
	});

	test("a service turning failed is 'service-failed', even when others are fine", () => {
		const prev = [svc("db", "running"), svc("api", "starting")];
		const next = [svc("db", "running"), svc("api", "failed", { exit_code: 1 })];
		expect(stackSound(prev, next)).toBe("service-failed");
	});

	test("failure wins over a simultaneous 'everything else is up'", () => {
		const prev = [svc("a", "starting"), svc("b", "starting")];
		const next = [svc("a", "running"), svc("b", "failed")];
		expect(stackSound(prev, next)).toBe("service-failed");
	});

	test("a service that was already failed and still is does not sound again", () => {
		const prev = [svc("api", "failed")];
		expect(stackSound(prev, [svc("api", "failed", { restarts: 3 })])).toBeNull();
	});

	test("a health tick that repeats the same states is silent", () => {
		const s = [svc("db", "running"), svc("api", "running")];
		expect(stackSound(s, structuredClone(s))).toBeNull();
	});

	test("the first list for a project (resync, history replay) is silent", () => {
		expect(stackSound([], [svc("db", "running"), svc("api", "running")])).toBeNull();
		expect(stackSound(undefined, [svc("api", "failed")])).toBeNull();
	});

	test("a stack that was already up and loses one service to 'stopped' is silent", () => {
		const prev = [svc("a", "running"), svc("b", "running")];
		expect(stackSound(prev, [svc("a", "running"), svc("b", "stopped")])).toBeNull();
	});

	test("deselected or one-shot completed services do not block 'stack-up'", () => {
		const prev = [
			svc("api", "starting"),
			svc("migrate", "stopped", { completed_ok: true }),
			svc("docs", "stopped", { deselected: true }),
		];
		const next = [
			svc("api", "running"),
			svc("migrate", "stopped", { completed_ok: true }),
			svc("docs", "stopped", { deselected: true }),
		];
		expect(stackSound(prev, next)).toBe("stack-up");
	});

	test("an empty next list is silent", () => {
		expect(stackSound([svc("a", "running")], [])).toBeNull();
	});

	test("starting -> starting is silent (nothing healthy yet)", () => {
		expect(stackSound([svc("a", "starting")], [svc("a", "starting")])).toBeNull();
	});

	// The rows below are the REAL runner payload (`__fixtures__/service-states.json`, written by the Rust wire test).
	const [real_api, real_web, real_db] = JSON.parse(
		readFileSync(join(import.meta.dir, "..", "__fixtures__", "service-states.json"), "utf8"),
	);
	const as = (row, state) => ({ ...row, state });

	test("a service that hangs (running -> unhealthy) sounds like a failure", () => {
		const prev = [real_db, as(real_web, "running")];
		expect(stackSound(prev, [real_db, real_web])).toBe("service-failed");
	});

	test("an already-unhealthy service does not sound again on every refresh", () => {
		expect(stackSound([real_web], [{ ...real_web, note: "unhealthy for 9s: 3 missed probes" }])).toBeNull();
	});

	test("a degraded service is not 'up': the stack-up jingle waits until it recovers", () => {
		const prev = [real_db, as(real_api, "starting")];
		expect(stackSound(prev, [real_db, real_api])).toBeNull();
		expect(stackSound(prev, [real_db, as(real_api, "running")])).toBe("stack-up");
	});
});

describe("agentSound", () => {
	test("done and the two waiting states map to their earcons, others are silent", () => {
		expect(agentSound("done")).toBe("agent-done");
		expect(agentSound("waiting")).toBe("agent-needs-you");
		expect(agentSound("blocked")).toBe("agent-needs-you");
		expect(agentSound("working")).toBeNull();
		expect(agentSound("idle")).toBeNull();
	});
});

describe("discoverySound", () => {
	test("a finished scan that found something sounds; cancelled, errored or empty ones do not", () => {
		expect(discoverySound({ cancelled: false }, 3, null)).toBe("discovery-done");
		expect(discoverySound({ cancelled: true }, 3, null)).toBeNull();
		expect(discoverySound({ cancelled: false }, 0, null)).toBeNull();
		expect(discoverySound(null, 3, null)).toBeNull();
		expect(discoverySound({ cancelled: false }, 3, "boom")).toBeNull();
	});
});
