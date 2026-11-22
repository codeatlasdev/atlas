// #284: the Linux startup bench is only worth having if it fails when the app does not open or a number is over budget,
// and if it reads the producer's real output (boot.rs's trace format, the rc-file probe log). `--judge` re-judges a saved
// report without an app; the launch tests run the real script on a private sway when the machine has one.
import { describe, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BUDGET, collectEvidence, countProbes, evidenceLine, explainMissingReady, garbledLines, judge, parseTrace, pct, roleOf, summarize, type Idle, type Report, type Run } from "./linux-startup-bench.ts";

const root = join(import.meta.dir, "..");
const script = join(import.meta.dir, "linux-startup-bench.ts");
const dir = mkdtempSync(join(tmpdir(), "startup-bench-"));

const run = (o: Partial<Run> = {}): Run => ({
	kind: "warm",
	marks: {},
	spawnToWebProcessMs: 300,
	spawnToWindowMs: 1200,
	readyMs: 1400,
	shownBy: "ready",
	loginShells: { app: 1, hostd: 1 },
	...o,
});
const idle = (o: Partial<Idle> = {}): Idle => ({
	settleS: 30,
	cpuS: 60,
	processes: [],
	rssMb: 900,
	pssMb: 500,
	swapMb: 0,
	dominant: { role: "webprocess", pid: 1, pssMb: 180, share: 0.36 },
	cpuPct: { total: 0.8, byRole: {} },
	pssMbAfter: 500,
	paint: { colours: 400, topPermille: 600 },
	...o,
});
const good = (): Report => ({ runs: [run({ kind: "cold", readyMs: 2000 }), run(), run({ readyMs: 1500 })], idle: idle() });

describe("parseTrace reads what boot.rs writes", () => {
	// The real format of `boot::mark`: `{:.1} {name}`; the frontend's marks arrive as `js:<name>`; page:Started repeats.
	const trace = "12.3 main\n40.0 window:created\n880.5 js:ready:gate\n881.0 show:ready\n900.0 page:Started\n950.2 page:Started\n";
	test("keeps the first time of a mark that repeats", () => {
		expect(parseTrace(trace)).toEqual({ main: 12.3, "window:created": 40, "js:ready:gate": 880.5, "show:ready": 881, "page:Started": 900 });
	});
});

// #591: one warm start in ~300 "never reported app_ready" and the CI log held nothing to tell a page that never booted from
// a trace mark that was lost. The bench now says which, from the trace alone.
describe("why app_ready is missing", () => {
	const trace = (names: string[]) => parseTrace(names.map((n, i) => `${10 + i}.0 ${n}`).join("\n"));
	const base = ["main", "window:created", "page:Started", "page:Finished", "js:layout:mounted", "js:session:start", "js:theme:init"];

	test("names the last page stage reached and the ones missing: here the workspace never loaded", () => {
		const why = explainMissingReady(trace([...base, "js:theme:ready", "js:projects:loaded", "show:deadline"]), [], null, 90);
		expect(why).toContain("no js:ready:gate within 90 s");
		expect(why).toContain("last page stage reached: projects:loaded");
		expect(why).toContain("never reached: workspace:loaded, ready:gate");
		expect(why).toContain("shown by the 1 s deadline");
	});

	test("a Rust-side show:ready without the page's mark is a lost mark, not a page that never booted", () => {
		const why = explainMissingReady(trace([...base, "show:ready"]), ["12.3 js:ready:gateshow:ready"], null, 90);
		expect(why).toContain("Rust did get app_ready");
		expect(why).toContain("only the page's trace mark is missing");
		expect(why).toContain("1 unreadable trace line(s)");
	});

	test("a page that never loaded, a page that loaded and ran nothing, and an app that exited are told apart", () => {
		expect(explainMissingReady(trace(["main", "window:created"]), [], null, 5)).toContain("never loaded");
		expect(explainMissingReady(trace(["main", "page:Started", "page:Finished"]), [], null, 5)).toContain("no startup stage");
		const gone = explainMissingReady(trace(["main"]), [], 134, 5);
		expect(gone).toContain("exited with status 134");
		expect(gone).toContain("never shown");
	});

	// The same bytes boot.rs's `append_mark` once produced when two threads wrote at the same time (its own test pins the writer).
	test("garbledLines finds interleaved and cut lines and ignores whole ones", () => {
		const text = "5.0 main\n100.0100.1  js:ready:gateshow:ready\n\n900.0 page:Started\n77.";
		expect(garbledLines(text)).toEqual(["100.0100.1  js:ready:gateshow:ready", "77."]);
		expect(garbledLines("5.0 main\n6.5 js:ready:gate\n")).toEqual([]);
	});
});

describe("countProbes counts login shells by who started them", () => {
	// #285: the app probed twice and hostd once. The rc file logs `<parent pid> <parent comm>`.
	test("app, hostd and anything else are told apart", () => {
		expect(countProbes("100 atlas-desktop\n100 atlas-desktop\n200 atlas\n300 bash\n")).toEqual({ app: 2, hostd: 1, bash: 1 });
	});
	test("an empty log is no probe", () => {
		expect(countProbes("")).toEqual({});
	});
});

describe("roleOf", () => {
	test("hostd is the atlas binary running `hostd`, not any atlas process", () => {
		expect(roleOf("atlas", "/x/atlas hostd")).toBe("hostd");
		expect(roleOf("atlas", "/x/atlas doctor")).toBe("atlas");
	});
	test("WebKit's processes are named", () => {
		expect(roleOf("WebKitWebProces", "")).toBe("webprocess");
		expect(roleOf("WebKitNetworkPr", "")).toBe("network");
		expect(roleOf("atlas-desktop", "")).toBe("app");
	});
});

test("percentiles interpolate and never invent a value above the max", () => {
	expect(pct([10, 20, 30, 40], 0.5)).toBe(25);
	expect(pct([5], 0.95)).toBe(5);
	expect(pct([1, 2, 3, 4, 100], 0.95)).toBeLessThanOrEqual(100);
});

describe("judge", () => {
	test("a report inside the budget passes", () => {
		expect(judge(good())).toEqual([]);
	});

	// Acceptance of #284: the script exits non-zero if app_ready never arrives.
	test("a run whose app_ready never arrived fails, however good the rest is", () => {
		const r = good();
		r.runs[1] = run({ readyMs: null, shownBy: "deadline" });
		expect(judge(r).join("\n")).toContain("run 2 (warm): app_ready never arrived");
	});

	test("no run at all is a failure, not a pass", () => {
		expect(judge({ runs: [], idle: null })).toEqual(["no run was measured"]);
	});

	test("the first launch and the warm p95 each have a budget", () => {
		const slowCold = good();
		slowCold.runs[0] = run({ kind: "cold", readyMs: BUDGET.coldReadyMs + 1 });
		expect(judge(slowCold).join()).toContain("cold start");
		const slowWarm = good();
		slowWarm.runs.push(...Array.from({ length: 5 }, () => run({ readyMs: BUDGET.warmReadyP95Ms + 500 })));
		expect(judge(slowWarm).join()).toContain("warm start");
	});

	test("the cold run does not count towards the warm p95", () => {
		const r = good();
		r.runs[0] = run({ kind: "cold", readyMs: BUDGET.coldReadyMs - 1 });
		expect(summarize(r).warmReady?.max).toBe(1500);
	});

	test("memory is judged in total and per process, and the failure names the process", () => {
		const total = judge({ ...good(), idle: idle({ pssMb: BUDGET.idlePssMb + 1 }) }).join();
		expect(total).toContain("idle PSS");
		expect(total).toContain("webprocess");
		const one = judge({ ...good(), idle: idle({ dominant: { role: "app", pid: 7, pssMb: BUDGET.largestPssMb + 1, share: 0.9 } }) }).join();
		expect(one).toContain("app (pid 7)");
	});

	test("idle CPU over budget fails; a skipped idle window (0 s) is not judged", () => {
		expect(judge({ ...good(), idle: idle({ cpuPct: { total: BUDGET.idleCpuPct + 1, byRole: {} } }) }).join()).toContain("idle CPU");
		expect(judge({ ...good(), idle: idle({ cpuS: 0, cpuPct: { total: 99, byRole: {} } }) })).toEqual([]);
	});

	// A crashed page leaves one flat colour; the number of colours alone passes a gradient, the share alone passes a
	// two-colour error page: both are checked (same rule as linux-launch-check.sh).
	test("a flat page is not a pass", () => {
		expect(judge({ ...good(), idle: idle({ paint: { colours: 1, topPermille: 1000 } }) }).join()).toContain("blank");
		expect(judge({ ...good(), idle: idle({ paint: { colours: 40, topPermille: 995 } }) }).join()).toContain("blank");
	});

	// #285: the app probes its login shell once; two probes at launch is the bug.
	test("the app running more login shells than the budget allows fails", () => {
		const r = good();
		r.runs[2] = run({ loginShells: { app: BUDGET.appLoginShells + 1, hostd: 1 } });
		expect(judge(r).join()).toContain("login shells");
	});
});

// #591: the log line says, without opening the artifact, whether the web process started and what became of it.
describe("the one-line evidence summary", () => {
	const base = { why: "x", exited: null, appAlive: true, webProcessSeenMs: 165, webProcessAlive: true, webProcessStates: [{ pid: 42, state: "S" }], windowMappedMs: 1180, coredumps: [] };
	test("a page that hung with a live web process", () => {
		const l = evidenceLine(base, "out/evidence/run-5");
		expect(l).toContain("app alive; web process started at 165 ms, alive (42:S); window mapped at 1180 ms");
		expect(l).toContain("kept in out/evidence/run-5");
	});
	test("a web process that started and is gone says so, with the dump when the machine kept one", () => {
		expect(evidenceLine({ ...base, webProcessAlive: false, webProcessStates: [] }, "d")).toContain("and is gone (no exit status: not our child; no coredump listed)");
		expect(evidenceLine({ ...base, webProcessAlive: false, webProcessStates: [], coredumps: ["Thu 11:00 1000 1000 SIGSEGV present /usr/lib/WebKitWebProcess"] }, "d")).toContain("1 coredump(s)");
	});
	test("an app that exited, no web process, no window", () => {
		const l = evidenceLine({ ...base, exited: 134, appAlive: false, webProcessSeenMs: null, webProcessAlive: false, webProcessStates: [], windowMappedMs: null }, "d");
		expect(l).toContain("app exited with status 134; web process never started; window never mapped");
	});
});

// The collector itself, without sway: it reads /proc, so it runs where CI does. A machine without grim, swaymsg or coredumpctl
// still gets everything else, and the collector never throws (it would hide the failure it documents).
describe.skipIf(!existsSync("/proc/self/stat"))("collectEvidence", () => {
	test("keeps the trace, the app's output and the app log, and reports a live app with no web process", async () => {
		const home = mkdtempSync(join(tmpdir(), "evidence-home-"));
		const prev = process.env.ATLAS_HOSTD_DIR;
		process.env.ATLAS_HOSTD_DIR = home;
		try {
			Bun.spawnSync(["mkdir", "-p", join(home, "desktop")]);
			writeFileSync(join(home, "desktop/app.log"), "1 linux graphics: llvmpipe\n");
			const trace = join(dir, "ev-trace.txt");
			writeFileSync(trace, "5.0 main\n40.0 page:Started\n");
			writeFileSync(`${trace}.out`, "booted-but-stuck\n");
			const child = Bun.spawn(["sleep", "30"], { env: { ...process.env, ATLAS_BENCH_TAG: "ev-test" } });
			const evDir = join(dir, "ev-out");
			try {
				const e = collectEvidence(evDir, { trace, tag: "ev-test", root: child.pid, why: "no js:ready:gate", marks: { main: 5 }, webSeenMs: null, windowMs: null, exited: null, since: Date.now() - 1000 });
				expect(e).toMatchObject({ appAlive: true, webProcessAlive: false, webProcessSeenMs: null, exited: null });
			} finally {
				child.kill();
			}
			expect(readFileSync(join(evDir, "app-output.txt"), "utf8")).toContain("booted-but-stuck");
			expect(readFileSync(join(evDir, "app.log"), "utf8")).toContain("linux graphics");
			expect(readFileSync(join(evDir, "trace.txt"), "utf8")).toContain("page:Started");
			expect(readFileSync(join(evDir, "processes.txt"), "utf8")).toContain("sleep");
			expect(JSON.parse(readFileSync(join(evDir, "evidence.json"), "utf8"))).toMatchObject({ why: "no js:ready:gate", coredumps: [] });
		} finally {
			if (prev === undefined) delete process.env.ATLAS_HOSTD_DIR;
			else process.env.ATLAS_HOSTD_DIR = prev;
		}
	});
});

describe("--judge on a saved report", () => {
	const judgeFile = (r: unknown, ...extra: string[]) => {
		const p = join(dir, "report.json");
		writeFileSync(p, JSON.stringify(r));
		const x = Bun.spawnSync(["bun", script, "--judge", p, ...extra], { stdout: "pipe", stderr: "pipe" });
		return { code: x.exitCode, err: x.stderr.toString() };
	};
	test("exits 0 inside the budget and 1 outside it, with the reason on stderr", () => {
		expect(judgeFile(good()).code).toBe(0);
		const bad = good();
		bad.runs[1] = run({ readyMs: null });
		const x = judgeFile(bad);
		expect(x.code).toBe(1);
		expect(x.err).toContain("app_ready never arrived");
		expect(judgeFile(bad, "--no-check").code).toBe(0);
	});
	test("the committed baseline is inside its own budget", () => {
		const x = Bun.spawnSync(["bun", script, "--judge", join(root, "docs/perf/linux-startup.json")], { stdout: "pipe", stderr: "pipe" });
		expect(x.stderr.toString()).toBe("");
		expect(x.exitCode).toBe(0);
	});
});

describe("the real script on a private sway", () => {
	const hasSway = ["sway", "grim", "dbus-run-session"].every((t) => Bun.which(t));
	const bench = (app: string, ...args: string[]) => {
		const out = join(dir, "out.json");
		const x = Bun.spawnSync(["bun", script, "--runs", "1", "--idle-cpu", "0", "--ready-timeout", "4", "--out", out, ...args, app], { cwd: root, stdout: "pipe", stderr: "pipe" });
		return { code: x.exitCode, err: x.stderr.toString(), out: x.stdout.toString() };
	};
	const fake = (name: string, body: string) => {
		const p = join(dir, name);
		writeFileSync(p, `#!/bin/sh\n${body}\n`);
		chmodSync(p, 0o755);
		return p;
	};

	test("a binary that does not exist is a usage error (2)", () => {
		expect(bench("/nonexistent/atlas-desktop").code).toBe(2);
	});

	// Incident class: a bench that reports a number for an app that never opened.
	test.skipIf(!hasSway)("an app that exits at once fails (1) and says so", () => {
		const x = bench("/bin/false");
		expect(x.code).toBe(1);
		expect(x.err).toContain("app_ready never arrived");
		expect(x.err).toContain("exited with status 1");
	});

	test.skipIf(!hasSway)("an app that runs but never sends app_ready fails (1)", () => {
		const x = bench(fake("silent", "exec sleep 30"));
		expect(x.code).toBe(1);
		expect(x.err).toContain("app_ready never arrived");
	});

	// #591: the next occurrence has to be diagnosable from the artifact alone.
	test.skipIf(!hasSway)("an app that never sends app_ready leaves its evidence: trace, output, processes, screenshot, verdict", () => {
		const app = fake("stuck", `printf '5.0 main\\n40.0 page:Started\\n60.0 js:layout:mounted\\n' >> "$ATLAS_BOOT_TRACE"\necho booted-but-stuck\nexec sleep 30`);
		const x = bench(app);
		expect(x.code).toBe(1);
		expect(x.err).toContain("last page stage reached: layout:mounted");
		expect(x.err).toContain("evidence: app alive; web process never started; window");
		const ev = join(dir, "evidence/run-1");
		for (const f of ["trace.txt", "app-output.txt", "processes.txt", "evidence.json", "sway-tree.json"]) expect(existsSync(join(ev, f))).toBe(true);
		expect(readFileSync(join(ev, "app-output.txt"), "utf8")).toContain("booted-but-stuck");
		const j = JSON.parse(readFileSync(join(ev, "evidence.json"), "utf8"));
		expect(j).toMatchObject({ appAlive: true, webProcessAlive: false, webProcessSeenMs: null });
		expect(j.why).toContain("never reached: session:start, theme:ready");
		expect((JSON.parse(readFileSync(join(dir, "out.json"), "utf8")) as Report).runs[0].evidence).toBe(ev);
	});

	// The trace is the contract with boot.rs: an app that writes js:ready:gate to ATLAS_BOOT_TRACE has reached app_ready.
	test.skipIf(!hasSway)("an app that writes js:ready:gate to ATLAS_BOOT_TRACE gets its time reported", () => {
		const app = fake("ready", `printf '5.0 main\\n77.5 js:ready:gate\\n78.0 show:ready\\n' >> "$ATLAS_BOOT_TRACE"\nexec sleep 30`);
		const x = bench(app, "--no-check");
		expect(x.code).toBe(0);
		const r = JSON.parse(readFileSync(join(dir, "out.json"), "utf8")) as Report;
		expect(r.runs[0].readyMs).toBe(77.5);
		expect(r.runs[0].shownBy).toBe("ready");
		expect(r.runs[0].kind).toBe("cold");
	});
});

describe("ci.yml", () => {
	const ci = Bun.YAML.parse(readFileSync(join(root, ".github/workflows/ci.yml"), "utf8")) as any;
	// #284 acceptance: the bench runs in the job that opens the app, and its failure fails that job (no continue-on-error).
	// #591: the evidence of a start that never reported app_ready lives next to the report, so the artifact step that uploads the
	// report's directory carries it, and it runs when the bench step fails.
	test("the bench's --out directory is uploaded as an artifact even when the bench step fails", () => {
		const steps: any[] = ci.jobs["desktop-linux"].steps;
		const bench = steps.find((s) => String(s.run ?? "").includes("linux-startup-bench.ts"));
		const out = /--out (\S+)\/report\.json/.exec(bench.run)?.[1];
		expect(out).toBeTruthy();
		const up = steps.find((s) => String(s.uses ?? "").startsWith("actions/upload-artifact@") && String(s.with?.path ?? "").replace(/\/$/, "") === out);
		expect(up).toBeDefined();
		expect(up.if).toContain("!cancelled()");
	});

	test("the Desktop opens (Linux) job runs the startup bench as a gating step after the launch check", () => {
		const steps: any[] = ci.jobs["desktop-linux"].steps;
		const launch = steps.findIndex((s) => String(s.run ?? "").includes("linux-launch-check.sh"));
		const bench = steps.findIndex((s) => String(s.run ?? "").includes("linux-startup-bench.ts"));
		expect(launch).toBeGreaterThanOrEqual(0);
		expect(bench).toBeGreaterThan(launch);
		expect(steps[bench]["continue-on-error"]).toBeUndefined();
		expect(steps[bench].run).toContain("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1");
	});
});
