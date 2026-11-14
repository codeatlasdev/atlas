// @ts-nocheck bun test runtime types are not part of the app tsconfig
// Contract: Atlas Desktop reads the paused state of a crew server from the real `crewmate_list` reply and the real
// `crew_paused` push event. Both fixtures are written by the Rust serializer
// (UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::pause), never by hand: a hand-written mock hid a
// snake_case field that crashed every real build (languageLabels, 1.3.0).
// Crew review P0, PR #106 comment 6874910355: there was no kill switch for a crew server.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import event from "../../__fixtures__/crew_paused_event.json" with { type: "json" };
import { anyPaused, parseCrewList, pausedFromEvent } from "./pause.ts";

const list = JSON.parse(
	readFileSync(join(import.meta.dir, "../../../../mobile/test/fixtures/crewmate_list.json"), "utf8"),
);

describe("parseCrewList: the real crewmate_list reply", () => {
	test("the fixture carries a boolean `paused` next to `crewmates`", () => {
		expect(typeof list.paused).toBe("boolean");
		expect(Array.isArray(list.crewmates)).toBe(true);
	});

	test("reads the crewmates and the paused flag of the real reply", () => {
		const r = parseCrewList(list);
		expect(r.crewmates.map((c) => c.id)).toEqual(list.crewmates.map((c) => c.id));
		expect(r.paused).toBe(list.paused);
	});

	test("a paused reply reads as paused, an unpaused one does not", () => {
		expect(parseCrewList({ ...list, paused: true }).paused).toBe(true);
		expect(parseCrewList({ ...list, paused: false }).paused).toBe(false);
	});

	// An older crew server has no field: it must read as running, never as paused (a "Paused" banner on a server
	// that cannot pause would offer a Resume that fails).
	test("a reply without the field, or with a non-boolean, is not paused", () => {
		const { paused: _, ...old } = list;
		expect(parseCrewList(old).paused).toBe(false);
		expect(parseCrewList({ ...list, paused: "true" }).paused).toBe(false);
		expect(parseCrewList({ ...list, paused: 1 }).paused).toBe(false);
	});

	test("converts last.ts to milliseconds like the rest of the store", () => {
		const withLast = list.crewmates.find((c) => c.last);
		const got = parseCrewList(list).crewmates.find((c) => c.id === withLast.id);
		expect(got.last.ts).toBe(withLast.last.ts * 1000);
	});
});

describe("pausedFromEvent: the real crew_paused event", () => {
	test("the fixture is the agreed shape", () => {
		expect(event.ev).toBe("crew_paused");
		expect(typeof event.paused).toBe("boolean");
	});

	test("reads the flag of the real event", () => {
		expect(pausedFromEvent(event)).toBe(event.paused);
		expect(pausedFromEvent({ ev: "crew_paused", paused: true })).toBe(true);
		expect(pausedFromEvent({ ev: "crew_paused", paused: false })).toBe(false);
	});

	test("anything else is not a pause change", () => {
		expect(pausedFromEvent({ ev: "crewmate_task", paused: true })).toBeNull();
		expect(pausedFromEvent({ ev: "crew_paused" })).toBeNull();
		expect(pausedFromEvent({ ev: "crew_paused", paused: "yes" })).toBeNull();
		expect(pausedFromEvent(null)).toBeNull();
	});
});

describe("anyPaused: the workspace banner and the sidebar button", () => {
	test("is true as soon as one server is paused, false for none", () => {
		expect(anyPaused([{ paused: false }, { paused: false }])).toBe(false);
		expect(anyPaused([{ paused: false }, { paused: true }])).toBe(true);
		expect(anyPaused([])).toBe(false);
		expect(anyPaused([{}])).toBe(false);
	});
});
