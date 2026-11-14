// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../__fixtures__/service-port-conflict.json";
import { sameConflict } from "./portConflict.ts";

// Contract (#133): the fixture is the REAL `service_json` the runner sends (atlas-tui wire.rs
// writes it with UPDATE_FIXTURES=1), so a rename of `port_conflict` or of its keys fails here.
describe("port_conflict as the runner really serializes it", () => {
	test("the row carries pid, cmd and cwd under snake_case `port_conflict`", () => {
		const c = fixture.port_conflict;
		expect(typeof c.pid).toBe("number");
		expect(typeof c.cmd).toBe("string");
		expect(typeof c.cwd).toBe("string");
		expect(Object.keys(c).sort()).toEqual(["cmd", "cwd", "pid"]);
	});

	test("the row's note tells the user what to do, and the card shows the note as its failure reason", () => {
		expect(fixture.state).toBe("failed");
		expect(fixture.note).toContain(`held by PID ${fixture.port_conflict.pid}`);
		expect(fixture.note).toMatch(/stop it yourself \(.+\) or change the service port$/);
	});

	test("a fresh object with the same holder is not a change (state events repeat every tick)", () => {
		const again = JSON.parse(JSON.stringify(fixture.port_conflict));
		expect(again).not.toBe(fixture.port_conflict);
		expect(sameConflict(fixture.port_conflict, again)).toBe(true);
	});

	test("another holder, a lost cwd, or a cleared conflict is a change", () => {
		const c = fixture.port_conflict;
		expect(sameConflict(c, { ...c, pid: c.pid + 1 })).toBe(false);
		expect(sameConflict(c, { ...c, cmd: "bun" })).toBe(false);
		expect(sameConflict(c, { ...c, cwd: null })).toBe(false);
		expect(sameConflict(c, null)).toBe(false);
		expect(sameConflict(null, c)).toBe(false);
		expect(sameConflict(null, undefined)).toBe(true);
	});
});
