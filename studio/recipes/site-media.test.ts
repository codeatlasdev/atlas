import { expect, test } from "bun:test";
import { parseRecipeV1 } from "../src/run/recipe.ts";
import { LOOPS, SHOTS } from "../src/site-media.ts";
import recipe from "./site-media.ts";

// This file is excluded from studio's tsc (it imports the app's source); bun runs it.
const parsed = parseRecipeV1(recipe);

test("every published loop and shot has a capture that records it, and every `shot` step it promises exists", () => {
	for (const l of LOOPS) expect(parsed.captures.find((c) => c.id === l.capture), `capture for loop-${l.name}`).toBeDefined();
	for (const s of SHOTS) {
		const c = parsed.captures.find((x) => x.id === s.capture);
		expect(c, `capture for shot-${s.name}`).toBeDefined();
		expect(c!.shots, `shot ${s.shot} of ${s.capture}`).toContain(s.shot);
	}
});

test("every capture's window is 16:10 and the largest the agent display admits (1248x780: 1272x784 is hidden by the window watchdog), so the loops' 1280x800 scale is exact", () => {
	for (const c of parsed.captures) expect(c.script.size).toEqual({ width: 1248, height: 780 });
});

test("a loop fits inside its capture: it starts after the settle and ends before the recording does", () => {
	for (const l of LOOPS) {
		const c = parsed.captures.find((x) => x.id === l.capture)!;
		// declared script time is a floor of the recording length (waits and the recorder's start only add to it)
		const declaredMs = c.script.steps.reduce((n, s) => n + ("ms" in s ? s.ms : 0), 0);
		expect(l.from + l.seconds, `loop-${l.name} runs past its capture (${declaredMs} ms of declared time)`).toBeLessThanOrEqual(declaredMs / 1000 + 2);
		expect(l.seconds).toBeGreaterThanOrEqual(9);
		expect(l.seconds).toBeLessThanOrEqual(14);
	}
});

test("recordings of stateful scenes start from a fixture, so the footage never shows a stopped, empty stack", () => {
	for (const id of ["panel", "logs", "agents", "mcp"]) expect(parsed.captures.find((c) => c.id === id)!.fixture, id).toBeDefined();
});

test("no step of a recording sends input that the app would treat as a user's: only DOM events and store calls (the grammar guarantees it); `key` combos are the app's own shortcuts", () => {
	for (const c of parsed.captures) for (const s of c.script.steps) if (s.do === "key") expect(s.combo).toMatch(/^(ctrl-cmd-s|alt-cmd-(left|right)|enter|escape)$/);
});
