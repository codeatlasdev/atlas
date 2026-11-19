// bun scripts/linux-startup-bench.ts [options] <atlas-desktop binary | .AppImage>
// bun scripts/linux-startup-bench.ts --judge docs/perf/linux-startup.json [--no-check]     (re-judge a saved report, no app)
//
// What Atlas Desktop costs on Linux before the user does anything (#284): cold and warm start to the first real frame,
// resident memory per process at idle, idle CPU. Same launch as scripts/linux-launch-check.sh (private headless sway,
// private D-Bus, throwaway HOME, nothing on your screen) so it runs the same way locally and in the "Desktop opens
// (Linux)" CI job. Exits 1 when the app never reaches `app_ready`, paints nothing, or a number is over BUDGET.
//
//   --runs N          starts measured, the first on a fresh HOME ("cold": no theme cache, no WebKit cache, hostd and
//                     config created), the others on the same HOME ("warm"). Default 5. The OS page cache is not dropped
//                     (that needs root): "cold" is the first launch of a fresh install, not a cold disk.
//   --idle-settle S   seconds after `app_ready` before the memory snapshot (default 30)
//   --idle-cpu S      seconds the idle CPU is averaged over, after the snapshot (default 60); 0 skips the idle run
//   --ready-timeout S seconds for `app_ready` to arrive (default 90)
//   --size WxH        virtual display (default 1440x900)
//   --env K=V         extra environment for the app (repeatable; CI passes WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1)
//   --out FILE        where the JSON report goes (default linux-startup-out/report.json)
//   --save FILE       also write it there (docs/perf/linux-startup.json: the committed baseline)
//   --no-check        report without judging
//
// Per run the app is started with ATLAS_BOOT_TRACE (boot.rs: `<ms since process start> <mark>`), and the script
// watches /proc from outside: spawn -> first WebKitWebProcess -> window mapped in sway. `app_ready` is the page's first
// real frame (`js:ready:gate`); the window is shown by it (`show:ready`) or, when it is late, by boot.rs's 1 s
// deadline (`show:deadline`), which the report records as `shownBy`.
//
// When a run never reaches `app_ready` (#591) the evidence is kept in `<--out dir>/evidence/run-N/` (uploaded with the report by
// CI): the raw trace and what the app printed, `app.log` and `stderr.log`, a screenshot of the display, sway's window tree, every
// process the run owns with its state (is the web process alive, was it ever seen), and `evidence.json` with the verdict that
// `explainMissingReady` draws from the trace (which page stage was the last one reached, a mark lost, unreadable trace lines).
//
// The login shell: the throwaway HOME's bash/zsh/fish rc files log who started them, so `loginShells` counts the
// interactive login shells the app and hostd run at launch by parent process (#285), without patching either.
//
// Memory is PSS (plus the swapped-out part of it, SwapPss) and RSS from /proc/<pid>/smaps_rollup of every process the app owns: its process tree, plus anything
// carrying this run's ATLAS_BENCH_TAG (hostd detaches itself). Needs: sway, grim, dbus-run-session.
import { spawnSync } from "bun";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";

// ---------- pure: parsing, summary, judgement (tested in linux-startup-bench.test.ts) ----------

/**
 * Chosen from measurements, with at least 1.6x headroom over the worst sample of each. docs/perf/linux-startup.json (Ryzen 7 7735HS, 5.5 GB
 * RAM shared with builds, debug build, llvmpipe, load ~9, WebKitGTK 2.54.1): cold app_ready 2.1 s, warm p95 1.9 s, idle PSS 404-485 MB, largest
 * process 148-168 MB, idle CPU 0.9%. The "Desktop opens (Linux)" job (GitHub runner, no GPU, sandbox off, Ubuntu's WebKitGTK), 6 runs of the
 * whole bench on 2 runner VMs: cold 1.33-1.93 s, warm p95 1.48-2.50 s, idle PSS 756-791 MB, largest process (a web process) 357-392 MB,
 * idle CPU 0.3-0.4%, login shells app 2 / hostd 1.
 * Worst sample x: time 2x (cold 5 s, warm p95 5 s), memory 1.6x (PSS 1300 MB, largest 600 MB). The memory budgets catch a doubling, not a
 * single extra web process (~70-100 MB): that is what the per-process list in the report is for, and what the tests of #286 pin.
 */
export const BUDGET = {
	/** warm starts: p95 of `app_ready` (ms since process start). */
	warmReadyP95Ms: 5000,
	/** the first launch of a fresh HOME. */
	coldReadyMs: 5000,
	/** PSS of every process the app owns, at idle, MB. */
	idlePssMb: 1300,
	/** the largest single process, MB: the budget that names who grows. */
	largestPssMb: 600,
	/** idle CPU of all processes together, % of one core, averaged over the window. */
	idleCpuPct: 5,
	/** interactive login shells started by the app at launch: 2 today (#285 makes it 1). */
	appLoginShells: 2,
};

export type Stat = { p50: number; p95: number; max: number };
export type Proc = { role: string; pid: number; comm: string; rssMb: number; /** Pss + SwapPss */ pssMb: number; swapMb: number };
export type Run = {
	kind: "cold" | "warm";
	/** ms since process start, first occurrence of each trace mark. */
	marks: Record<string, number>;
	/** ms since spawn, seen from outside. */
	spawnToWebProcessMs: number | null;
	spawnToWindowMs: number | null;
	readyMs: number | null;
	shownBy: string | null;
	loginShells: Record<string, number>;
	/** Set when `app_ready` never arrived: the directory holding the evidence of this run. */
	evidence?: string;
};
export type Idle = {
	settleS: number;
	cpuS: number;
	processes: Proc[];
	rssMb: number;
	/** Pss + SwapPss, all processes */
	pssMb: number;
	/** of which swapped out when measured: non-zero means the machine was short of memory and RSS under-reports */
	swapMb: number;
	/** process with the largest PSS */
	dominant: { role: string; pid: number; pssMb: number; share: number };
	cpuPct: { total: number; byRole: Record<string, number> };
	pssMbAfter: number;
	/** distinct colours / commonest share (per mille) of a screenshot at the end of the window: a blank page has one colour. */
	paint: { colours: number; topPermille: number };
};
export type Report = {
	runs: Run[];
	idle: Idle | null;
	budget?: typeof BUDGET;
	[k: string]: unknown;
};

/** Linear-interpolated percentile of an unsorted list. */
export function pct(xs: number[], p: number): number {
	if (!xs.length) return NaN;
	const s = [...xs].sort((a, b) => a - b);
	const i = (s.length - 1) * p;
	const lo = Math.floor(i);
	return s[lo] + (s[Math.min(lo + 1, s.length - 1)] - s[lo]) * (i - lo);
}
export const stat = (xs: number[]): Stat => ({
	p50: round(pct(xs, 0.5)),
	p95: round(pct(xs, 0.95)),
	max: round(Math.max(...xs)),
});
const round = (n: number) => Math.round(n * 10) / 10;

/** First occurrence of every mark in a boot trace (`<ms> <mark>` per line); a mark repeated (page:Started, ...) keeps its first time. */
export function parseTrace(text: string): Record<string, number> {
	const out: Record<string, number> = {};
	for (const line of text.split("\n")) {
		const m = /^(\d+(?:\.\d+)?) (\S+)$/.exec(line.trim());
		if (m && !(m[2] in out)) out[m[2]] = Number(m[1]);
	}
	return out;
}

/** Lines of a boot trace that are not `<ms> <mark>`: a write cut short or two marks interleaved (#591). */
export function garbledLines(text: string): string[] {
	return text.split("\n").map((l) => l.trim()).filter((l) => l && !/^\d+(?:\.\d+)?\s\S+$/.test(l));
}

/** The page's startup stages in the order they happen (`js:` marks, apps/desktop `session.svelte.ts`, `theme.svelte.ts`, `ready.svelte.ts`).
 * `ready:gate` needs the theme, the project list and the workspace; the last stage reached says which one is missing. */
export const PAGE_STAGES = ["js:layout:mounted", "js:session:start", "js:theme:ready", "js:projects:loaded", "js:workspace:loaded", "js:ready:gate"];

/** One sentence on why `app_ready` is missing, from the trace alone. */
export function explainMissingReady(marks: Record<string, number>, garbled: string[], exited: number | null, timeoutS: number): string {
	const out: string[] = [];
	if (exited !== null) out.push(`the app exited with status ${exited}`);
	else out.push(`no js:ready:gate within ${timeoutS} s`);
	if ("show:ready" in marks) out.push(`Rust did get app_ready (show:ready at ${marks["show:ready"]} ms): only the page's trace mark is missing`);
	else if ("show:deadline" in marks) out.push(`the window was shown by the 1 s deadline, app_ready never reached Rust`);
	else if (!Object.keys(marks).some((k) => k.startsWith("show:"))) out.push("the window was never shown");
	const reached = PAGE_STAGES.filter((k) => k in marks);
	const last = reached.at(-1);
	if (!("js:ready:gate" in marks)) {
		if (!last) out.push(Object.keys(marks).some((k) => k.startsWith("page:")) ? "the page loaded but ran no startup stage (no js: mark at all)" : "the page never loaded (no page: mark)");
		else out.push(`last page stage reached: ${last.slice(3)}; never reached: ${PAGE_STAGES.filter((k) => !(k in marks)).map((k) => k.slice(3)).join(", ")}`);
	}
	if (garbled.length) out.push(`${garbled.length} unreadable trace line(s), e.g. ${JSON.stringify(garbled[0])}`);
	return out.join("; ");
}

/** What a process is, from its command name and command line. */
export function roleOf(comm: string, cmdline: string): string {
	if (comm.startsWith("atlas-desktop")) return "app";
	if (comm.startsWith("WebKitWebProces")) return "webprocess";
	if (comm.startsWith("WebKitNetworkPr")) return "network";
	if (comm.startsWith("WebKitGPUProces")) return "gpuprocess";
	if (comm === "atlas" && /\bhostd\b/.test(cmdline)) return "hostd";
	return comm;
}

/** Probes logged by the rc files (`<parent pid> <parent comm>` per line) counted by who started the shell. */
export function countProbes(log: string): Record<string, number> {
	const out: Record<string, number> = {};
	for (const line of log.split("\n")) {
		const comm = line.trim().split(/\s+/)[1];
		if (!comm) continue;
		const who = comm.startsWith("atlas-desktop") ? "app" : comm === "atlas" ? "hostd" : comm;
		out[who] = (out[who] ?? 0) + 1;
	}
	return out;
}

/** The numbers a report is judged on. */
export function summarize(r: Report) {
	const warm = r.runs.filter((x) => x.kind === "warm" && x.readyMs != null);
	const cold = r.runs.find((x) => x.kind === "cold");
	return {
		coldReadyMs: cold?.readyMs ?? null,
		warmReady: warm.length ? stat(warm.map((x) => x.readyMs as number)) : null,
		warmWindow: warm.length && warm.every((x) => x.spawnToWindowMs != null) ? stat(warm.map((x) => x.spawnToWindowMs as number)) : null,
		warmWebProcess: warm.length && warm.every((x) => x.spawnToWebProcessMs != null) ? stat(warm.map((x) => x.spawnToWebProcessMs as number)) : null,
	};
}

/** Why a report fails (empty = passes). `budget` defaults to BUDGET. */
export function judge(r: Report, budget: typeof BUDGET = BUDGET): string[] {
	const bad: string[] = [];
	if (!r.runs.length) return ["no run was measured"];
	r.runs.forEach((x, i) => {
		if (x.readyMs == null) bad.push(`run ${i + 1} (${x.kind}): app_ready never arrived`);
	});
	const s = summarize(r);
	if (s.coldReadyMs != null && s.coldReadyMs > budget.coldReadyMs) bad.push(`cold start to app_ready ${s.coldReadyMs} ms > ${budget.coldReadyMs} ms`);
	if (s.warmReady && s.warmReady.p95 > budget.warmReadyP95Ms) bad.push(`warm start to app_ready p95 ${s.warmReady.p95} ms > ${budget.warmReadyP95Ms} ms`);
	for (const [i, x] of r.runs.entries()) {
		const n = x.loginShells.app ?? 0;
		if (n > budget.appLoginShells) bad.push(`run ${i + 1}: the app ran ${n} login shells at launch > ${budget.appLoginShells}`);
	}
	const idle = r.idle;
	if (idle) {
		if (idle.paint.colours < 12 || idle.paint.topPermille > 985) bad.push(`the page is blank at idle (${idle.paint.colours} colours, commonest ${idle.paint.topPermille}‰)`);
		if (idle.pssMb > budget.idlePssMb) bad.push(`idle PSS ${idle.pssMb} MB > ${budget.idlePssMb} MB (largest: ${idle.dominant.role} ${idle.dominant.pssMb} MB)`);
		if (idle.dominant.pssMb > budget.largestPssMb) bad.push(`${idle.dominant.role} (pid ${idle.dominant.pid}) PSS ${idle.dominant.pssMb} MB > ${budget.largestPssMb} MB`);
		if (idle.cpuS > 0 && idle.cpuPct.total > budget.idleCpuPct) bad.push(`idle CPU ${idle.cpuPct.total}% of a core > ${budget.idleCpuPct}% over ${idle.cpuS} s`);
	}
	return bad;
}

// ---------- /proc ----------

type P = { pid: number; ppid: number; comm: string; ticks: number };
const mb = (kb: number) => Math.round((kb / 1024) * 10) / 10;

function readProcs(): Map<number, P> {
	const out = new Map<number, P>();
	for (const d of readdirSync("/proc")) {
		if (!/^\d+$/.test(d)) continue;
		try {
			const s = readFileSync(`/proc/${d}/stat`, "utf8");
			const close = s.lastIndexOf(")");
			const f = s.slice(close + 2).split(" ");
			out.set(Number(d), { pid: Number(d), ppid: Number(f[1]), comm: s.slice(s.indexOf("(") + 1, close), ticks: Number(f[11]) + Number(f[12]) });
		} catch {
			// gone between readdir and read
		}
	}
	return out;
}

function descendants(procs: Map<number, P>, root: number): P[] {
	const want = new Set([root]);
	let grew = true;
	while (grew) {
		grew = false;
		for (const p of procs.values()) if (!want.has(p.pid) && want.has(p.ppid)) (want.add(p.pid), (grew = true));
	}
	return [...want].map((pid) => procs.get(pid)).filter((p): p is P => !!p);
}

const tagged = (pid: number, tag: string) => {
	try {
		return readFileSync(`/proc/${pid}/environ`, "latin1").split("\0").includes(`ATLAS_BENCH_TAG=${tag}`);
	} catch {
		return false;
	}
};

/** Everything this run owns: the app's tree and every process that carries its tag (hostd detaches). */
function owned(procs: Map<number, P>, root: number, tag: string): P[] {
	const out = new Map(descendants(procs, root).map((p) => [p.pid, p]));
	for (const p of procs.values()) if (!out.has(p.pid) && tagged(p.pid, tag)) out.set(p.pid, p);
	return [...out.values()];
}

/** `pss` counts what the kernel swapped out (SwapPss): on a busy machine a process's pages leave RAM, and a footprint that shrinks by being evicted is not a saving. */
function smaps(pid: number): { rss: number; pss: number; swap: number } | null {
	try {
		const t = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8");
		const kb = (k: string) => Number(new RegExp(`^${k}:\\s+(\\d+) kB`, "m").exec(t)?.[1] ?? 0);
		return { rss: kb("Rss"), pss: kb("Pss") + kb("SwapPss"), swap: kb("SwapPss") };
	} catch {
		return null;
	}
}

function snapshot(procs: P[]): (Proc & { ticks: number })[] {
	const out: (Proc & { ticks: number })[] = [];
	for (const p of procs) {
		const m = smaps(p.pid);
		if (!m) continue;
		let cmd = "";
		try {
			cmd = readFileSync(`/proc/${p.pid}/cmdline`, "utf8").replaceAll("\0", " ");
		} catch {}
		out.push({ role: roleOf(p.comm, cmd), pid: p.pid, comm: p.comm, rssMb: mb(m.rss), pssMb: mb(m.pss), swapMb: mb(m.swap), ticks: p.ticks });
	}
	return out.sort((a, b) => b.pssMb - a.pssMb);
}
const bare = ({ ticks: _t, ...p }: Proc & { ticks: number }): Proc => p;

const sum = (xs: number[]) => round(xs.reduce((a, b) => a + b, 0));

// ---------- the launch ----------

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const flag = (k: string) => process.argv.includes(`--${k}`);
const all = (k: string) => process.argv.flatMap((a, i) => (a === `--${k}` ? [process.argv[i + 1]] : []));

const RC = {
	// bash/zsh: who started this shell.
	posix: `printf '%s %s\\n' "$PPID" "$(cat /proc/$PPID/comm 2>/dev/null)" >> "$HOME/.bench-probes"\n`,
	fish: `set -l pp (string split ' ' (cat /proc/$fish_pid/stat))[4]\necho $pp (cat /proc/$pp/comm) >> $HOME/.bench-probes\n`,
};
function seedProbeRc(home: string) {
	mkdirSync(join(home, ".config/fish"), { recursive: true });
	for (const f of [".bash_profile", ".zshrc"]) writeFileSync(join(home, f), RC.posix);
	writeFileSync(join(home, ".config/fish/config.fish"), RC.fish);
}

/** settings.json `onboarding.done` (apps/desktop `onboarding/state.svelte.ts`): the welcome deck is gone on the next launch. */
function finishOnboarding(home: string) {
	const f = join(home, ".atlas/desktop/settings.json");
	const cur = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
	mkdirSync(dirname(f), { recursive: true });
	writeFileSync(f, JSON.stringify({ ...cur, onboarding: { ...cur.onboarding, done: true } }));
}

const killTagged = async (tag: string, root: number) => {
	for (const sig of ["TERM", "KILL"]) {
		const mine = owned(readProcs(), root, tag).map((p) => p.pid);
		if (!mine.length) return;
		for (const pid of mine) try { process.kill(pid, `SIG${sig}`); } catch {}
		for (let i = 0; i < 60 && owned(readProcs(), root, tag).length; i++) await Bun.sleep(50);
	}
};

/** Process state letter of `pid` (R, S, D, Z, T...), `?` when it is gone. */
const stateOf = (pid: number) => {
	try {
		const s = readFileSync(`/proc/${pid}/stat`, "utf8");
		return s.slice(s.lastIndexOf(")") + 2, s.lastIndexOf(")") + 3);
	} catch {
		return "?";
	}
};

/** What the next person needs when a run never got `app_ready` (#591), copied while the app is still as it was. */
function collectEvidence(dir: string, o: { trace: string; tag: string; root: number; why: string; marks: Record<string, number>; webSeenMs: number | null; windowMs: number | null; exited: number | null }) {
	mkdirSync(dir, { recursive: true });
	const keep = (from: string, to: string) => {
		try {
			if (existsSync(from)) writeFileSync(join(dir, to), readFileSync(from));
		} catch {}
	};
	keep(o.trace, "trace.txt");
	keep(`${o.trace}.out`, "app-output.txt");
	const base = process.env.ATLAS_HOSTD_DIR ?? join(process.env.HOME as string, ".atlas");
	keep(join(base, "desktop/app.log"), "app.log");
	keep(join(base, "desktop/stderr.log"), "stderr.log");
	const procs = readProcs();
	const mine = owned(procs, o.root, o.tag);
	// Web processes anywhere: one that lost its parent is as telling as one that is missing.
	const web = [...procs.values()].filter((p) => p.comm.startsWith("WebKit") && !mine.some((m) => m.pid === p.pid));
	const row = (p: P, own: boolean) => {
		let cmd = "";
		try {
			cmd = readFileSync(`/proc/${p.pid}/cmdline`, "utf8").replaceAll("\0", " ").trim();
		} catch {}
		return `${own ? "own  " : "other"} pid ${String(p.pid).padEnd(7)} ppid ${String(p.ppid).padEnd(7)} state ${stateOf(p.pid)} ${p.comm}  ${cmd.slice(0, 200)}`;
	};
	writeFileSync(join(dir, "processes.txt"), `${[...mine.map((p) => row(p, true)), ...web.map((p) => row(p, false))].join("\n")}\n`);
	// A page on screen or a blank one, and where the window is.
	spawnSync(["grim", join(dir, "window.png")], { timeout: 15_000 });
	const tree = spawnSync(["swaymsg", "-t", "get_tree"], { timeout: 5_000 }).stdout?.toString() ?? "";
	writeFileSync(join(dir, "sway-tree.json"), tree);
	const webAlive = mine.filter((p) => p.comm.startsWith("WebKitWebProces"));
	writeFileSync(
		join(dir, "evidence.json"),
		`${JSON.stringify(
			{
				why: o.why,
				exited: o.exited,
				appAlive: o.exited === null && mine.some((p) => p.pid === o.root),
				webProcessSeenMs: o.webSeenMs,
				webProcessAlive: webAlive.length > 0,
				webProcessStates: webAlive.map((p) => ({ pid: p.pid, state: stateOf(p.pid) })),
				windowMappedMs: o.windowMs,
				marks: o.marks,
				garbledTraceLines: existsSync(o.trace) ? garbledLines(readFileSync(o.trace, "utf8")) : [],
				loadAvg: readFileSync("/proc/loadavg", "utf8").trim(),
			},
			null,
			2,
		)}\n`,
	);
}

async function launch(o: { bin: string; envs: string[]; kind: Run["kind"]; trace: string; tag: string; readyTimeoutS: number; evidenceDir: string }) {
	const home = process.env.HOME as string;
	rmSync(o.trace, { force: true });
	rmSync(join(home, ".bench-probes"), { force: true });
	const env: Record<string, string> = { ...(process.env as Record<string, string>), ATLAS_BOOT_TRACE: o.trace, ATLAS_BENCH_TAG: o.tag, APPIMAGE_EXTRACT_AND_RUN: process.env.APPIMAGE_EXTRACT_AND_RUN ?? "1" };
	for (const e of o.envs) env[e.slice(0, e.indexOf("="))] = e.slice(e.indexOf("=") + 1);
	const t0 = performance.now();
	const child = Bun.spawn(["setsid", o.bin], { env, stdin: null, stdout: Bun.file(`${o.trace}.out`), stderr: Bun.file(`${o.trace}.out`) });
	const root = child.pid;
	let web: number | null = null;
	let win: number | null = null;
	let exited: number | null = null;
	void child.exited.then((c) => (exited = c));
	let lastSway = 0;
	const deadline = t0 + o.readyTimeoutS * 1000;
	let marks: Record<string, number> = {};
	while (performance.now() < deadline && exited === null) {
		const procs = readProcs();
		const tree = descendants(procs, root);
		if (web === null && tree.some((p) => p.comm.startsWith("WebKitWebProces"))) web = performance.now() - t0;
		if (win === null && performance.now() - lastSway > 100) {
			lastSway = performance.now();
			const t = spawnSync(["swaymsg", "-t", "get_tree"]).stdout.toString();
			const pids = new Set([...t.matchAll(/"pid": (\d+)/g)].map((m) => Number(m[1])));
			if (tree.some((p) => pids.has(p.pid))) win = performance.now() - t0;
		}
		marks = existsSync(o.trace) ? parseTrace(readFileSync(o.trace, "utf8")) : {};
		if ("js:ready:gate" in marks) break;
		await Bun.sleep(50);
	}
	const out = existsSync(`${o.trace}.out`) ? readFileSync(`${o.trace}.out`, "utf8") : "";
	// One more read: a mark written after the last poll must not make a run that did arrive look like one that did not.
	if (existsSync(o.trace)) marks = parseTrace(readFileSync(o.trace, "utf8"));
	const run: Run = {
		kind: o.kind,
		marks,
		spawnToWebProcessMs: web === null ? null : round(web),
		spawnToWindowMs: win === null ? null : round(win),
		readyMs: marks["js:ready:gate"] ?? null,
		shownBy: Object.keys(marks).find((k) => k.startsWith("show:"))?.slice(5) ?? null,
		loginShells: {},
	};
	if (run.readyMs === null) {
		const garbled = existsSync(o.trace) ? garbledLines(readFileSync(o.trace, "utf8")) : [];
		const why = explainMissingReady(marks, garbled, exited, o.readyTimeoutS);
		collectEvidence(o.evidenceDir, { trace: o.trace, tag: o.tag, root, why, marks, webSeenMs: run.spawnToWebProcessMs, windowMs: run.spawnToWindowMs, exited });
		run.evidence = o.evidenceDir;
		console.error(`app_ready never arrived: ${why}\nevidence kept in ${o.evidenceDir}; app output:\n${out.slice(-2000)}`);
	}
	return { run, root, child, stop: () => killTagged(o.tag, root) };
}

/** Flat-colour test of a 1/4 grab of the display: distinct colours and the commonest share (per mille). */
function paint(): { colours: number; topPermille: number } {
	const r = spawnSync(["grim", "-s", "0.25", "-t", "ppm", "-"]);
	const b = r.stdout;
	// P6\n<w> <h>\n255\n then raw rgb
	let off = 0;
	for (let nl = 0; nl < 3 && off < b.length; off++) if (b[off] === 10) nl++;
	const seen = new Map<number, number>();
	for (let i = off; i + 2 < b.length; i += 3) {
		const k = (b[i] << 16) | (b[i + 1] << 8) | b[i + 2];
		seen.set(k, (seen.get(k) ?? 0) + 1);
	}
	const n = [...seen.values()].reduce((a, c) => a + c, 0);
	return { colours: seen.size, topPermille: n ? Math.round((Math.max(...seen.values()) * 1000) / n) : 1000 };
}

const TICK = 100; // CLK_TCK is 100 on every Linux Atlas supports

async function idleRun(root: number, tag: string, settleS: number, cpuS: number): Promise<Idle> {
	await Bun.sleep(settleS * 1000);
	const first = snapshot(owned(readProcs(), root, tag));
	const t0 = performance.now();
	await Bun.sleep(cpuS * 1000);
	const secs = (performance.now() - t0) / 1000;
	const last = snapshot(owned(readProcs(), root, tag));
	const before = new Map(first.map((p) => [p.pid, p.ticks]));
	const byRole: Record<string, number> = {};
	for (const p of last) {
		const b = before.get(p.pid);
		if (b === undefined || cpuS === 0) continue;
		byRole[p.role] = round((byRole[p.role] ?? 0) + (((p.ticks - b) / TICK) / secs) * 100);
	}
	const top = first[0];
	const pssMb = sum(first.map((p) => p.pssMb));
	return {
		settleS,
		cpuS,
		processes: first.map(bare),
		rssMb: sum(first.map((p) => p.rssMb)),
		swapMb: sum(first.map((p) => p.swapMb)),
		pssMb,
		dominant: { role: top.role, pid: top.pid, pssMb: top.pssMb, share: round(top.pssMb / pssMb) },
		cpuPct: { total: sum(Object.values(byRole)), byRole },
		pssMbAfter: sum(last.map((p) => p.pssMb)),
		paint: paint(),
	};
}

function machine(webpid: number | null) {
	const sh = (c: string[]) => spawnSync(c).stdout.toString().trim();
	const cpu = /model name\s*:\s*(.+)/.exec(readFileSync("/proc/cpuinfo", "utf8"))?.[1] ?? "unknown";
	const os = /PRETTY_NAME="?([^"\n]+)/.exec(existsSync("/etc/os-release") ? readFileSync("/etc/os-release", "utf8") : "")?.[1] ?? "unknown";
	let driver = "unknown";
	if (webpid) {
		try {
			const maps = readFileSync(`/proc/${webpid}/maps`, "utf8");
			driver = [...new Set([...maps.matchAll(/\/(\w+)_dri\.so|\/(libgallium[\w.-]*\.so)/g)].map((m) => m[1] ?? m[2]))].join(",") || "none loaded";
		} catch {}
	}
	return {
		os,
		kernel: sh(["uname", "-r"]),
		cpu,
		cores: Number(sh(["nproc"])),
		memMb: Math.round(Number(/MemTotal:\s+(\d+)/.exec(readFileSync("/proc/meminfo", "utf8"))?.[1]) / 1024),
		display: "headless sway (wlroots, pixman); no real display",
		gpu: sh(["sh", "-c", "lspci 2>/dev/null | grep -iE 'vga|3d' | head -1 | cut -d: -f3-"]) || "none visible",
		gpuDriverLoadedByWebKit: driver,
		webkit: sh(["pkg-config", "--modversion", "webkit2gtk-4.1"]) || "unknown",
		/** 1-minute load average when the bench started: a number measured on a busy machine says so. */
		loadAvg1: Number(readFileSync("/proc/loadavg", "utf8").split(" ")[0]),
	};
}

async function inner() {
	const bin = process.argv[process.argv.indexOf("--inner") + 1];
	const runs = Number(arg("runs", "5"));
	const settle = Number(arg("idle-settle", "30"));
	const cpuS = Number(arg("idle-cpu", "60"));
	const readyS = Number(arg("ready-timeout", "90"));
	const envs = all("env");
	const out = arg("out", "linux-startup-out/report.json") as string;
	const tmp = mkdtempSync(join(tmpdir(), "linux-startup-"));
	seedProbeRc(process.env.HOME as string);
	const results: Run[] = [];
	let failed = false;
	const one = async (kind: Run["kind"], i: number) => {
		const tag = `${process.pid}-${i}`;
		const trace = join(tmp, `trace-${i}.txt`);
		const l = await launch({ bin, envs, kind, trace, tag, readyTimeoutS: readyS, evidenceDir: join(dirname(resolve(out)), "evidence", `run-${i + 1}`) });
		if (l.run.readyMs !== null) await Bun.sleep(4000); // the hostd spawn and the login-shell probes finish after app_ready
		l.run.marks = existsSync(trace) ? parseTrace(readFileSync(trace, "utf8")) : l.run.marks;
		const log = join(process.env.HOME as string, ".bench-probes");
		l.run.loginShells = existsSync(log) ? countProbes(readFileSync(log, "utf8")) : {};
		return l;
	};
	for (let i = 0; i < runs; i++) {
		const l = await one(i === 0 ? "cold" : "warm", i);
		results.push(l.run);
		console.error(`run ${i + 1}/${runs} ${l.run.kind}: app_ready ${l.run.readyMs ?? "NEVER"} ms, shown by ${l.run.shownBy}, window ${l.run.spawnToWindowMs} ms, login shells ${JSON.stringify(l.run.loginShells)}`);
		if (l.run.readyMs === null) failed = true;
		await l.stop();
		// The first launch of a fresh HOME shows the welcome deck, which animates at 60 fps (under llvmpipe that is ~50% of a core, the
		// whole of the "idle" CPU of a fresh install). Everything after it, idle included, is the app of someone who finished onboarding.
		if (i === 0) finishOnboarding(process.env.HOME as string);
		if (failed) break;
	}
	let idle: Idle | null = null;
	let mach = machine(null);
	if (!failed && cpuS > 0) {
		const l = await launch({ bin, envs, kind: "warm", trace: join(tmp, "trace-idle.txt"), tag: `${process.pid}-idle`, readyTimeoutS: readyS, evidenceDir: join(dirname(resolve(out)), "evidence", "idle") });
		if (l.run.readyMs === null) failed = true;
		else {
			const wp = descendants(readProcs(), l.root).find((p) => p.comm.startsWith("WebKitWebProces"));
			mach = machine(wp?.pid ?? null);
			idle = await idleRun(l.root, `${process.pid}-idle`, settle, cpuS);
		}
		await l.stop();
	}
	rmSync(tmp, { recursive: true, force: true });
	const report: Report = {
		when: new Date().toISOString(),
		binary: basename(bin),
		format: bin.endsWith(".AppImage") ? "appimage" : "binary",
		build: process.env.ATLAS_BENCH_BUILD ?? "debug, bundled frontend (cargo build --features tauri/custom-protocol)",
		machine: mach,
		runs: results,
		summary: summarize({ runs: results, idle }),
		idle,
		budget: BUDGET,
	};
	mkdirSync(dirname(out), { recursive: true });
	writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
	console.log(render(report));
	const bad = judge(report);
	if (failed && !bad.length) bad.push("a run failed");
	if (bad.length && !flag("no-check")) {
		console.error(`startup budget exceeded:\n  ${bad.join("\n  ")}`);
		process.exit(1);
	}
}

export function render(r: Report): string {
	const s = summarize(r);
	const l: string[] = [`cold: app_ready ${s.coldReadyMs} ms`];
	if (s.warmReady) l.push(`warm (${r.runs.filter((x) => x.kind === "warm").length}): app_ready p50 ${s.warmReady.p50} p95 ${s.warmReady.p95} ms; window p50 ${s.warmWindow?.p50} ms; web process p50 ${s.warmWebProcess?.p50} ms`);
	if (r.idle) {
		l.push(`idle after ${r.idle.settleS} s: PSS ${r.idle.pssMb} MB, RSS ${r.idle.rssMb} MB; largest ${r.idle.dominant.role} ${r.idle.dominant.pssMb} MB (${Math.round(r.idle.dominant.share * 100)}%)`);
		for (const p of r.idle.processes) l.push(`  ${p.role.padEnd(12)} pid ${String(p.pid).padEnd(7)} PSS ${String(p.pssMb).padStart(7)} MB  RSS ${String(p.rssMb).padStart(7)} MB`);
		l.push(`idle CPU over ${r.idle.cpuS} s: ${r.idle.cpuPct.total}% of one core ${JSON.stringify(r.idle.cpuPct.byRole)}`);
	}
	return l.join("\n");
}

async function main() {
	const judging = arg("judge");
	if (judging) {
		const saved = JSON.parse(readFileSync(judging, "utf8")) as Report;
		console.log(render(saved));
		const bad = judge(saved);
		if (bad.length && !flag("no-check")) {
			console.error(`startup budget exceeded:\n  ${bad.join("\n  ")}`);
			process.exit(1);
		}
		return;
	}
	if (flag("inner")) return inner();
	// Outside: the private display and bus (scripts/linux-headless.sh), then this script again inside them.
	const here = import.meta.dir;
	const bin = process.argv.at(-1) as string;
	if (process.argv.length < 3 || !existsSync(bin)) {
		console.error("usage: linux-startup-bench.ts [options] <atlas-desktop binary | .AppImage>");
		process.exit(2);
	}
	const rest = process.argv.slice(2, -1);
	const script = `set -uo pipefail
here=$1; size=$2; shift 2
. "$here/linux-headless.sh"
headless_prepare "$size"
headless_session bash -s "$here" "$work" "$@" <<'INNER'
here=$1; work=$2; shift 2
. "$here/linux-headless.sh"
headless_up || exit 3
"$@"; code=$?
headless_down
exit $code
INNER
`;
	const r = Bun.spawnSync(["bash", "-c", script, "bench", here, arg("size", "1440x900") as string, process.execPath, join(here, "linux-startup-bench.ts"), "--inner", resolve(bin), ...rest], { stdin: "inherit", stdout: "inherit", stderr: "inherit" });
	if (r.exitCode === 0 && arg("save")) {
		const out = arg("out", "linux-startup-out/report.json") as string;
		mkdirSync(dirname(resolve(arg("save") as string)), { recursive: true });
		writeFileSync(resolve(arg("save") as string), readFileSync(out));
	}
	process.exit(r.exitCode ?? 1);
}

if (import.meta.main) await main();
