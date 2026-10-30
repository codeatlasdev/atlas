// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	EMPTY_SNAPSHOT,
	TRAY_MAX_HEIGHT,
	TRAY_MIN_HEIGHT,
	agentDoing,
	agentSections,
	agentWhere,
	ago,
	failedServices,
	groupProjects,
	headerSummary,
	projectSections,
	runningMatches,
	restartFailedCalls,
	searchOpensStopped,
	showSearch,
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
	parent: null,
	path: "",
	pinned: false,
	recentAt: 0,
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
	test("the header line pluralizes who needs you", () => {
		const a = (needsYou) => snap({ agents: { working: 0, needsYou, focusTab: null, list: [] } });
		expect(headerSummary(a(1)).text).toBe("1 needs you");
		expect(headerSummary(a(3)).text).toBe("3 need you");
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

// ---- Popover redesign: attention first, repos with nested worktrees, stopped behind a disclosure ----

const agent = (over = {}) => ({
	tabId: "t1",
	agent: "omp",
	project: "atlas",
	worktree: "main",
	state: "working",
	lastMessage: null,
	updatedAt: 0,
	...over,
});
const wt = (id, parent, branch, over = {}) =>
	proj(id, null, { id, name: "atlas", parent, branch, path: `/w/${id}`, ...over });

describe("agent sections", () => {
	test("needs-you agents (waiting or blocked), blocked first then oldest wait first; working agents newest first", () => {
		const list = [
			agent({ tabId: "w1", state: "working", updatedAt: 5 }),
			agent({ tabId: "n2", state: "waiting", updatedAt: 20 }),
			agent({ tabId: "b1", state: "blocked", updatedAt: 30 }),
			agent({ tabId: "n1", state: "waiting", updatedAt: 10 }),
			agent({ tabId: "w2", state: "working", updatedAt: 9 }),
			agent({ tabId: "d1", state: "done", updatedAt: 99 }),
			agent({ tabId: "i1", state: "idle", updatedAt: 99 }),
		];
		const { needsYou, working } = agentSections(list);
		expect(needsYou.map((a) => a.tabId)).toEqual(["b1", "n1", "n2"]);
		expect(working.map((a) => a.tabId)).toEqual(["w2", "w1"]);
	});
	test("done and idle agents are not news", () => {
		const s = agentSections([agent({ state: "done" }), agent({ state: "idle" })]);
		expect(s).toEqual({ needsYou: [], working: [] });
	});
	test("an agent row says where it runs: the worktree only when it is not the project's own", () => {
		expect(agentWhere(agent({ project: "api", worktree: "main" }))).toBe("api");
		expect(agentWhere(agent({ project: "api", worktree: "api" }))).toBe("api");
		expect(agentWhere(agent({ project: "api", worktree: "feat-login" }))).toBe("api · feat-login");
		expect(agentWhere(agent({ project: "", worktree: "feat-login" }))).toBe("feat-login");
	});
	test("what an agent is doing: last message, one line, else the state", () => {
		expect(agentDoing(agent({ lastMessage: "  Run the migration?\n\nyes/no " }))).toBe("Run the migration?");
		expect(agentDoing(agent({ state: "working" }))).toBe("Working");
		expect(agentDoing(agent({ state: "waiting" }))).toBe("Waiting for you");
		expect(agentDoing(agent({ state: "blocked" }))).toBe("Needs approval");
	});
});

describe("project groups", () => {
	test("worktrees nest under their repo; a worktree whose repo is missing becomes its own group", () => {
		const repo = proj("atlas", ["running"], { name: "atlas", parent: null, branch: "main" });
		const a = wt("atlas-fix", "atlas", "fix/tray");
		const orphan = wt("gone-wt", "gone", "x");
		const groups = groupProjects([a, repo, orphan]);
		expect(groups.map((g) => [g.repo.id, g.worktrees.map((w) => w.id)])).toEqual([
			["atlas", ["atlas-fix"]],
			["gone-wt", []],
		]);
	});
	test("no N identical rows: every worktree is labelled by its branch, else by the path tail, else its id", () => {
		const repo = proj("atlas", null, { parent: null, branch: "main" });
		const labels = groupProjects([
			repo,
			wt("a", "atlas", "feat/x"),
			wt("b", "atlas", null, { path: "/Users/me/code/atlas-hotfix" }),
			wt("c", "atlas", null, { path: "" }),
		])[0].worktrees.map((w) => [w.id, w.label]);
		expect(labels).toEqual([
			["b", "atlas-hotfix"],
			["c", "c"],
			["a", "feat/x"],
		]);
	});
	test("two worktrees on the same branch are told apart by their path", () => {
		const repo = proj("atlas", null, { parent: null, branch: "main" });
		const g = groupProjects([
			repo,
			wt("a", "atlas", "dev", { path: "/p/one/atlas" }),
			wt("b", "atlas", "dev", { path: "/p/two/atlas" }),
		])[0];
		expect(new Set(g.worktrees.map((w) => w.label)).size).toBe(2);
		expect(g.worktrees.map((w) => w.label)).toEqual(["dev · one/atlas", "dev · two/atlas"]);
	});
	test("a group is running when its repo or any worktree runs; running groups first, then recents, then name", () => {
		const mk = (name, running, recentAt = 0, extra = []) => [
			proj(name, running ? ["running"] : null, { parent: null, recentAt }),
			...extra,
		];
		const idleOld = mk("zz", false, 1);
		const idleNew = mk("aa", false, 9);
		const runWt = [
			...mk("mm", false, 0),
			wt("mm-wt", "mm", "b", { running: true, services: [svc("s", "running")] }),
		];
		const runOwn = mk("bb", true, 0);
		const order = groupProjects([...idleOld, ...idleNew, ...runWt, ...runOwn]).map((g) => g.repo.name);
		expect(order).toEqual(["bb", "mm", "aa", "zz"]);
	});
	test("pinned projects lead their section", () => {
		const gs = groupProjects([
			proj("b", null, { parent: null, pinned: true }),
			proj("a", null, { parent: null, recentAt: 99 }),
		]);
		expect(gs.map((g) => g.repo.name)).toEqual(["b", "a"]);
	});
	test("worktrees inside a group: running first, then recents, then label", () => {
		const repo = proj("atlas", null, { parent: null });
		const g = groupProjects([
			repo,
			wt("old", "atlas", "a-old", { recentAt: 1 }),
			wt("run", "atlas", "z-run", { running: true, services: [svc("s", "running")] }),
			wt("new", "atlas", "b-new", { recentAt: 9 }),
		])[0];
		expect(g.worktrees.map((w) => w.id)).toEqual(["run", "new", "old"]);
	});
});

describe("popover sections", () => {
	const gs = () =>
		groupProjects([
			proj("run", ["running"], { parent: null }),
			proj("idle1", null, { parent: null }),
			proj("idle2", null, { parent: null }),
		]);
	test("stopped groups go behind the disclosure and count what is inside; a running repo is not repeated here", () => {
		const s = projectSections(gs(), "");
		expect(s.running).toEqual([]);
		expect(s.stopped.map((g) => g.repo.name)).toEqual(["idle1", "idle2"]);
		expect(s.stoppedCount).toBe(2);
	});
	test("the stopped count counts worktrees too (a repo with 2 stopped worktrees is 3 rows)", () => {
		const g = groupProjects([proj("r", null, { parent: null }), wt("a", "r", "x"), wt("b", "r", "y")]);
		expect(projectSections(g, "").stoppedCount).toBe(3);
	});
	test("search matches name, branch, worktree label and path, case-insensitively, and keeps a group when a worktree matches", () => {
		const g = groupProjects([
			proj("atlas", null, { parent: null, path: "/w/atlas" }),
			wt("w1", "atlas", "feat/Tray"),
			wt("w2", "atlas", "fix/other"),
			proj("shop", null, { parent: null, path: "/w/shop" }),
		]);
		const s = projectSections(g, "  TRAY ");
		expect(s.stopped.map((x) => x.repo.name)).toEqual(["atlas"]);
		expect(s.stopped[0].worktrees.map((w) => w.id)).toEqual(["w1"]);
		expect(projectSections(g, "/w/shop").stopped.map((x) => x.repo.name)).toEqual(["shop"]);
		expect(projectSections(g, "nope")).toEqual({ running: [], stopped: [], stoppedCount: 0 });
	});
	test("a search opens the stopped section (a hit hidden behind a collapsed disclosure is a missed hit)", () => {
		expect(searchOpensStopped("")).toBe(false);
		expect(searchOpensStopped("  ")).toBe(false);
		expect(searchOpensStopped("a")).toBe(true);
	});
	test("search shows only when there are many projects to search", () => {
		expect(showSearch(7)).toBe(false);
		expect(showSearch(8)).toBe(true);
	});
});

describe("header summary", () => {
	test("one line: agents needing you, working, stacks up; zero parts are omitted", () => {
		const s = snap({
			agents: { working: 2, needsYou: 1, focusTab: null, list: [] },
			projects: [proj("a", ["running"]), proj("b", ["running"]), proj("c", null)],
		});
		expect(headerSummary(s)).toEqual({ tone: "warn", text: "1 needs you · 2 working" });
	});
	test("nothing going on", () => {
		expect(headerSummary(snap())).toEqual({ tone: "off", text: "All quiet" });
	});
	test("a failed service is red and named in the line before the counts", () => {
		const s = snap({ projects: [proj("a", ["running", "failed"])] });
		expect(headerSummary(s)).toEqual({ tone: "fail", text: "1 failed · 1 up" });
	});
	// Redesign review: the line sat beside four icon buttons and a Dev chip and was cut off with "…" in a 360 px popover.
	test("the busiest realistic line stays short enough for the header (≤ 28 characters)", () => {
		const s = snap({
			agents: { working: 12, needsYou: 12, focusTab: null, list: [] },
			projects: [proj("a", ["running", "failed"]), proj("b", ["running"])],
		});
		expect(headerSummary(s).text.length).toBeLessThanOrEqual(28);
	});
	test("worktrees count as stacks too", () => {
		const s = snap({
			projects: [
				proj("a", ["running"]),
				wt("w", "a", "x", { running: true, services: [svc("s", "running")] }),
			],
		});
		expect(headerSummary(s).text).toBe("2 up");
	});
});

describe("no project is listed twice", () => {
	// Redesign review: a running repo showed in "Running" and again, with its worktrees, in "Projects".
	test("the project list leaves out every row already shown as a running stack, and a group with nothing left", () => {
		const g = groupProjects([
			proj("atlas", ["running"], { parent: null }),
			wt("w-run", "atlas", "feat/a", { running: true, services: [svc("s", "running")] }),
			wt("w-idle", "atlas", "feat/b"),
			proj("shop", ["running"], { parent: null }),
		]);
		const s = projectSections(g, "");
		expect(s.running.map((x) => [x.repo.id, x.worktrees.map((w) => w.id)])).toEqual([["atlas", ["w-idle"]]]);
	});
	test("a running repo whose worktrees are all stopped keeps no empty group", () => {
		const g = groupProjects([proj("solo", ["running"], { parent: null })]);
		expect(projectSections(g, "").running).toEqual([]);
	});
	test("a stopped repo whose only worktree runs still lists the repo (it is not shown anywhere else)", () => {
		const g = groupProjects([
			proj("repo", null, { parent: null }),
			wt("w", "repo", "x", { running: true, services: [svc("s", "running")] }),
		]);
		const s = projectSections(g, "");
		expect(s.running.map((x) => [x.repo.id, x.worktrees.map((w) => w.id)])).toEqual([["repo", []]]);
	});
});

describe("same-named repos and search across sections", () => {
	// Redesign review: a clone of the same repo in another folder showed as a second plain "atlas" row.
	test("repos that share a name are told apart by their folder; unique names stay bare", () => {
		const gs = groupProjects([
			proj("a", null, { parent: null, name: "atlas", path: "/Users/me/code/atlas" }),
			proj("b", null, { parent: null, name: "atlas", path: "/Users/me/clones/atlas" }),
			proj("c", null, { parent: null, name: "shop", path: "/Users/me/code/shop" }),
		]);
		const byId = Object.fromEntries(gs.map((g) => [g.repo.id, g.repoLabel]));
		expect(byId).toEqual({ a: "atlas · code/atlas", b: "atlas · clones/atlas", c: "shop" });
	});
	// A search that only hits a running stack must say so, not "No project matches".
	test("runningMatches filters the running stacks by the same fields as the project search", () => {
		const a = proj("a", ["running"], { name: "api", branch: "feat/tray", path: "/p/api" });
		const b = proj("b", ["running"], { name: "web", branch: "main", path: "/p/web" });
		expect(runningMatches([a, b], " TRAY ").map((p) => p.id)).toEqual(["a"]);
		expect(runningMatches([a, b], "").map((p) => p.id)).toEqual(["a", "b"]);
		expect(runningMatches([a, b], "zzz")).toEqual([]);
	});
});
