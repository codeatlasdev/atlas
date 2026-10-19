// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { applyEvent, idle, parseEvent, providerHint, startState } from "./commitMessage.ts";
import events from "./__fixtures__/commit-msg-events.json";

// The fixture is the REAL serialization of the Rust `Event` enum (written by `commit_msg::tests`), never a hand-made mock.
const [start, chunk, done, error, cancelled] = events.map(parseEvent);

test("every event the Rust side emits parses with the fields the UI reads", () => {
	expect(start).toEqual({
		kind: "start",
		agent: "omp",
		scope: "staged",
		digest: { summarized: 1, withheld: 2, truncated: 3, omitted: 0 },
	});
	expect(chunk).toEqual({ kind: "chunk", text: "feat: add" });
	expect(done).toEqual({ kind: "done", message: "feat: add x" });
	expect(error).toEqual({ kind: "error", message: "boom" });
	expect(cancelled).toEqual({ kind: "cancelled" });
});

test("an unknown event kind is rejected, not ignored into a stuck spinner", () => {
	expect(() => parseEvent({ kind: "mystery" })).toThrow();
	expect(() => parseEvent({ kind: "chunk" })).toThrow();
	expect(() => parseEvent(null)).toThrow();
});

test("a run goes idle → running → done, and the message streams into the box", () => {
	let s = startState("req1");
	expect(s.phase).toBe("running");
	s = applyEvent(s, start);
	expect(s.agent).toBe("omp");
	expect(s.scope).toBe("staged");
	s = applyEvent(s, chunk);
	expect(s.text).toBe("feat: add");
	s = applyEvent(s, done);
	expect(s).toMatchObject({ phase: "idle", text: "feat: add x" });
});

test("cancel and error end the run and keep what streamed so far (the author may still want it)", () => {
	let s = applyEvent(applyEvent(startState("r"), start), chunk);
	expect(applyEvent(s, cancelled)).toMatchObject({ phase: "idle", text: "feat: add", cancelled: true });
	expect(applyEvent(s, error)).toMatchObject({ phase: "idle", text: "feat: add", error: "boom" });
});

test("events of another request never touch this run", () => {
	const s = startState("mine");
	expect(applyEvent(s, chunk, "someone-else")).toBe(s);
});

test("the notice says what was not sent, and is empty when everything was", () => {
	expect(startState("r").notice).toBe("");
	const s = applyEvent(startState("r"), start);
	expect(s.notice).toContain("2 files withheld");
	expect(s.notice).toContain("3 cut");
	const clean = applyEvent(startState("r"), {
		...start,
		digest: { summarized: 0, withheld: 0, truncated: 0, omitted: 0 },
		scope: "all",
	});
	expect(clean.notice).toBe("Nothing was staged: used all changes");
});

test("provider hint: ready shows the button, off hides it, none explains the setup", () => {
	expect(providerHint({ state: "ready", agent: "omp" })).toEqual({ show: true, setup: false });
	expect(providerHint({ state: "off" })).toEqual({ show: false, setup: false });
	expect(providerHint({ state: "none" })).toEqual({ show: true, setup: true });
	expect(idle().phase).toBe("idle");
});
