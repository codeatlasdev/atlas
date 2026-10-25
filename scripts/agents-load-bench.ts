// bun scripts/agents-load-bench.ts [--name loadbench] [--agents 7] [--seconds 30] [--warmup 5] [--hz 30] [--lines 40]
//     [--hook-every 150] [--title-hz 10] [--save docs/perf/x.json] [--no-check] [--keep]
// bun scripts/agents-load-bench.ts --judge docs/perf/x.json [--no-check]      (re-judge a saved report, no app)
//
// #110, first half: what the app does with N busy agents. It starts an agent instance (never the user's app), opens N
// terminal tabs in one worktree, each running `scripts/agents-load-gen.ts` (a 30 Hz full-screen TUI redraw, streaming
// lines, a spinner title and periodic hook posts: what omp does), lets them run for --seconds, and measures
//  - the longest main-thread stall per second (MessageChannel ping-pong; WKWebView reports no `longtask`) and rAF gaps,
//  - frames presented per terminal (`term_stats`) against the bytes its program printed,
//  - RSS of the app, hostd and the WebContent process over the run,
//  - `agent://status` and `host://event` events the page received (an event storm is a freeze too).
// Exits 1 when `agents-load-judge.ts` BUDGET is exceeded, or when nothing was measured (a crashed app is not a pass).
// The instance is stopped explicitly at the end, also on failure (`--keep` leaves it). Run it alone, wrapped in
// `scripts/heavy.sh`: it is a heavy job (N renderers + N bun processes). Method and numbers: docs/terminal.md.
import { $ } from "bun";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BUDGET, check, parseLoad, summarize, type Load } from "./agents-load-judge.ts";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const flag = (k: string) => process.argv.includes(`--${k}`);
const name = arg("name", "loadbench")!;
const agents = Number(arg("agents", "7"));
const seconds = Number(arg("seconds", "30"));
const warmup = Number(arg("warmup", "5"));
const gen = ["--hz", arg("hz", "30"), "--lines", arg("lines", "40"), "--hook-every", arg("hook-every", "150"), "--title-hz", arg("title-hz", "10")];
const save = arg("save");
const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
const dir = `/private/tmp/atlas-agent-${name}`;
const sock = `${dir}/debug.sock`;
/** Where each generator writes the bytes it printed (inside this instance's own dir, removed with it). */
const counts = `${dir}/loadgen-counts`;
const env = { ...process.env, ATLAS_DEBUG_SOCK: sock };

const debug = async (...a: string[]) => (await $`${atlas} desktop debug ${a}`.env(env).quiet()).text();
const evalJs = async (js: string) => JSON.parse(await debug("eval", js));

/** Resident MB summed over the processes of this instance whose command line matches `pattern`. */
async function rss(pattern: string): Promise<number> {
	const ps = (await $`ps -axo rss=,command=`.quiet().nothrow()).text();
	let kb = 0;
	for (const l of ps.split("\n")) if (l.includes(pattern) && l.includes(dir)) kb += Number(l.trim().split(/\s+/)[0]) || 0;
	return Math.round(kb / 1024);
}

/** App, hostd and WebContent of this instance. */
async function rssNow() {
	return { app: await rss("atlas-desktop"), hostd: await rss("hostd"), web: await rss("WebContent") };
}

/** Page side: a stall sampler, rAF gaps and event counters, started once and read at the end. */
const START = `
const L = (window.__load = { stalls: [], gaps: [], status: 0, host: 0, stop: false });
let last = performance.now();
const raf = (t) => { L.gaps.push(t - last); last = t; if (!L.stop) requestAnimationFrame(raf); };
requestAnimationFrame(raf);
// longest gap between two MessageChannel turns, reported once a second
const ch = new MessageChannel(); let prev = performance.now(), worst = 0, win = performance.now();
ch.port1.onmessage = () => { const n = performance.now(); worst = Math.max(worst, n - prev); prev = n;
  if (n - win >= 1000) { L.stalls.push(worst); worst = 0; win = n; }
  if (!L.stop) ch.port2.postMessage(0); };
ch.port2.postMessage(0);
const T = window.__TAURI_INTERNALS__;
for (const [event, key] of [["agent://status", "status"], ["host://event", "host"]])
  await T.invoke("plugin:event|listen", { event, target: { kind: "Any" }, handler: T.transformCallback(() => L[key]++) });
return 1;`;

const termStats = async () => (await evalJs(`return await window.__TAURI_INTERNALS__.invoke("term_stats")`)) as Record<string, { frames: number }>;

/** Bytes each generator printed, by pane key (tab id): the last number it wrote. */
function printed(): Record<string, number> {
	const out: Record<string, number> = {};
	for (const f of readdirSync(counts)) out[f.replace(/\.bytes$/, "")] = Number(readFileSync(join(counts, f), "utf8")) || 0;
	return out;
}

async function main() {
	const started = (await $`ls ${sock}`.quiet().nothrow()).exitCode === 0;
	if (!started) await $`bun run desktop:agent -- --name ${name} --scratch`.cwd(root).quiet();
	try {
		rmSync(counts, { recursive: true, force: true });
		mkdirSync(counts, { recursive: true });
		// a scratch project with an atlas.yaml: one worktree to hold N terminals
		const fx = "/private/tmp/agents-load-fixture";
		mkdirSync(fx, { recursive: true });
		writeFileSync(join(fx, "atlas.yaml"), 'name: agents-load\nservices:\n  web:\n    command: "sleep 100000"\n    port: 20999\n');
		await evalJs(`await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(fx)} }); await window.__atlas.store.refresh(); return 1`);
		const wt = (await evalJs(`return window.__atlas.ws.doc.selected ?? null`)) as string | null;
		if (!wt) throw new Error("no worktree to open terminals in (is the instance on a project view?)");
		const argv = JSON.stringify(["bun", join(root, "scripts/agents-load-gen.ts"), ...gen]);
		const genEnv = JSON.stringify({ LOADGEN_COUNT_DIR: counts });
		// agent: "omp" makes each tab an agent tab, so status events, the sidebar and the tray react like with real agents
		const tabs = (await evalJs(
			`const out = []; for (let i = 0; i < ${agents}; i++) { const t = await window.__atlas.ws.newTerminal(${JSON.stringify(wt)}, { argv: ${argv}, env: ${genEnv}, agent: "omp", label: "load " + i }); if (t) out.push(t.id); } return out`,
		)) as string[];
		if (tabs.length < agents) throw new Error(`opened ${tabs.length} of ${agents} terminals`);
		await evalJs(START);
		await Bun.sleep(warmup * 1000);
		const [before, bytes0] = [await termStats(), printed()];
		await evalJs(`const L = window.__load; L.stalls.length = 0; L.gaps.length = 0; L.status = 0; L.host = 0; return 1`);
		const rssMb = { app: [] as number[], hostd: [] as number[], web: [] as number[] };
		for (let s = 0; s < seconds; s++) {
			const r = await rssNow();
			for (const k of ["app", "hostd", "web"] as const) rssMb[k].push(r[k]);
			await Bun.sleep(1000);
		}
		const [after, bytes1] = [await termStats(), printed()];
		const page = (await evalJs(`const L = window.__load; L.stop = true; return { stalls: L.stalls, gaps: L.gaps, status: L.status, host: L.host }`)) as {
			stalls: number[];
			gaps: number[];
			status: number;
			host: number;
		};
		const load: Load = {
			stallMs: page.stalls,
			frameGapMs: page.gaps,
			terms: tabs.map((tab) => ({
				tab,
				framesStart: before[tab]?.frames ?? 0,
				framesEnd: after[tab]?.frames ?? 0,
				bytes: (bytes1[tab] ?? 0) - (bytes0[tab] ?? 0),
			})),
			rssMb,
			statusEvents: page.status,
			hostEvents: page.host,
			seconds,
		};
		const out = { when: new Date().toISOString(), agents, budget: BUDGET, ...summarize(load), load };
		console.log(JSON.stringify({ ...out, load: undefined }, null, 2));
		if (save) writeFileSync(join(root, save), `${JSON.stringify(out, null, 2)}\n`);
		verdict(load);
	} finally {
		// explicit, not a trap: a persistent shell never fires EXIT traps and the instance would leak
		if (!started && !flag("keep")) await $`bun run desktop:agent -- --stop --name ${name}`.cwd(root).quiet().nothrow();
	}
}

/** Exit 1 (message on stderr) when the budget is exceeded, unless `--no-check`. */
function verdict(load: Load) {
	const bad = check(load);
	if (bad.length && !flag("no-check")) {
		console.error(`agents load budget exceeded:\n  ${bad.join("\n  ")}`);
		process.exit(1);
	}
}

const judging = arg("judge");
if (judging) {
	const parsed = parseLoad(JSON.parse(readFileSync(judging, "utf8")));
	if ("error" in parsed) {
		console.error(`${judging}: ${parsed.error}`);
		process.exit(1);
	}
	verdict(parsed.load);
} else await main();
