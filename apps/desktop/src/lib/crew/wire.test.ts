// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../../__fixtures__/crewmate_tasks.json" with { type: "json" };
import { normalizeCrewmate, normalizeTask } from "./wire.ts";

// The fixture is what hostd serializes for `crewmate_tasks` (the Rust contract test writes it), so these run the real
// wire JSON through the boundary instead of a hand-written mock.
describe("normalizeTask: hostd seconds become milliseconds", () => {
	test("every task ts is a unix-seconds value on the wire and a ms value after", () => {
		for (const raw of fixture.tasks) {
			expect(raw.ts).toBeLessThan(2e10);
			expect(normalizeTask(raw).ts).toBe(raw.ts * 1000);
			expect(normalizeTask(raw).ts).toBeGreaterThan(1e12);
		}
	});

	test("started_at and finished_at convert when present and stay absent when not", () => {
		const running = fixture.tasks.find((t) => t.started_at && !t.finished_at);
		expect(running).toBeDefined();
		const out = normalizeTask(running);
		expect(out.started_at).toBe(running.started_at * 1000);
		expect(out.finished_at).toBeUndefined();
		const done = fixture.tasks.find((t) => t.finished_at);
		expect(normalizeTask(done).finished_at).toBe(done.finished_at * 1000);
	});

	test("every step ts converts, and its text and group survive", () => {
		const withSteps = fixture.tasks.find((t) => t.steps?.length);
		expect(withSteps).toBeDefined();
		const out = normalizeTask(withSteps);
		out.steps.forEach((s, i) => {
			expect(s.ts).toBe(withSteps.steps[i].ts * 1000);
			expect(s.text).toBe(withSteps.steps[i].text);
		});
	});

	test("does not mutate the wire object", () => {
		const raw = structuredClone(fixture.tasks[0]);
		normalizeTask(raw);
		expect(raw).toEqual(fixture.tasks[0]);
	});

	test("the other fields pass through untouched", () => {
		const raw = fixture.tasks.find((t) => t.action);
		const out = normalizeTask(raw);
		expect(out.action).toEqual(raw.action);
		expect(out.id).toBe(raw.id);
		expect(out.preparing).toBe(raw.preparing);
	});
});

describe("normalizeCrewmate", () => {
	const base = { id: "c1", name: "Nami", role: "", agent: "omp", approval: "ask", state: "idle", queued: 0 };

	test("last.ts converts to milliseconds", () => {
		const out = normalizeCrewmate({ ...base, last: { text: "done", state: "done", ts: 1761213000 } });
		expect(out.last.ts).toBe(1761213000 * 1000);
		expect(out.last.text).toBe("done");
	});

	test("a crewmate that never answered has no last and is returned as is", () => {
		const c = { ...base };
		expect(normalizeCrewmate(c)).toBe(c);
	});
});
