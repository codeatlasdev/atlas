// bun scripts/linux-idle-bench.ts --sock <debug.sock> --atlas <atlas binary> [options]
// bun scripts/linux-idle-bench.ts --judge <report.jsonl> <label>=<max CPU %> ...     (re-judge saved windows, no app; exit 1 over budget)
// bun scripts/linux-idle-bench.ts --judge <report.jsonl> --gate                      (the CI budgets, GATE_BUDGET; a gated state with no window fails; scripts/linux-idle-gate.sh)
//
// What a RUNNING Atlas Desktop costs while it sits in one state (#289, docs/perf/linux-webkit-study.md): CPU % per
// process role over a window, RSS and PSS per process, and what the page itself reports (rAF/s, long tasks) from
// `atlas desktop debug perf`. scripts/linux-startup-bench.ts measures launch and the first idle minute of a fresh HOME;
// this measures any state you put the app in (a project open, a terminal printing, a chat streaming, the window
// hidden) so a fix to "WebKit burns CPU in state X" has a number to move.
//
//   --sock S        the instance's ATLAS_DEBUG_SOCK (an agent instance: `bun run desktop:agent`, never the user's app)
//   --atlas A       the atlas CLI that talks to it (default target/debug/atlas)
//   --label L       prefix of the window's label in the report (`<label>/<scenario>`), e.g. the renderer or the knob under test
//   --scenario S    put the app in a state first and measure it: welcome (start with no project), workspace (SCM dock open),
//                   stack-running, terminal-idle, terminal-output, chat-idle, chat-stream, list-scroll, unfocused, hidden-workspace,
//                   hidden-chat-stream, unfocused-chat-stream. Without it the app is measured as it is. The scenarios open a
//                   fixture project (git repo, 2 worktrees, 2000 files) under the instance's HOME and use
//                   scripts/fixtures/linux-idle/fake-omp.py as the chat engine; "unfocused" needs `foot` and a keyboard on the
//                   instance's own sway (docs/perf/linux-webkit-study.md, "Method").
//   --settle S      seconds to wait before measuring (default 10)
//   --seconds S     length of the CPU window (default 30)
//   --probe S       afterwards, run `debug perf --seconds S` (default 5; 0 skips). The probe's own requestAnimationFrame loop
//                   forces a frame per vsync, so it runs AFTER the CPU window, never during it.
//   --out FILE      append the scenario's JSON line there (default: print only)
//
// CPU is utime+stime from /proc/<pid>/stat at both ends of the window, per process, summed by role; 100 = one core.
import { spawnSync } from "bun";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// ---------- pure: tested in linux-idle-bench.test.ts ----------

/** One thread: its name (`prctl(PR_SET_NAME)`, so WebKit and tokio name theirs), CPU ticks and context switches (a switch is a wakeup). */
export type Thread = { tid: number; name: string; ticks: number; switches: number };
export type Sample = { pid: number; role: string; comm: string; ticks: number; rssKb: number; pssKb: number; swapKb: number; threads: Thread[] };
export type Window = { label: string; seconds: number; cpuPct: Record<string, number>; totalCpuPct: number; rssMb: number; pssMb: number; swapMb: number; processes: { role: string; pid: number; cpuPct: number; /** context switches per second, all threads */ switchesPerS: number; rssMb: number; pssMb: number; swapMb: number; /** busiest threads over the window */ threads: { name: string; cpuPct: number; switchesPerS: number }[] }[] };

export const TICKS_PER_S = 100;
const round1 = (n: number) => Math.round(n * 10) / 10;

/** What a process is, by its command name (kernel comm is cut at 15 characters) and command line. */
export function roleOf(comm: string, cmdline: string): string {
	if (comm.startsWith("atlas-desktop")) return "app";
	if (comm.startsWith("WebKitWebProces")) return "webprocess";
	if (comm.startsWith("WebKitNetworkPr")) return "network";
	if (comm.startsWith("WebKitGPUProces")) return "gpuprocess";
	if (comm === "atlas" && /\bhostd\b/.test(cmdline)) return "hostd";
	return comm;
}

/** CPU of every process between two samples. A process that only exists in `after` ran for the whole window since its start (its ticks are all new). */
export function cpuWindow(label: string, seconds: number, before: Sample[], after: Sample[]): Window {
	const was = new Map(before.map((s) => [s.pid, s.ticks]));
	const processes = after.map((s) => {
		const cpu = ((s.ticks - (was.get(s.pid) ?? s.ticks)) / TICKS_PER_S / seconds) * 100;
		const wasT = new Map((before.find((b) => b.pid === s.pid)?.threads ?? []).map((t) => [t.tid, t]));
		const threads = s.threads
			.map((t) => ({ name: t.name, cpuPct: round1(((t.ticks - (wasT.get(t.tid)?.ticks ?? t.ticks)) / TICKS_PER_S / seconds) * 100), switchesPerS: round1((t.switches - (wasT.get(t.tid)?.switches ?? t.switches)) / seconds) }))
			.sort((a, b) => b.cpuPct - a.cpuPct || b.switchesPerS - a.switchesPerS)
			.slice(0, 4);
		const switchesPerS = round1(threads.length ? s.threads.reduce((n, t) => n + t.switches - (wasT.get(t.tid)?.switches ?? t.switches), 0) / seconds : 0);
		return { role: s.role, pid: s.pid, cpuPct: round1(cpu), switchesPerS, threads, rssMb: round1(s.rssKb / 1024), pssMb: round1(s.pssKb / 1024), swapMb: round1(s.swapKb / 1024) };
	});
	const cpuPct: Record<string, number> = {};
	for (const p of processes) cpuPct[p.role] = round1((cpuPct[p.role] ?? 0) + p.cpuPct);
	const sum = (xs: number[]) => round1(xs.reduce((a, b) => a + b, 0));
	return { label, seconds, cpuPct, totalCpuPct: sum(processes.map((p) => p.cpuPct)), rssMb: sum(processes.map((p) => p.rssMb)), pssMb: sum(processes.map((p) => p.pssMb)), swapMb: sum(processes.map((p) => p.swapMb)), processes: processes.sort((a, b) => b.pssMb - a.pssMb) };
}

/** Commits per second a Wayland client made, from `WAYLAND_DEBUG=client` lines (`[<ms>] -> wl_surface@N.commit()`) between two byte offsets of the log. */
export function commitsPerSecond(log: string, seconds: number): number {
	const n = (log.match(/ -> wl_surface@\d+\.commit\(\)/g) ?? []).length;
	return round1(n / seconds);
}

/**
 * The states CI holds to a CPU budget (% of one core over the window, all processes of the instance, llvmpipe: a GitHub
 * runner has no GPU). Only states that are already idle on `main`: a state with a known cost is added here by the PR that
 * removes it (`stack-running` after the animation fix, `terminal-idle` after the hostd fix, `chat-stream`, `terminal-output`),
 * so the gate never starts red. Each is the worst of 3 runs of this machine plus headroom for a 2-vCPU runner:
 * docs/perf/linux-webkit-study.md, "Gate".
 */
export const GATE_BUDGET: Record<string, number> = { welcome: 3, workspace: 4 };

/** Why the windows saved by a gate run fail: a gated state over its budget, or a gated state that was never measured (nothing measured must not read as a pass). */
export function judgeGate(windows: Window[], budgets: Record<string, number> = GATE_BUDGET): string[] {
	return Object.entries(budgets).flatMap(([scenario, budget]) => {
		const w = windows.filter((x) => x.label.endsWith(`/${scenario}`));
		return w.length ? w.flatMap((x) => judgeWindow(x, budget)) : [`${scenario}: no window measured`];
	});
}

/** Why a saved window fails its budget (empty = passes). */
export function judgeWindow(w: Window, budgetPct: number): string[] {
	return w.totalCpuPct > budgetPct ? [`${w.label}: ${w.totalCpuPct}% of a core over ${w.seconds} s > ${budgetPct}% (${JSON.stringify(w.cpuPct)})`] : [];
}

// ---------- /proc ----------

function children(root: number): number[] {
	const ppid = new Map<number, number[]>();
	for (const d of readdirSync("/proc")) {
		if (!/^\d+$/.test(d)) continue;
		try {
			const s = readFileSync(`/proc/${d}/stat`, "utf8");
			const f = s.slice(s.lastIndexOf(")") + 2).split(" ");
			const p = Number(f[1]);
			ppid.set(p, [...(ppid.get(p) ?? []), Number(d)]);
		} catch {}
	}
	const out = [root];
	for (let i = 0; i < out.length; i++) out.push(...(ppid.get(out[i]) ?? []));
	return out;
}

/** hostd detaches from the app; the instance's own ATLAS_HOSTD_DIR in its environment finds it. */
function tagged(hostdDir: string): number[] {
	const out: number[] = [];
	for (const d of readdirSync("/proc")) {
		if (!/^\d+$/.test(d)) continue;
		try {
			if (readFileSync(`/proc/${d}/environ`, "latin1").split("\0").includes(`ATLAS_HOSTD_DIR=${hostdDir}`)) out.push(Number(d));
		} catch {}
	}
	return out;
}

function sample(pids: number[]): Sample[] {
	const out: Sample[] = [];
	for (const pid of pids) {
		try {
			const s = readFileSync(`/proc/${pid}/stat`, "utf8");
			const f = s.slice(s.lastIndexOf(")") + 2).split(" ");
			const comm = s.slice(s.indexOf("(") + 1, s.lastIndexOf(")"));
			const m = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8");
			const kb = (k: string) => Number(new RegExp(`^${k}:\\s+(\\d+) kB`, "m").exec(m)?.[1] ?? 0);
			const cmd = readFileSync(`/proc/${pid}/cmdline`, "utf8").replaceAll("\0", " ");
			const threads: Thread[] = [];
			for (const tid of readdirSync(`/proc/${pid}/task`)) {
				try {
					const ts = readFileSync(`/proc/${pid}/task/${tid}/stat`, "utf8");
					const tf = ts.slice(ts.lastIndexOf(")") + 2).split(" ");
					const st = readFileSync(`/proc/${pid}/task/${tid}/status`, "utf8");
					const sw = Number(/voluntary_ctxt_switches:\s+(\d+)/.exec(st)?.[1] ?? 0) + Number(/nonvoluntary_ctxt_switches:\s+(\d+)/.exec(st)?.[1] ?? 0);
					threads.push({ tid: Number(tid), name: ts.slice(ts.indexOf("(") + 1, ts.lastIndexOf(")")), ticks: Number(tf[11]) + Number(tf[12]), switches: sw });
				} catch {}
			}
			out.push({ pid, role: roleOf(comm, cmd), comm, ticks: Number(f[11]) + Number(f[12]), rssKb: kb("Rss"), pssKb: kb("Pss"), swapKb: kb("SwapPss"), threads });
		} catch {}
	}
	return out;
}

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};

function debug(atlas: string, sock: string, ...a: string[]) {
	const r = spawnSync([atlas, "desktop", "debug", ...a], { env: { ...process.env, ATLAS_DEBUG_SOCK: sock } });
	if (r.exitCode !== 0) throw new Error(`atlas desktop debug ${a.join(" ")} failed: ${r.stderr.toString() || r.stdout.toString()}`);
	return r.stdout.toString();
}

/** The environment of a process, which is how the bench finds the instance's private sway (SWAYSOCK) without ever reading its own. */
function environOf(pid: number): Record<string, string> {
	const out: Record<string, string> = {};
	for (const kv of readFileSync(`/proc/${pid}/environ`, "latin1").split("\0")) {
		const i = kv.indexOf("=");
		if (i > 0) out[kv.slice(0, i)] = kv.slice(i + 1);
	}
	return out;
}

const here = import.meta.dir;

/**
 * Two ports nobody listens on, picked by the kernel (port 0) and held open until `release()` so that a second reservation made
 * meanwhile (another bench of this machine, another agent's) cannot get the same ones. Fixed ports made two concurrent benches
 * start each other's services and measure the wrong stack.
 */
export function reservePorts(n = 2): { ports: number[]; release: () => void } {
	const held = Array.from({ length: n }, () => Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } }));
	return { ports: held.map((l) => l.port), release: () => held.forEach((l) => l.stop(true)) };
}

/** The fixture's `atlas.yaml`: two real listeners (`python3 -m http.server`) on the given ports. */
export function fixtureYaml(ports: number[]): string {
	const svc = (name: string, port: number) => `  ${name}:\n    command: "python3 -m http.server ${port} --bind 127.0.0.1"\n    port: ${port}\n`;
	return `name: idle-project\nservices:\n${svc("web", ports[0]!)}${svc("api", ports[1]!)}`;
}

/** The ports an existing fixture's `atlas.yaml` listens on. */
export function fixturePorts(yaml: string): number[] {
	return [...yaml.matchAll(/^    port: (\d+)$/gm)].map((m) => Number(m[1]));
}

/** A throwaway project the scenarios open: a git repo with `atlas.yaml`, ~2000 files in 40 folders, two linked worktrees and an uncommitted change. */
function makeFixture(dir: string): { root: string; ports: number[] } {
	const git = (cwd: string, ...a: string[]) => {
		const r = spawnSync(["git", "-c", "user.name=bench", "-c", "user.email=bench@example.com", ...a], { cwd });
		if (r.exitCode !== 0) throw new Error(`git ${a.join(" ")}: ${r.stderr.toString()}`);
	};
	const root = join(dir, "idle-project");
	if (existsSync(join(root, ".git"))) return { root, ports: fixturePorts(readFileSync(join(root, "atlas.yaml"), "utf8")) }; // built once per instance, every scenario opens the same project (and the same ports)
	mkdirSync(root, { recursive: true });
	const reserved = reservePorts();
	writeFileSync(join(root, "atlas.yaml"), fixtureYaml(reserved.ports));
	for (let d = 0; d < 40; d++) {
		mkdirSync(join(root, `dir-${String(d).padStart(2, "0")}`), { recursive: true });
		for (let f = 0; f < 50; f++) writeFileSync(join(root, `dir-${String(d).padStart(2, "0")}`, `file-${String(f).padStart(3, "0")}.ts`), `export const v${f} = ${d * 100 + f};\n`.repeat(8));
	}
	git(root, "init", "-q", "-b", "main");
	git(root, "add", "-A");
	git(root, "commit", "-q", "-m", "fixture");
	for (const w of ["wt1", "wt2"]) git(root, "worktree", "add", "-q", join(dir, `idle-project-${w}`), "-b", w);
	writeFileSync(join(root, "dir-00/file-000.ts"), "export const changed = true;\n");
	writeFileSync(join(root, "untracked.txt"), "new\n");
	reserved.release();
	return { root, ports: reserved.ports };
}

/** Page-side setup (through `debug eval`, which wraps it in an async function) shared by the scenarios. */
const JS = {
	open: (root: string) => `const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(root)} }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === ${JSON.stringify(root)}).id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id`,
	chord: (code: string) => `document.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(code.slice(3))}, code: ${JSON.stringify(code)}, ctrlKey: true, shiftKey: true, bubbles: true })); return 1`,
	terminal: (id: string, argv: string[], extra = "") => `const t = await window.__atlas.ws.newTerminal(${JSON.stringify(id)}, { argv: ${JSON.stringify(argv)}, label: "bench"${extra} }); await new Promise((r) => setTimeout(r, 2500)); return t.id`,
	start: `[...document.querySelectorAll("button")].find((b) => b.innerText.trim() === "Start")?.click(); return 1`,
	stop: `[...document.querySelectorAll("button")].find((b) => b.innerText.trim() === "Stop")?.click(); return 1`,
	close: (tab: string) => `window.__atlas.ws.closeTab(${JSON.stringify(tab)}); return 1`,
	scroll: `const el = [...document.querySelectorAll(".dock *")].find((e) => e.scrollHeight > e.clientHeight + 20 && getComputedStyle(e).overflowY !== "visible"); if (!el) throw new Error("no scrollable list in the dock"); window.__benchScroll = true; (function step(t0) { if (!window.__benchScroll) return; requestAnimationFrame((t) => { el.scrollTop = ((t - t0) * 0.1) % (el.scrollHeight - el.clientHeight); step(t0); }); })(performance.now()); return el.scrollHeight`,
	scrollStop: `window.__benchScroll = false; return 1`,
};

export const SCENARIOS = [
	"welcome",
	"workspace",
	"terminal-idle",
	"terminal-output",
	"chat-idle",
	"chat-stream",
	"list-scroll",
	"unfocused",
	"hidden-workspace",
	"stack-running",
	"hidden-chat-stream",
	"unfocused-chat-stream",
] as const;

type Ctx = { pid: number; atlas: string; sock: string; run: (js: string) => unknown; sway: (...a: string[]) => string; tmp: string; project: string; wt: string };

/** Puts the app in the state `name` measures and returns what undoes it. `welcome` needs an instance with no project: start it that way. */
async function enter(name: (typeof SCENARIOS)[number], c: Ctx): Promise<() => void> {
	const rpc = (argv: string[]) => c.run(JS.terminal(c.wt, argv)) as string;
	const stream = join(c.tmp, "stream");
	const chat = () => c.run(JS.terminal(c.wt, ["python3", join(here, "fixtures/linux-idle/fake-omp.py"), "--mode", "rpc-ui", "--stream-file", stream], ", chat: true, agent: \"omp\"")) as string;
	switch (name) {
		case "welcome":
			return () => {};
		case "workspace":
			c.run(JS.chord("KeyG"));
			return () => {};
		case "stack-running":
			// The two services are real listeners, so their dots turn from "starting" (pulsing) to ok.
			c.run(JS.start);
			await Bun.sleep(8000);
			return () => c.run(JS.stop);
		case "terminal-idle": {
			const t = rpc(["bash", "--norc", "--noprofile", "-i"]);
			return () => c.run(JS.close(t));
		}
		case "terminal-output": {
			const t = rpc(["sh", "-c", 'while :; do yes "2026-11-22 12:00:00 INFO request handled path=/api/v1/items status=200 ms=12" | head -c 20M; done']);
			return () => c.run(JS.close(t));
		}
		case "chat-idle": {
			rmSync(stream, { force: true });
			const t = chat();
			return () => c.run(JS.close(t));
		}
		case "chat-stream": {
			writeFileSync(stream, "");
			const t = chat();
			return () => (rmSync(stream, { force: true }), c.run(JS.close(t)));
		}
		case "list-scroll":
			c.run(JS.chord("KeyE"));
			await Bun.sleep(1500);
			c.run(JS.scroll);
			return () => c.run(JS.scrollStop);
		case "unfocused": {
			// A second window takes the keyboard focus on the instance's own sway; the app stays mapped and visible.
			const e = environOf(c.pid);
			const other = Bun.spawn(["foot", "-e", "sleep", "100000"], { env: { PATH: process.env.PATH ?? "", HOME: c.tmp, XDG_RUNTIME_DIR: e.XDG_RUNTIME_DIR, WAYLAND_DISPLAY: e.WAYLAND_DISPLAY }, stdout: null, stderr: null });
			await Bun.sleep(1500);
			return () => other.kill();
		}
		case "hidden-workspace":
			c.sway("workspace", "2");
			return () => c.sway("workspace", "1");
		case "hidden-chat-stream": {
			// What a user who streams an agent and switches to another workspace pays: the page keeps animating unseen?
			const stop = await enter("chat-stream", c);
			c.sway("workspace", "2");
			return () => (c.sway("workspace", "1"), stop());
		}
		case "unfocused-chat-stream": {
			const stop = await enter("chat-stream", c);
			const off = await enter("unfocused", c);
			return () => (off(), stop());
		}
	}
}

let fixturePortsUsed: number[] | undefined;

async function measure() {
	const sock = arg("sock") as string;
	const atlas = arg("atlas", "target/debug/atlas") as string;
	const label = arg("label", "run") as string;
	const scenario = arg("scenario");
	const settle = Number(arg("settle", "10"));
	const seconds = Number(arg("seconds", "30"));
	const probeS = Number(arg("probe", "5"));
	if (!sock || !existsSync(sock)) throw new Error("--sock must be the debug socket of a running agent instance");
	const info = JSON.parse(debug(atlas, sock, "info"));
	if (info.agent !== true) throw new Error("not an agent instance: refusing to measure someone's real app");
	const root = info.pid as number;
	const env = environOf(root);
	const pids = () => [...new Set([...children(root), ...(env.ATLAS_HOSTD_DIR ? tagged(env.ATLAS_HOSTD_DIR) : [])])];
	let undo = () => {};
	const tmp = join(env.HOME ?? "/tmp", ".idle-bench");
	mkdirSync(tmp, { recursive: true });
	if (scenario) {
		if (!(SCENARIOS as readonly string[]).includes(scenario)) throw new Error(`unknown scenario ${scenario}; one of ${SCENARIOS.join(", ")}`);
		const run = (js: string) => JSON.parse(debug(atlas, sock, "eval", js));
		const sway = (...a: string[]) => {
			// Only the instance's own compositor: the socket comes from the app's environment, never from ours.
			if (!env.SWAYSOCK?.startsWith(env.XDG_RUNTIME_DIR ?? "\0")) throw new Error("the instance has no private sway");
			return spawnSync(["swaymsg", ...a], { env: { PATH: process.env.PATH ?? "", SWAYSOCK: env.SWAYSOCK } }).stdout.toString();
		};
		let project = arg("project") as string | undefined;
		let fixture: { root: string; ports: number[] } | undefined;
		if (!project && scenario !== "welcome") (fixture = makeFixture(tmp)), (project = fixture.root);
		fixturePortsUsed = fixture?.ports;
		const wt = scenario === "welcome" ? "" : (run(JS.open(project as string)) as string);
		undo = await enter(scenario as (typeof SCENARIOS)[number], { pid: root, atlas, sock, run, sway, tmp, project: project ?? "", wt });
	}
	await Bun.sleep(settle * 1000);
	const before = sample(pids());
	const t0 = performance.now();
	await Bun.sleep(seconds * 1000);
	const secs = (performance.now() - t0) / 1000;
	const after = sample(pids());
	const w = cpuWindow(`${label}/${scenario ?? "state"}`, secs, before, after);
	const res: Record<string, unknown> = { ...w, fixturePorts: fixturePortsUsed, when: new Date().toISOString(), loadAvg1: Number(readFileSync("/proc/loadavg", "utf8").split(" ")[0]) };
	if (probeS > 0) res.page = JSON.parse(debug(atlas, sock, "perf", "--seconds", String(probeS)));
	// What the page believes about itself in this state (the probe above ran in it too).
	res.pageState = JSON.parse(debug(atlas, sock, "eval", "return { visibility: document.visibilityState, hasFocus: document.hasFocus(), hidden: document.hidden, animations: document.getAnimations().filter((a) => a.playState === 'running').length }"));
	undo();
	console.log(JSON.stringify(res));
	const out = arg("out");
	if (out) appendFileSync(out, `${JSON.stringify(res)}\n`);
}

if (import.meta.main) {
	const j = arg("judge");
	if (process.argv.includes("--gate-scenarios")) console.log(Object.keys(GATE_BUDGET).join(" "));
	else if (j && process.argv.includes("--gate")) {
		const bad = judgeGate(readFileSync(j, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l) as Window));
		if (bad.length) (console.error(bad.join("\n")), process.exit(1));
	} else if (j) {
		const bad = readFileSync(j, "utf8").trim().split("\n").flatMap((l) => {
			const w = JSON.parse(l) as Window;
			const b = process.argv.find((a) => a.startsWith(`${w.label}=`))?.split("=")[1];
			return b ? judgeWindow(w, Number(b)) : [];
		});
		if (bad.length) (console.error(bad.join("\n")), process.exit(1));
	} else await measure();
}
