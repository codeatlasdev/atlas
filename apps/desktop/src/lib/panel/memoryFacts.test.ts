// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import rows from "../__fixtures__/service-memory.json";
import { memoryNote, sameMemory } from "./memoryFacts.ts";
import { isGreen, needsAttention, problemText } from "./serviceState.ts";

// Contract (#131): the fixture is the REAL `service_json` the runner sends for a service leaking
// past `memory_warn` and for a steady one (atlas-tui wire.rs writes it with UPDATE_FIXTURES=1),
// so a rename of any `memory_*` / `budget_state` key fails here, not in the user's panel.
const [api, web] = rows;

describe("memory keys as the runner really serializes them", () => {
	test("the leaking service carries rss, trend, budget and leak under snake_case keys", () => {
		expect(api.state).toBe("degraded");
		expect(typeof api.memory_rss).toBe("number");
		expect(api.memory_trend).toBe("rising");
		expect(api.memory_trend_mb_min).toBe(360);
		expect(api.budget_state).toBe("warn");
		expect(api.memory_leak).toMatch(/^leak suspected: \+\d+ MB over \d+ reloads$/);
	});

	test("a steady service says ok / flat and no leak, not missing keys", () => {
		expect(web.state).toBe("running");
		expect(web.budget_state).toBe("ok");
		expect(web.memory_trend).toBe("flat");
		expect(web.memory_leak).toBeNull();
		expect(web.memory_rss).toBeGreaterThan(0);
	});

	test("the degraded reason is the memory budget one, in the keys the card already reads", () => {
		expect(api.probe_error).toBe("using 1.3 GB, over memory_warn 1.0 GB");
		expect(problemText(api)).toBe(api.probe_error);
		expect(needsAttention(api)).toBe(true);
		expect(isGreen(api)).toBe(false);
		expect(needsAttention(web)).toBe(false);
	});
});

describe("the card's memory line", () => {
	test("a suspected leak wins over the budget and the trend", () => {
		const n = memoryNote(api);
		expect(n.tone).toBe("warn");
		expect(n.text).toBe(api.memory_leak);
	});

	test("over max_memory is an error, over memory_warn a warning, without a leak", () => {
		const base = { ...api, memory_leak: null };
		expect(memoryNote({ ...base, budget_state: "over" })).toMatchObject({
			tone: "error",
			text: "Over max_memory",
		});
		expect(memoryNote({ ...base, budget_state: "warn" })).toMatchObject({
			tone: "warn",
			text: "Over memory_warn",
		});
	});

	test("a rising service in budget says how fast, from the real trend", () => {
		const n = memoryNote({ ...api, memory_leak: null, budget_state: "ok" });
		expect(n.text).toBe("Rising +360 MB/min");
		expect(n.title).toContain("1360 MB");
	});

	test("a steady service, and a falling or unknown one, stay quiet", () => {
		expect(memoryNote(web)).toBeNull();
		expect(memoryNote({ ...web, memory_trend: "falling", memory_trend_mb_min: -30 })).toBeNull();
		expect(memoryNote({})).toBeNull();
		// A row from an older runner has none of the keys.
		expect(memoryNote({ state: "running" })).toBeNull();
	});
});

describe("row diff", () => {
	test("a fresh copy of the same facts is not a change (state events repeat every tick)", () => {
		expect(sameMemory(api, JSON.parse(JSON.stringify(api)))).toBe(true);
	});

	test("each fact changing is a change", () => {
		expect(sameMemory(api, { ...api, memory_rss: api.memory_rss + 1 })).toBe(false);
		expect(sameMemory(api, { ...api, memory_trend: "flat" })).toBe(false);
		expect(sameMemory(api, { ...api, memory_trend_mb_min: 1 })).toBe(false);
		expect(sameMemory(api, { ...api, budget_state: "over" })).toBe(false);
		expect(sameMemory(api, { ...api, memory_leak: null })).toBe(false);
	});

	test("an older runner's row (no keys) equals another such row", () => {
		expect(sameMemory({}, {})).toBe(true);
	});
});
