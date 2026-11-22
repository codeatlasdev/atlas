// scripts/linux-idle-bench.ts turns /proc samples into "this state costs X % of a core". A bench that mis-attributes CPU
// (a new process counted as if it had run all along, ticks of one process credited to another, a gone process dropped) would
// make every later "this fix saved N %" claim false, so the arithmetic is pinned here against hand-computed numbers.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { cpuWindow, fixturePorts, fixtureYaml, GATE_BUDGET, judgeGate, judgeWindow, reservePorts, roleOf, type Sample } from "./linux-idle-bench.ts";

const sample = (pid: number, role: string, ticks: number, o: Partial<Sample> = {}): Sample => ({ pid, role, comm: role, ticks, rssKb: 100 * 1024, pssKb: 60 * 1024, swapKb: 0, threads: [], ...o });

describe("cpuWindow", () => {
	test("100 ticks (1 s of CPU) over a 10 s window is 10 % of one core, summed per role", () => {
		const w = cpuWindow("x", 10, [sample(1, "app", 0), sample(2, "webprocess", 500), sample(3, "webprocess", 0)], [sample(1, "app", 100), sample(2, "webprocess", 650), sample(3, "webprocess", 50)]);
		expect(w.cpuPct).toEqual({ app: 10, webprocess: 20 });
		expect(w.totalCpuPct).toBe(30);
	});

	test("a process that appeared during the window has no earlier ticks to subtract: it counts as zero, not as its whole lifetime", () => {
		// A terminal's shell started 2 s ago with 200 ticks of startup work: that is not idle cost of the state.
		const w = cpuWindow("x", 10, [sample(1, "app", 0)], [sample(1, "app", 10), sample(9, "bash", 200)]);
		expect(w.cpuPct.bash).toBe(0);
		expect(w.totalCpuPct).toBe(1);
	});

	test("a process that left is not reported", () => {
		const w = cpuWindow("x", 10, [sample(1, "app", 0), sample(2, "bash", 0)], [sample(1, "app", 10)]);
		expect(w.processes.map((p) => p.pid)).toEqual([1]);
	});

	test("memory is the sum of the processes, swapped-out PSS reported on its own", () => {
		const w = cpuWindow("x", 10, [], [sample(1, "app", 0, { pssKb: 1024 * 100, rssKb: 1024 * 150, swapKb: 1024 * 30 }), sample(2, "webprocess", 0, { pssKb: 1024 * 200, rssKb: 1024 * 250 })]);
		expect([w.pssMb, w.rssMb, w.swapMb]).toEqual([300, 400, 30]);
	});

	test("threads: the busiest first, wakeups per second from context-switch deltas", () => {
		const t = (tid: number, name: string, ticks: number, switches: number) => ({ tid, name, ticks, switches });
		const before = sample(1, "app", 0, { threads: [t(1, "main", 0, 0), t(2, "tokio", 0, 0), t(3, "idle", 0, 0)] });
		const after = sample(1, "app", 150, { threads: [t(1, "main", 50, 2000), t(2, "tokio", 100, 100), t(3, "idle", 0, 10)] });
		const [p] = cpuWindow("x", 10, [before], [after]).processes;
		expect(p.threads.map((x) => [x.name, x.cpuPct, x.switchesPerS])).toEqual([["tokio", 10, 10], ["main", 5, 200], ["idle", 0, 1]]);
		expect(p.switchesPerS).toBe(211);
	});
});

describe("roleOf", () => {
	test("WebKit's helper processes keep their 15-character kernel names", () => {
		expect(roleOf("WebKitWebProces", "")).toBe("webprocess");
		expect(roleOf("WebKitNetworkPr", "")).toBe("network");
		expect(roleOf("WebKitGPUProces", "")).toBe("gpuprocess");
		expect(roleOf("atlas-desktop", "")).toBe("app");
	});
	test("only `atlas hostd` is hostd: another atlas process is itself", () => {
		expect(roleOf("atlas", "/x/atlas hostd --dir /y")).toBe("hostd");
		expect(roleOf("atlas", "/x/atlas dev")).toBe("atlas");
	});
});

describe("judge", () => {
	const w = cpuWindow("workspace", 30, [sample(1, "app", 0)], [sample(1, "app", 300)]);
	test("a window over its budget fails and names the state and the split", () => {
		expect(judgeWindow(w, 5)[0]).toMatch(/workspace: 10% of a core over 30 s > 5%.*"app":10/);
	});
	test("a window at or under budget passes", () => {
		expect(judgeWindow(w, 10)).toEqual([]);
	});
	test("--judge re-judges a saved file by label and exits 1 over budget, with no app", () => {
		const f = join(mkdtempSync(join(tmpdir(), "idle-bench-")), "r.jsonl");
		writeFileSync(f, `${JSON.stringify(w)}\n`);
		const run = (b: string) => Bun.spawnSync(["bun", join(import.meta.dir, "linux-idle-bench.ts"), "--judge", f, `workspace=${b}`], { stderr: "pipe" });
		expect(run("5").exitCode).toBe(1);
		expect(run("5").stderr.toString()).toContain("workspace");
		expect(run("20").exitCode).toBe(0);
	});
});

// CI gate (#289): "Desktop opens (Linux)" fails when a state that is idle on main starts to cost CPU. The danger is a gate that
// passes because it measured nothing, or one that judges a state against another's budget.
describe("judgeGate", () => {
	const win = (scenario: string, ticks: number) => cpuWindow(`ci/${scenario}`, 30, [sample(1, "app", 0)], [sample(1, "app", ticks)]);
	const all = (ticks: number) => Object.keys(GATE_BUDGET).map((s) => win(s, ticks));
	test("every gated state inside its budget passes", () => {
		expect(judgeGate(all(30))).toEqual([]); // 1 % of a core
	});
	test("a gated state over its own budget fails and names the state", () => {
		const bad = judgeGate([...all(30).filter((w) => !w.label.endsWith("/workspace")), win("workspace", 3000)]);
		expect(bad).toHaveLength(1);
		expect(bad[0]).toMatch(/ci\/workspace: 100% of a core over 30 s > \d+%/);
	});
	test("a gated state that was never measured fails: nothing measured is not a pass", () => {
		expect(judgeGate([])).toEqual(Object.keys(GATE_BUDGET).map((s) => `${s}: no window measured`));
		expect(judgeGate(all(30).slice(1))).toEqual([`${Object.keys(GATE_BUDGET)[0]}: no window measured`]);
	});
	test("states that are not gated (still costly on main) are not judged", () => {
		expect(judgeGate([...all(30), win("stack-running", 3000)])).toEqual([]);
	});
	test("every window of a gated state is judged, not just the first (a retried scenario)", () => {
		expect(judgeGate([win("welcome", 30), win("welcome", 3000), ...all(30).slice(1)])).toHaveLength(1);
	});
	test("--judge --gate exits 1 on a missing state and 0 on a clean report, with no app", () => {
		const f = join(mkdtempSync(join(tmpdir(), "idle-gate-")), "r.jsonl");
		const run = () => Bun.spawnSync(["bun", join(import.meta.dir, "linux-idle-bench.ts"), "--judge", f, "--gate"], { stderr: "pipe" });
		writeFileSync(f, `${JSON.stringify(win("welcome", 30))}\n`);
		expect(run().exitCode).toBe(1);
		expect(run().stderr.toString()).toContain("workspace: no window measured");
		writeFileSync(f, `${all(30).map((w) => JSON.stringify(w)).join("\n")}\n`);
		expect(run().exitCode).toBe(0);
	});
});

// Fixed ports 21001/21002 let two benches on one machine (two agents) start each other's services and measure the wrong stack.
describe("fixture ports", () => {
	test("two concurrent reservations never share a port, and none is listening", async () => {
		const a = reservePorts();
		const b = reservePorts();
		const all = [...a.ports, ...b.ports];
		expect(new Set(all).size).toBe(4);
		for (const p of all) expect(p).toBeGreaterThan(1023);
		a.release();
		b.release();
		// released: a service can now bind them
		const l = Bun.listen({ hostname: "127.0.0.1", port: a.ports[0]!, socket: { data() {} } });
		l.stop(true);
	});
	test("the yaml carries exactly the ports it was given, for the command and for the readiness check, and they read back", () => {
		const y = fixtureYaml([31001, 31002]);
		expect(y).toContain("python3 -m http.server 31001 --bind 127.0.0.1");
		expect(y).toContain("python3 -m http.server 31002 --bind 127.0.0.1");
		expect(fixturePorts(y)).toEqual([31001, 31002]);
		expect(y).not.toMatch(/2100[12]/);
	});
});
