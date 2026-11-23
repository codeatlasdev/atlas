// bun scripts/linux-terminal-soak.ts --sock <debug.sock> --atlas <atlas binary> [--cycles 10] [--flood-mb 50] [--budget 100] [--settle 5] [--out FILE]
//
// Does a closed terminal give its memory back (cause 6 of docs/perf/linux-webkit-study.md)? Open a terminal that prints
// FLOOD-MB of text, close the tab, wait SETTLE seconds, sample the web process's PSS (smaps_rollup); CYCLES times.
// The first sample is the idle baseline (the project open, no terminal). Exit 1 when the PSS after the last close is more
// than BUDGET MB above the baseline. Against an agent instance only (a debug build: `window.__atlas`).
//
// Every wait is in this script (a file the flood writes, a sleep), never inside a page evaluation: the study's `soak.sh` held a
// 5 s `await` inside `debug eval` per cycle and stalled on a busy machine. `live` is what the page still holds after the
// close: `.xterm` elements and canvases (xterm's, and its WebGL renderer's); anything but 0 is a leak whatever the PSS says.
import { spawnSync } from "bun";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// ---------- pure: tested in linux-terminal-soak.test.ts ----------

export type Point = { label: string; webPssMb: number; live?: { xterm: number; canvas: number } };

/** Why a soak run fails (empty = passes): memory over the budget after the last close, or xterm objects the page kept. */
export function judgeSoak(points: Point[], budgetMb: number): string[] {
	const [base] = points;
	const last = points[points.length - 1];
	if (!base || points.length < 2) return ["no cycle was sampled"];
	const out: string[] = [];
	const grown = Math.round((last.webPssMb - base.webPssMb) * 10) / 10;
	if (grown > budgetMb) out.push(`web PSS ${last.webPssMb} MB after ${last.label} is ${grown} MB over the ${base.webPssMb} MB idle baseline (budget ${budgetMb} MB)`);
	for (const p of points) if (p.live && (p.live.xterm > 0 || p.live.canvas > 0)) out.push(`${p.label}: the page still holds ${p.live.xterm} .xterm and ${p.live.canvas} canvas element(s) after the tab closed`);
	return out;
}

// ---------- /proc ----------

function descendants(root: number): number[] {
	const kids = new Map<number, number[]>();
	for (const d of readdirSync("/proc")) {
		if (!/^\d+$/.test(d)) continue;
		try {
			const s = readFileSync(`/proc/${d}/stat`, "utf8");
			const ppid = Number(s.slice(s.lastIndexOf(")") + 2).split(" ")[1]);
			kids.set(ppid, [...(kids.get(ppid) ?? []), Number(d)]);
		} catch {}
	}
	const out = [root];
	for (let i = 0; i < out.length; i++) out.push(...(kids.get(out[i]) ?? []));
	return out;
}

/** PSS in MB of every WebKitWebProcess under `root`. */
function webPssMb(root: number): number {
	let kb = 0;
	for (const pid of descendants(root)) {
		try {
			if (!readFileSync(`/proc/${pid}/comm`, "utf8").startsWith("WebKitWebProces")) continue;
			kb += Number(/^Pss:\s+(\d+) kB/m.exec(readFileSync(`/proc/${pid}/smaps_rollup`, "utf8"))?.[1] ?? 0);
		} catch {}
	}
	return Math.round((kb / 1024) * 10) / 10;
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

function homeOf(pid: number): string {
	for (const kv of readFileSync(`/proc/${pid}/environ`, "latin1").split("\0")) if (kv.startsWith("HOME=")) return kv.slice(5);
	return "/tmp";
}

async function main() {
	const sock = arg("sock") as string;
	const atlas = arg("atlas", "target/debug/atlas") as string;
	const cycles = Number(arg("cycles", "10"));
	const mb = Number(arg("flood-mb", "50"));
	const budget = Number(arg("budget", "100"));
	const settle = Number(arg("settle", "5"));
	const out = arg("out");
	if (!sock || !existsSync(sock)) throw new Error("--sock must be the debug socket of a running agent instance");
	const info = JSON.parse(debug(atlas, sock, "info"));
	if (info.agent !== true) throw new Error("not an agent instance: refusing to measure someone's real app");
	const root = info.pid as number;
	const run = (js: string) => JSON.parse(debug(atlas, sock, "eval", js));
	const tmp = join(homeOf(root), ".terminal-soak");
	mkdirSync(tmp, { recursive: true });

	// A project to open terminals in: a git repo with one commit.
	const project = join(tmp, "soak-project");
	if (!existsSync(join(project, ".git"))) {
		mkdirSync(project, { recursive: true });
		writeFileSync(join(project, "a.txt"), "a\n");
		for (const a of [["init", "-q", "-b", "main"], ["add", "-A"], ["commit", "-q", "-m", "x"]]) spawnSync(["git", "-c", "user.name=soak", "-c", "user.email=soak@example.com", ...a], { cwd: project });
	}
	const wt = run(`const a = window.__atlas; await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(project)} }); await a.store.refresh(); const id = a.store.projects.find((p) => p.root === ${JSON.stringify(project)}).id; a.shell.select(a.shell.landing(id)); await new Promise((r) => setTimeout(r, 2000)); return id`) as string;
	await Bun.sleep(10_000);

	const points: Point[] = [{ label: "baseline", webPssMb: webPssMb(root) }];
	const emit = (p: Point) => {
		const line = JSON.stringify({ ...p, load: Number(readFileSync("/proc/loadavg", "utf8").split(" ")[0]), when: new Date().toISOString() });
		console.log(line);
		if (out) appendFileSync(out, `${line}\n`);
	};
	emit(points[0]);
	for (let i = 1; i <= cycles; i++) {
		const marker = join(tmp, `flooded-${i}`);
		const text = "2026-11-22 12:00:00 INFO request handled path=/api/v1/items status=200 ms=12";
		const tab = run(`const t = await window.__atlas.ws.newTerminal(${JSON.stringify(wt)}, { argv: ["sh", "-c", ${JSON.stringify(`yes "${text}" | head -c ${mb}M; touch ${marker}; sleep 3600`)}], label: "soak" }); return t.id`) as string;
		// The program has printed it all (the page may still be parsing: the close must work on a busy page too).
		for (let t = 0; !existsSync(marker); t++) {
			if (t > 600) throw new Error(`cycle ${i}: the flood did not finish in 5 minutes`);
			await Bun.sleep(500);
		}
		await Bun.sleep(2000);
		const open: Point = { label: `open-${i}`, webPssMb: webPssMb(root) };
		emit(open);
		run(`window.__atlas.ws.closeTab(${JSON.stringify(tab)}); return 1`);
		await Bun.sleep(settle * 1000);
		const live = run(`return { xterm: document.querySelectorAll(".xterm").length, canvas: document.querySelectorAll("canvas").length }`) as Point["live"];
		const p: Point = { label: `closed-${i}`, webPssMb: webPssMb(root), live };
		points.push(p);
		emit(p);
	}
	await Bun.sleep(30_000);
	const final: Point = { label: "final", webPssMb: webPssMb(root), live: points[points.length - 1].live };
	points.push(final);
	emit(final);
	const bad = judgeSoak(points, budget);
	if (bad.length) (console.error(bad.join("\n")), process.exit(1));
}

if (import.meta.main) await main();
