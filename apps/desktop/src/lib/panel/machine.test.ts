// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../__fixtures__/machine-report.json";
import {
	agentRows,
	bytes,
	gauge,
	hygieneView,
	runTrend,
	serviceNote,
	type MachineReport,
} from "./machine.ts";

const report = fixture as MachineReport;

// Contract: __fixtures__/machine-report.json is the REAL serialization of what the `machine_report` command
// returns (crates/atlas-desktop/src/cmds/machine.rs regenerates it with UPDATE_FIXTURES=1). The view reads it as is.
describe("the real machine_report payload", () => {
	test("has the keys the view reads, in the shapes it expects", () => {
		for (const k of [
			"at",
			"cores",
			"load",
			"mem_available_pct",
			"mem_total_bytes",
			"gates",
			"heavy",
			"history",
		]) {
			expect(report[k], k).not.toBeUndefined();
		}
		expect(Object.keys(report.load).sort()).toEqual(["fifteen", "five", "one"]);
		expect(Object.keys(report.gates).sort()).toEqual(["holding", "max_load", "min_mem_pct"]);
		for (const s of report.services) {
			for (const k of ["project", "name", "state", "memory_rss", "memory_trend", "budget_state", "cpu"]) {
				expect(s[k], k).not.toBeUndefined();
			}
		}
		for (const t of report.terminals) {
			for (const k of ["pty", "pid", "cwd", "command", "mem", "cpu"]) expect(t[k], k).not.toBeUndefined();
		}
		// The queue is the heavy_status payload, untouched: the drawer's reader works on it.
		expect(report.heavy.slots).toBe(2);
		expect(report.heavy.running[0].state).toBe("running");
	});
});

describe("gauge", () => {
	test("load is a fraction of the cores and its threshold is where a heavy job waits", () => {
		const g = gauge.load(report);
		expect(g.value).toBeCloseTo(4.0 / 12);
		expect(g.limit).toBeCloseTo(9 / 12);
		expect(g.label).toBe("4.0 of 12 cores");
		expect(g.over).toBe(false);
	});

	test("load at or above the gate is over, memory under its floor is over", () => {
		expect(gauge.load({ ...report, load: { one: 1, five: 9, fifteen: 1 } }).over).toBe(true);
		expect(gauge.load({ ...report, load: { one: 1, five: 8.99, fifteen: 1 } }).over).toBe(false);
		const low = gauge.memory({ ...report, mem_available_pct: 19 });
		expect(low.over).toBe(true);
		expect(gauge.memory({ ...report, mem_available_pct: 20 }).over).toBe(false);
	});

	test("memory shows the USED share, with the free floor as the line", () => {
		const g = gauge.memory(report);
		expect(g.value).toBeCloseTo(0.58);
		expect(g.limit).toBeCloseTo(0.8);
		expect(g.label).toBe("42% free · 24 GB");
	});

	test("a gate that is off has no line, a reading the OS cannot give has no value", () => {
		const off = { ...report, gates: { ...report.gates, max_load: null, min_mem_pct: null } };
		expect(gauge.load(off).limit).toBeNull();
		expect(gauge.memory(off).limit).toBeNull();
		const blind = { ...report, load: null, mem_available_pct: null, mem_total_bytes: null };
		expect(gauge.load(blind)).toMatchObject({ value: null, over: false, label: "no reading" });
		expect(gauge.memory(blind)).toMatchObject({ value: null, over: false, label: "no reading" });
	});

	test("a load above the core count still draws (clamped) and a fraction never leaves 0..1", () => {
		expect(gauge.load({ ...report, load: { one: 0, five: 30, fifteen: 0 } }).value).toBe(1);
		expect(gauge.load({ ...report, cores: 0 }).value).toBe(1);
	});
});

describe("bytes", () => {
	test("MB under a gigabyte, GB with one decimal below ten", () => {
		expect([bytes(0), bytes(41943040), bytes(943718400), bytes(1426063360), bytes(25769803776)]).toEqual([
			"0 MB",
			"40 MB",
			"900 MB",
			"1.3 GB",
			"24 GB",
		]);
	});
});

describe("serviceNote", () => {
	test("leak beats budget beats a climbing trend; a flat service in budget says nothing", () => {
		const [api, docs, web] = report.services;
		expect(serviceNote(api)).toEqual({ tone: "warn", text: "Over memory_warn · rising +360 MB/min" });
		expect(serviceNote(docs)).toBeNull();
		expect(serviceNote(web)).toBeNull();
		expect(serviceNote({ ...api, memory_leak: "leak suspected: +660 MB over 11 reloads" }).text).toBe(
			"leak suspected: +660 MB over 11 reloads",
		);
		expect(serviceNote({ ...api, budget_state: "over" })).toEqual({
			tone: "error",
			text: "Over max_memory · rising +360 MB/min",
		});
		expect(serviceNote({ ...docs, memory_trend: "rising", memory_trend_mb_min: 40 })).toEqual({
			tone: "warn",
			text: "Rising +40 MB/min",
		});
	});
});

describe("agentRows", () => {
	const tabs = [
		{ id: "tab-1", pty: "p1", agent: "omp", label: "omp", worktreeId: "w1" },
		{ id: "tab-2", pty: "p2", label: "zsh", worktreeId: "w1" },
	];
	const status = { "tab-1": { state: "working", agent: "omp", model: "opus", prompt: "fix the flake" } };

	test("joins each terminal to its tab and agent status; an agent tab is named for the agent", () => {
		const rows = agentRows(report.terminals, tabs, status);
		expect(rows[0]).toMatchObject({
			pty: "p1",
			title: "omp",
			agent: "omp",
			state: "working",
			detail: "fix the flake",
			mem: 943718400,
			cpu: 85.5,
		});
		// A plain shell is listed too (it may run a build), without an agent or state.
		expect(rows[1]).toMatchObject({ pty: "p2", title: "zsh", agent: null, state: null });
	});

	test("agents come before plain shells, then by memory", () => {
		const rows = agentRows(
			[{ ...report.terminals[1], pty: "big-shell", mem: 5_000_000_000 }, ...report.terminals],
			[...tabs, { id: "tab-9", pty: "big-shell", label: "zsh", worktreeId: "w1" }],
			status,
		);
		expect(rows.map((r) => r.pty)).toEqual(["p1", "big-shell", "p2"]);
	});

	test("a terminal the app has no tab for (another window, a restored pty) still lists, by its command", () => {
		const [row] = agentRows([report.terminals[0]], [], {});
		expect(row).toMatchObject({ pty: "p1", title: "omp --auto-approve", agent: null, state: null });
	});
});

describe("runTrend", () => {
	test("last run against the earlier average: slower, faster or steady (10% dead band)", () => {
		expect(runTrend([100, 100, 100, 130])).toEqual({ kind: "slower", pct: 30 });
		expect(runTrend([100, 100, 100, 70])).toEqual({ kind: "faster", pct: 30 });
		expect(runTrend([100, 100, 100, 105])).toEqual({ kind: "steady", pct: 5 });
		expect(runTrend([100, 100, 100, 110])).toEqual({ kind: "steady", pct: 10 });
	});

	test("one run or an all-zero history has nothing to compare", () => {
		expect(runTrend([])).toBeNull();
		expect(runTrend([240])).toBeNull();
		expect(runTrend([0, 0, 5])).toBeNull();
	});

	test("the fixture's gate history reads as steady-ish: 270 against 270", () => {
		expect(runTrend(report.history[0].secs)).toEqual({ kind: "steady", pct: 0 });
	});
});

describe("hygieneView", () => {
	// #127 is not on main yet: the section is fed by a typed hook that says "not available".
	test("no report (the command does not exist yet, or it failed) is not available, never an error", () => {
		expect(hygieneView(null)).toEqual({ available: false });
	});

	test("a report summarises orphans and the disk the user could free", () => {
		const v = hygieneView({
			orphans: [
				{ pid: 1, start: 1, name: "chrome", session: "s", kind: "browser", label: null, project: "ui" },
				{ pid: 2, start: 1, name: "vite", session: "s", kind: "service", label: null, project: "ui" },
			],
			live: 5,
			projects: [
				{
					root: "/a",
					name: "a",
					target: 3 << 30,
					node_modules: 1 << 30,
					logs: 1 << 20,
					worktrees: [["/a-wt", 2 << 30]],
					indexed: [],
				},
			],
			dirs: [],
			freeable: { dirs_removed: 1, entries_removed: 4, bytes: 5 << 20 },
		});
		expect(v).toEqual({
			available: true,
			orphans: 2,
			live: 5,
			disk: [{ name: "a", bytes: (3 << 30) + (1 << 30) + (1 << 20) + (2 << 30) }],
			freeable: 5 << 20,
		});
	});
});
