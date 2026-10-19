// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	EMPTY_SNAPSHOT,
	TRAY_MAX_HEIGHT,
	TRAY_MIN_HEIGHT,
	agentsLine,
	ago,
	failedServices,
	restartFailedCalls,
	idleProjects,
	moveFocus,
	overall,
	projectHeadline,
	projectTone,
	recommend,
	runningCards,
	sparkPaths,
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
		const host = recommend(snap({ host: { status: "outdated", version: "0.5.0" } }), NOW);
		expect(host.icon).toBe("host");
		expect(host.title).toBe("Terminal host needs a restart");
		expect(host.action).toEqual({ kind: "host" });
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

// Incident (v1.5.0-alpha.7): the card said "4 services failed" in the header but never named them, and offered no
// way to act on them. The popover now names the failed services and restarts exactly those.
describe("failed services", () => {
	test("names only the failed services of a running stack, in service order", () => {
		const p = proj("ac", ["running", "failed", "starting", "failed"]);
		expect(failedServices(p)).toEqual(["s1", "s3"]);
	});
	test("a stopped project has none, even if its last states were failed", () => {
		expect(failedServices(proj("ac", null, { services: [svc("api", "failed")] }))).toEqual([]);
	});
});

describe("project headline", () => {
	test("healthy / starting / failed / idle", () => {
		expect(projectHeadline(proj("a", ["running", "running"]))).toEqual({
			state: "healthy",
			text: "All 2 services up",
		});
		expect(projectHeadline(proj("a", ["running", "starting"]))).toEqual({
			state: "starting",
			text: "1 starting",
		});
		expect(projectHeadline(proj("a", null))).toEqual({ state: "idle", text: "Stopped" });
	});
	test("failed names up to two services and counts the rest", () => {
		const names = (n) =>
			proj("a", ["running", ...Array(n).fill("failed")], {}).services.map((s, i) => ({
				...s,
				name: ["db", "api", "web", "docs"][i] ?? s.name,
			}));
		const mk = (n) => proj("a", ["running"], { services: names(n) });
		expect(projectHeadline(mk(1)).text).toBe("api failed");
		expect(projectHeadline(mk(2)).text).toBe("api, web failed");
		expect(projectHeadline(mk(3)).text).toBe("api, web +1 failed");
		expect(projectHeadline(mk(3)).state).toBe("failed");
	});
	test("failed beats starting in the same stack", () => {
		expect(projectHeadline(proj("a", ["starting", "failed"])).state).toBe("failed");
	});
	// Same incident as metrics' "no services" summary: a running stack with nothing declared read "All 0 services up".
	test("a running stack with no services says so", () => {
		expect(projectHeadline(proj("a", []))).toEqual({ state: "healthy", text: "No services" });
		expect(projectTone(proj("a", []))).toBe("ok");
	});
});

// Incident: the sparkline under the project card rendered as a clipped scribble (an ECharts canvas given a
// `var(--c)` / oklch colour it cannot parse, in a 22px box). The popover draws its own SVG path: pure, so testable.
describe("sparkline path", () => {
	test("maps the series into the box, newest on the right, never outside it", () => {
		const { line, area } = sparkPaths([0, 10, 5, 10], 100, 20, 2);
		const ys = [...line.matchAll(/[ML]\s*([\d.]+)[ ,]([\d.]+)/g)].map((m) => [Number(m[1]), Number(m[2])]);
		expect(ys.length).toBe(4);
		expect(ys[0][0]).toBe(0);
		expect(ys[3][0]).toBe(100);
		for (const [x, y] of ys) {
			expect(x).toBeGreaterThanOrEqual(0);
			expect(x).toBeLessThanOrEqual(100);
			expect(y).toBeGreaterThanOrEqual(2);
			expect(y).toBeLessThanOrEqual(18);
		}
		expect(ys[1][1]).toBe(2); // the peak touches the top padding
		expect(ys[0][1]).toBe(18); // zero sits on the bottom padding
		expect(area.endsWith("Z")).toBe(true);
	});
	test("a flat or empty series is a flat baseline, not NaN or a crash", () => {
		for (const data of [[], [0], [0, 0, 0], [7, 7, 7]]) {
			const { line, area } = sparkPaths(data, 100, 20, 2);
			expect(line).not.toMatch(/NaN|Infinity/);
			expect(area).not.toMatch(/NaN|Infinity/);
		}
		expect(sparkPaths([], 100, 20, 2).line).toBe("");
	});
	test("a single point is drawn as a segment, so a fresh stack still shows something", () => {
		expect(sparkPaths([5], 100, 20, 2).line).toMatch(/^M/);
	});
});

describe("keyboard focus", () => {
	test("arrows move through the items and stop at the ends; Home/End jump", () => {
		expect(moveFocus(3, -1, "ArrowDown")).toBe(0);
		expect(moveFocus(3, 0, "ArrowDown")).toBe(1);
		expect(moveFocus(3, 2, "ArrowDown")).toBe(2);
		expect(moveFocus(3, 0, "ArrowUp")).toBe(0);
		expect(moveFocus(3, 2, "ArrowUp")).toBe(1);
		expect(moveFocus(3, 1, "Home")).toBe(0);
		expect(moveFocus(3, 1, "End")).toBe(2);
	});
	test("nothing to focus, or an unrelated key, changes nothing", () => {
		expect(moveFocus(0, -1, "ArrowDown")).toBe(-1);
		expect(moveFocus(3, 1, "a")).toBe(1);
	});
});

// Incident: the project list was cut off at the bottom with no scroll. The window is capped by the SCREEN too, not
// only by a constant: on a short display a 560px popover would hang off the bottom edge.
describe("trayHeight on a short screen", () => {
	test("never taller than the room the screen leaves under the menu bar", () => {
		expect(trayHeight(5000, 400)).toBe(400);
		expect(trayHeight(5000, 900)).toBe(TRAY_MAX_HEIGHT);
		expect(trayHeight(300, 900)).toBe(300);
		expect(trayHeight(10, 900)).toBe(TRAY_MIN_HEIGHT);
	});
	test("a screen smaller than the minimum wins over the minimum", () => {
		expect(trayHeight(5000, 150)).toBe(150);
	});
});

// "Restart failed" must restart exactly the failed services through the same `service_cmd` the main window uses
// (cmds/stack.rs accepts start_service | stop_service | restart_service), never the whole stack.
describe("restart failed", () => {
	test("one restart_service call per failed service of that project, nothing else", () => {
		const p = proj("ac", ["running", "failed", "starting", "failed"]);
		expect(restartFailedCalls(p)).toEqual([
			{ id: "ac", cmd: "restart_service", service: "s1" },
			{ id: "ac", cmd: "restart_service", service: "s3" },
		]);
	});
	test("nothing failed, or the project stopped: no calls", () => {
		expect(restartFailedCalls(proj("ac", ["running", "starting"]))).toEqual([]);
		expect(restartFailedCalls(proj("ac", null, { services: [svc("api", "failed")] }))).toEqual([]);
	});
});
