// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../../__fixtures__/crewmate_tasks.json" with { type: "json" };
import { queuedLabel } from "./board/boardText.ts";
import { normalizeCrewmate, normalizeTask, withParkedTask } from "./wire.ts";

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

	// #296: hostd marks a task waiting for a server-wide agent slot; the apps showed a bare "Queued".
	test("a queued task waiting for a slot keeps waiting_for through the boundary", () => {
		const busy = fixture.tasks.find((t) => t.state === "queued");
		expect(busy).toBeDefined();
		expect(busy.waiting_for).toBe("server busy");
		const out = normalizeTask(busy);
		expect(out.waiting_for).toBe("server busy");
		expect(queuedLabel(out)).toBe("Queued · server busy");
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

// #524: the report's crew server (hostd alpha.24) listed a crewmate parked on a card as "working". A current hostd says
// "needs_you" itself and is believed; an older one is corrected where its reply still shows the card.
describe("a crewmate parked on a card is never shown as working", () => {
	const base = { id: "c1", name: "Gerente", role: "", agent: "omp", approval: "ask", state: "working", queued: 2 };
	const choose = { kind: "choose", question: "Which?", options: ["A", "B"] };

	test("what hostd says as `needs_you` is kept, and `working` with no card stays working", () => {
		expect(normalizeCrewmate({ ...base, state: "needs_you" }).state).toBe("needs_you");
		expect(normalizeCrewmate(base).state).toBe("working");
	});

	test("an older hostd's `working` with a parked task in its reply reads as needs_you", () => {
		const out = normalizeCrewmate({ ...base, waiting_task: { id: "t1", action: choose } });
		expect(out.state).toBe("needs_you");
		const refused = { kind: "refused", reasons: ["a command wrote a file outside the work tree"] };
		expect(normalizeCrewmate({ ...base, waiting_task: { id: "t1", action: refused } }).state).toBe("needs_you");
	});

	test("an older hostd with only `last` reads the same when the newest task is the parked one", () => {
		const last = { text: "pick", state: "waiting_action", ts: 1761213000 };
		expect(normalizeCrewmate({ ...base, last }).state).toBe("needs_you");
		expect(normalizeCrewmate({ ...base, last: { ...last, state: "queued" } }).state).toBe("working");
	});

	test("the open crewmate reads its own loaded tasks, which also show a card behind queued tasks", () => {
		const tasks = [
			{ id: "t1", state: "waiting_action", action: choose },
			{ id: "t2", state: "queued" },
			{ id: "t3", state: "queued" },
		];
		expect(withParkedTask(base, tasks).state).toBe("needs_you");
		expect(withParkedTask(base, tasks.slice(1)).state).toBe("working");
		expect(withParkedTask({ ...base, state: "idle" }, tasks).state).toBe("idle");
	});
});
