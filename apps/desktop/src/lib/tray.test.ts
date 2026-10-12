// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	EMPTY_SNAPSHOT,
	TRAY_MAX_HEIGHT,
	TRAY_MIN_HEIGHT,
	agentsLine,
	ago,
	idleProjects,
	overall,
	recommend,
	runningCards,
	trayHeight,
	upLine,
} from "./tray.ts";

const svc = (name, state) => ({ name, state });
const proj = (name, states, over = {}) => ({
	id: name,
	name,
	hue: 10,
	icon: { kind: "monogram", text: "A" },
	running: states !== null,
	branch: null,
	services: (states ?? []).map((s, i) => svc(`s${i}`, s)),
	cpu: 0,
	mem: 0,
	cpuSeries: [],
	failed: null,
	...over,
});
const snap = (over = {}) => ({ ...EMPTY_SNAPSHOT, ...over });

describe("overall", () => {
	test("nothing running, also with only stopped projects", () => {
		expect(overall([])).toEqual({ tone: "off", label: "Nothing running" });
		expect(overall([proj("a", null)])).toEqual({ tone: "off", label: "Nothing running" });
	});
	test("counts failures across projects and beats starting", () => {
		const ps = [proj("a", ["running", "failed"]), proj("b", ["failed", "starting"])];
		expect(overall(ps)).toEqual({ tone: "fail", label: "2 services failed" });
		expect(overall([proj("a", ["failed", "starting"])]).label).toBe("1 service failed");
	});
	test("starting is a warning", () => {
		expect(overall([proj("a", ["running", "starting"])])).toEqual({
			tone: "warn",
			label: "1 service starting",
		});
	});
	test("healthy wording follows the number of stacks", () => {
		expect(overall([proj("a", ["running"])]).label).toBe("Stack healthy");
		expect(overall([proj("a", ["running"]), proj("b", ["running"]), proj("c", null)])).toEqual({
			tone: "ok",
			label: "All stacks healthy",
		});
	});
	test("a stopped service in a running stack is not healthy", () => {
		expect(overall([proj("a", ["running", "stopped"])]).tone).toBe("warn");
	});
});

describe("card ordering", () => {
	test("running only; failed, then starting, then healthy; ties by name", () => {
		const ps = [
			proj("zeta", ["running"]),
			proj("beta", ["running"]),
			proj("idle", null),
			proj("boot", ["starting"]),
			proj("broken", ["failed"]),
		];
		expect(runningCards(ps).map((p) => p.name)).toEqual(["broken", "boot", "beta", "zeta"]);
	});
	test("does not reorder its input", () => {
		const ps = [proj("b", ["running"]), proj("a", ["failed"])];
		runningCards(ps);
		expect(ps[0].name).toBe("b");
	});
	test("idle projects by name", () => {
		expect(
			idleProjects([proj("b", null), proj("a", null), proj("c", ["running"])]).map((p) => p.name),
		).toEqual(["a", "b"]);
	});
});

describe("recommend", () => {
	const NOW = 10_000_000;
	test("a fresh failure wins over an update and names the service", () => {
		const s = snap({
			projects: [proj("api-stack", ["failed"], { failed: { service: "api", at: NOW - 120_000 } })],
			update: { version: "0.6.0" },
		});
		const r = recommend(s, NOW);
		expect(r.title).toBe("api failed 2 min ago");
		expect(r.action).toEqual({ kind: "logs", id: "api-stack", service: "api" });
	});
	test("the most recent of several failures is shown", () => {
		const s = snap({
			projects: [
				proj("a", ["failed"], { failed: { service: "old", at: NOW - 600_000 } }),
				proj("b", ["failed"], { failed: { service: "new", at: NOW - 60_000 } }),
			],
		});
		expect(recommend(s, NOW).action.service).toBe("new");
	});
	test("a failure older than an hour, or of a stopped project, is ignored", () => {
		const old = proj("a", ["failed"], { failed: { service: "x", at: NOW - 2 * 3_600_000 } });
		const stopped = proj("b", null, { failed: { service: "y", at: NOW - 1000 } });
		expect(recommend(snap({ projects: [old, stopped] }), NOW)).toBeNull();
	});
	test("update, then outdated host, then nothing", () => {
		expect(recommend(snap({ update: { version: "1.2.3" } }), NOW).action).toEqual({ kind: "update" });
		expect(recommend(snap({ host: { status: "outdated", version: "0.5.0" } }), NOW).icon).toBe("host");
		expect(recommend(snap(), NOW)).toBeNull();
	});
});

describe("labels", () => {
	test("ago", () => {
		expect(ago(10_000)).toBe("just now");
		expect(ago(120_000)).toBe("2 min ago");
		expect(ago(3 * 3_600_000)).toBe("3 h ago");
		expect(ago(-5000)).toBe("just now");
	});
	test("upLine counts running services", () => {
		expect(upLine(proj("a", ["running", "failed", "running"]))).toBe("2/3 up");
	});
	test("agentsLine", () => {
		expect(agentsLine({ working: 0, needsYou: 0, focusTab: null })).toBe("All quiet");
		expect(agentsLine({ working: 2, needsYou: 1, focusTab: "t" })).toBe("1 needs you · 2 working");
		expect(agentsLine({ working: 0, needsYou: 3, focusTab: null })).toBe("3 need you");
	});
	test("trayHeight clamps", () => {
		expect(trayHeight(10)).toBe(TRAY_MIN_HEIGHT);
		expect(trayHeight(300.2)).toBe(301);
		expect(trayHeight(5000)).toBe(TRAY_MAX_HEIGHT);
	});
});
