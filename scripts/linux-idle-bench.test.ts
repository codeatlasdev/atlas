// scripts/linux-idle-bench.ts turns /proc samples into "this state costs X % of a core". A bench that mis-attributes CPU
// (a new process counted as if it had run all along, ticks of one process credited to another, a gone process dropped) would
// make every later "this fix saved N %" claim false, so the arithmetic is pinned here against hand-computed numbers.
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SCENARIOS, counterLive, cpuWindow, waitForLoad, fixturePorts, fixtureYaml, GATE_BUDGET, judgeGate, judgeWindow, levelLive, reservePorts, roleOf, type Sample } from "./linux-idle-bench.ts";

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
	test("the gate names only real scenarios, and the state that opens the crew rail runs last", () => {
		const names = Object.keys(GATE_BUDGET);
		for (const n of names) expect(SCENARIOS as readonly string[]).toContain(n);
		expect(names.at(-1)).toBe("crew-unfocused");
	});
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
		expect(judgeGate([...all(30), win("list-scroll", 3000)])).toEqual([]);
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

// Incident (2026-11-22, redo of the compositing matrix): `unfocused-chat-stream` read 1-2 % in some runs and 22-60 % in others
// because the chat sometimes never streamed; the low rows looked like a win for the setting under test. A run in which the
// state did not happen must fail, not report a low number.
describe("liveness: a state that did not happen is not a result", () => {
	test("a counter must grow at the stated rate over the window, and a stalled one fails with the numbers", () => {
		expect(counterLive("lines", 100, 400, 30, 10).ok).toBe(true); // exactly 10/s
		const dead = counterLive("lines", 100, 100, 30, 10);
		expect(dead.ok).toBe(false);
		expect(dead.why).toMatch(/lines grew by 0 in 30 s, needs at least 300/);
		expect(counterLive("lines", 100, 399, 30, 10).ok).toBe(false); // one short
	});
	test("a level must hold at both ends: services up at the start and gone at the end is not a running stack", () => {
		expect(levelLive("services", 2, 2, 2).ok).toBe(true);
		expect(levelLive("services", 2, 1, 2).ok).toBe(false);
		expect(levelLive("services", 0, 2, 2).ok).toBe(false);
	});
	test("the gate fails a window whose state did not happen even when its CPU is under budget", () => {
		const w = { ...cpuWindow("ci/welcome", 30, [sample(1, "app", 0)], [sample(1, "app", 3)]), live: counterLive("x", 0, 0, 30, 1) };
		const others = Object.keys(GATE_BUDGET).filter((s) => s !== "welcome").map((s) => cpuWindow(`ci/${s}`, 30, [sample(1, "app", 0)], [sample(1, "app", 3)]));
		expect(judgeGate([w, ...others])).toEqual([expect.stringContaining("ci/welcome: x grew by 0")]);
	});
	test("the real fake engine counts what it sends, and stops counting when its stream file goes away", async () => {
		const dir = mkdtempSync(join(tmpdir(), "fake-omp-"));
		const stream = join(dir, "stream");
		writeFileSync(stream, "");
		const p = Bun.spawn(["python3", join(import.meta.dir, "fixtures/linux-idle/fake-omp.py"), "--mode", "rpc-ui", "--stream-file", stream], { stdout: "ignore", stdin: "pipe" });
		await Bun.sleep(2500);
		const n = Number(await Bun.file(`${stream}.sent`).text());
		p.kill();
		// 30 updates/s: 2.5 s is ~75; the counter is written every 30 lines, so at least 30 and a multiple of 30
		expect(n).toBeGreaterThanOrEqual(30);
		expect(n % 30).toBe(0);
	});
});

// A bench run while other agents build measures their load, not the app's (the 2026-11-22 matrix had rows at load 5-13).
describe("waitForLoad", () => {
	const clock = (loads: number[]) => {
		let t = 0, i = 0;
		return { slept: [] as number[], load: () => loads[Math.min(i++, loads.length - 1)]!, sleep: async function (this: { slept: number[] }, ms: number) { t += ms; this.slept.push(ms); }, now: () => t };
	};
	const run = (c: ReturnType<typeof clock>, max = 2, timeoutMs = 60_000) => waitForLoad({ max, pollMs: 10_000, timeoutMs, load: c.load, sleep: (ms) => c.sleep.call(c, ms), now: c.now });
	test("an idle machine starts at once and reports the load it started at", async () => {
		const c = clock([0.4]);
		expect(await run(c)).toBe(0.4);
		expect(c.slept).toEqual([]);
	});
	test("above the limit it polls every 10 s until the load drops under it", async () => {
		const c = clock([5, 3.2, 2, 1.9]); // 2.0 is not under 2
		expect(await run(c)).toBe(1.9);
		expect(c.slept).toEqual([10_000, 10_000, 10_000]);
	});
	test("at the timeout it throws and says the load: the caller exits with no row", async () => {
		const c = clock([8]);
		await expect(run(c, 2, 30_000)).rejects.toThrow(/load stayed >= 2 for 30 s \(last 8\)/);
	});
	test.skipIf(!existsSync("/proc/loadavg"))("the CLI exits 1 with no row when the load never drops (limit below any real load)", () => {
		const out = join(mkdtempSync(join(tmpdir(), "idle-load-")), "r.jsonl");
		const r = Bun.spawnSync(["bun", join(import.meta.dir, "linux-idle-bench.ts"), "--max-load", "0.000001", "--load-timeout", "0", "--sock", "/nonexistent", "--out", out], { stderr: "pipe" });
		expect(r.exitCode).toBe(1);
		expect(r.stderr.toString()).toContain("no measurement taken");
		expect(Bun.file(out).size).toBe(0);
	});
});
