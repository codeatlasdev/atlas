#!/usr/bin/env bun
/**
 * Whole-app flicker sweep: drives every surface of the REAL app (an agent instance, never the user's window, never
 * OS input) while the in-app recorder films it at 60 fps, then judges the recordings pixel by pixel and exits 1 on
 * any flicker.
 *
 *   bun scripts/flicker-sweep.ts [--name sweep] [--only boot,settings] [--out /tmp/flicker-sweep] [--keep] [--list]
 *   bun scripts/flicker-sweep.ts --judge <file.mov> [--surface name]       (judge one recording, no app)
 *
 * One recording per surface, so a finding names its surface. Per recording: frames are decoded with their own
 * presentation times (the recorder is variable frame rate: no `fps=` resample), reduced to a luminance grid
 * (`apps/desktop/src/lib/debug/flickerSweep.ts`, unit-tested), and judged for single-frame blips, blank frames and
 * BT.1702 flashing. The report (`report.json`, `report.md`) lists every surface covered and, for each finding, the
 * timestamp and a crop of the frames around it.
 *
 * Needs Screen Recording permission (the recorder is ScreenCaptureKit). Heavy: run it alone, like gesture-bench.
 */
import { $ } from "bun";
import { loadavg } from "node:os";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	analyze,
	CLEAN_PROBE,
	cropRect,
	frameStarts,
	GRID_H,
	GRID_W,
	gridOf,
	verdict,
	type Frame,
	type SurfaceResult,
} from "../apps/desktop/src/lib/debug/flickerSweep.ts";
import { flashWindows } from "../studio/src/lint/checks-video.ts";
import { fixtures } from "./flicker-sweep-fixtures.ts";
import { SURFACES, type Drive } from "./flicker-sweep-surfaces.ts";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const has = (k: string) => process.argv.includes(`--${k}`);
const name = arg("name", "sweep")!;
const out = arg("out", "/tmp/flicker-sweep")!;
const only = arg("only")?.split(",");
const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
const sock = `/private/tmp/atlas-agent-${name}/debug.sock`;
const env = { ...process.env, ATLAS_DEBUG_SOCK: sock };

const debug = async (...a: string[]) => (await $`${atlas} desktop debug ${a}`.env(env).quiet()).text();
const evalJs = async <T = unknown>(js: string): Promise<T> => JSON.parse(await debug("eval", js));

/** Decode width: an even multiple of the grid, enough to see a 1-cell change without decoding retina frames in full. */
const DEC_W = GRID_W * 8;
const DEC_H = GRID_H * 8;

/** The recording's frames: grid + own presentation time. */
async function decode(file: string): Promise<{ frames: Frame[]; w: number; h: number }> {
	const info: { streams: Array<{ width: number; height: number }> } = JSON.parse(
		await $`ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of json ${file}`.text(),
	);
	const probe = info.streams[0]!;
	const times = frameStarts(
		await $`ffprobe -v error -select_streams v:0 -show_entries frame=pts_time -of csv=p=0 ${file}`.text(),
	);
	// no fps filter: one decoded frame per stored frame, in the file's own variable rate (-vsync passthrough)
	const raw = new Uint8Array(
		await $`ffmpeg -v error -i ${file} -fps_mode passthrough -vf scale=${DEC_W}:${DEC_H}:flags=area -f rawvideo -pix_fmt rgb24 -`
			.quiet()
			.arrayBuffer(),
	);
	const size = DEC_W * DEC_H * 3;
	const n = Math.floor(raw.length / size);
	const frames: Frame[] = [];
	for (let i = 0; i < n; i++)
		frames.push({ t: times[i] ?? (times[times.length - 1] ?? 0), grid: gridOf(raw.subarray(i * size, (i + 1) * size), DEC_W, DEC_H) });
	return { frames, w: probe.width, h: probe.height };
}

/**
 * A busy host drops frames, and a dropped frame reads as flicker, so the sweep starts only when the 1-minute load is
 * under `LOAD_MAX` (the 5-minute figure trails a spike by minutes and never opens on a working machine). It waits once,
 * for at most `LOAD_WAIT_S`, then goes on and records which surfaces were filmed hot: the recorder, the app and the fixture stacks
 * load the machine themselves, so waiting again before every surface only adds minutes. A finding made while hot says so.
 */
const LOAD_MAX = Number(arg("load-max", "4"));
const LOAD_WAIT_S = 120;
const hotSurfaces: string[] = [];
async function settle() {
	for (let i = 0; i < LOAD_WAIT_S / 5 && loadavg()[0]! >= LOAD_MAX; i++) await Bun.sleep(5000);
	if (loadavg()[0]! >= LOAD_MAX) console.error(`warning: load ${loadavg()[0]!.toFixed(1)} >= ${LOAD_MAX}; frame drops on a busy host can read as flicker`);
}

/** Frames around a finding, cropped to its region (and the whole frame next to it), for the report. */
async function crops(file: string, dir: string, w: number, h: number, frame: number, box: Parameters<typeof cropRect>[0], tag: string) {
	mkdirSync(dir, { recursive: true });
	const r = cropRect(box, w, h, 2);
	const files: string[] = [];
	for (const k of [frame - 1, frame, frame + 1]) {
		if (k < 0) continue;
		const f = join(dir, `${tag}-f${k}.png`);
		await $`ffmpeg -v error -y -i ${file} -fps_mode passthrough -vf ${`select=eq(n\\,${k}),crop=${r.w}:${r.h}:${r.x}:${r.y}`} -frames:v 1 ${f}`
			.quiet()
			.nothrow();
		if (existsSync(f)) files.push(f);
	}
	return files;
}

async function judge(file: string, surface: string, dir: string): Promise<SurfaceResult & { crops: Record<string, string[]> }> {
	const { frames, w, h } = await decode(file);
	const findings = frames.length >= 3 ? analyze(frames, flashWindows) : { blips: [], blanks: [], flashes: [] };
	const shots: Record<string, string[]> = {};
	for (const b of findings.blips) shots[`blip@${b.t.toFixed(3)}`] = await crops(file, dir, w, h, b.frame, b.box, `${surface}-blip-${b.frame}`);
	for (const b of findings.blanks)
		shots[`${b.kind}@${b.t.toFixed(3)}`] = await crops(file, dir, w, h, b.frame, { x0: 0, y0: 0, x1: GRID_W, y1: GRID_H }, `${surface}-${b.kind}-${b.frame}`);
	return { surface, frames: frames.length, findings, crops: shots };
}

async function main() {
	if (has("list")) {
		for (const s of SURFACES) console.log(`${s.id.padEnd(22)} ${s.covers}`);
		return;
	}
	const judging = arg("judge");
	if (judging) {
		const dir = join(out, "judge");
		const r = await judge(judging, arg("surface", "recording")!, dir);
		const v = verdict([r]);
		for (const l of v.lines) console.error(l);
		console.log(JSON.stringify({ surface: r.surface, frames: r.frames, ...r.findings }, null, 2));
		process.exit(v.code);
	}

	rmSync(out, { recursive: true, force: true });
	mkdirSync(out, { recursive: true });
	const started = (await $`ls ${sock}`.quiet().nothrow()).exitCode === 0;
	if (started) throw new Error(`an agent instance named "${name}" is already running; stop it first (bun run desktop:agent -- --stop --name ${name})`);
	const results: Array<SurfaceResult & { crops: Record<string, string[]> }> = [];
	const fx = fixtures();
	try {
		await settle();
		await $`bun run desktop:agent -- --name ${name} --scratch`.cwd(root).quiet();
		const drive: Drive = {
			eval: evalJs,
			sleep: Bun.sleep,
			click: async (selector) => {
				const r: { clicked: boolean; reason?: string } = JSON.parse(await debug("click", selector));
				if (!r.clicked) throw new Error(`click ${selector}: ${r.reason}`);
			},
			type: (selector, text, delay = 40) => debug("type", selector, text, "--delay", String(delay)).then(() => undefined),
			resize: (w, h) => debug("resize", String(w), String(h)).then(() => undefined),
			fixtures: fx,
		};
		for (const root of fx.roots) await evalJs(`await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(root)} }); return 1`);
		await evalJs(`await window.__atlas.store.refresh(); return window.__atlas.store.projects.length`);
		// a real worktree (the sidebar's worktree rows and their menu exist only then), made through the app's own path like
		// `atlas wt create`; the blank terminal it opens is closed again so no native view is left over the pane
		const wt = await evalJs<string | null>(
			`const a = window.__atlas; const repo = a.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(fx.main)}); return (await a.worktrees.create({ repoId: repo.id, name: "checkout-flow", runSetup: false, agent: "blank" })) ?? null`,
		);
		if (!wt) throw new Error("the fixture worktree was not created (see: atlas desktop debug logs)");
		await Bun.sleep(2000);
		await evalJs(`const a = window.__atlas; for (const t of a.ws.tabsOf(${JSON.stringify(wt)}).filter(t => t.kind === "terminal")) a.ws.closeTab(t.id, true, true); return 1`);
		await Bun.sleep(1000);
		for (const s of SURFACES) {
			if (only && !only.includes(s.id)) continue;
			const file = join(out, `${s.id}.mov`);
			console.error(`▶ ${s.id}: ${s.covers}`);
			await s.setup?.(drive);
			// the previous surface must have left the bare app: otherwise this recording films that leftover
			const before = await evalJs<string[]>(`return ${CLEAN_PROBE}`);
			if (before.length) throw new Error(`${s.id} starts with an open dialog (${before.join(", ")}): the previous surface left it open`);
			if (loadavg()[0]! >= LOAD_MAX) hotSurfaces.push(s.id);
			// the recorder runs for `seconds`; the driver runs concurrently on its own bridge connection
			const rec = $`${atlas} desktop debug record ${file} --seconds ${String(s.seconds)} --fps 60`.env(env).quiet();
			await Bun.sleep(600); // the recorder needs a beat before the first frame
			let broke: string | undefined;
			try {
				await s.run(drive);
			} catch (e) {
				broke = e instanceof Error ? e.message : String(e);
			}
			if (!broke) {
				const open = await evalJs<string[]>(`return ${CLEAN_PROBE}`);
				if (open.length) broke = `left a dialog open over the app (${open.join(", ")})`;
			}
			if (!broke)
				for (const [what, probe] of s.expects ?? []) {
					const found = await evalJs<boolean>(probe.startsWith("js:") ? `return !!(${probe.slice(3)})` : `return [...document.querySelectorAll(${JSON.stringify(probe)})].some(e => e.offsetParent !== null)`);
					if (!found) broke = `expected on screen at the end: ${what} (${probe})`;
				}
			await rec; // always: a recorder left running would collide with the next surface
			if (broke) {
				// a surface the sweep could not drive was not exercised: that fails the sweep like a flicker would
				console.error(`  could not drive ${s.id}: ${broke}`);
			}
			const r = await judge(file, s.id, join(out, "crops"));
			results.push({ ...r, undriven: broke?.split("\n")[0] });
			console.error(`  ${r.frames} frames, ${r.findings.blips.length} blips, ${r.findings.blanks.length} blank, ${r.findings.flashes.length} flashing`);
		}
	} finally {
		if (!has("keep")) {
			await $`bun run desktop:agent -- --stop --name ${name}`.cwd(root).quiet().nothrow();
			fx.cleanup();
		}
	}
	const v = verdict(results);
	writeFileSync(join(out, "report.json"), JSON.stringify({ when: new Date().toISOString(), loaded: hotSurfaces, surfaces: results, exit: v.code }, null, 2));
	writeFileSync(
		join(out, "report.md"),
		[
			"# Flicker sweep",
			"",
			"| surface | frames | blips | blank | flashing |",
			"|---|---:|---:|---:|---:|",
			...results.map((r) => `| ${r.surface} | ${r.frames} | ${r.findings.blips.length} | ${r.findings.blanks.length} | ${r.findings.flashes.length} |`),
			"",
			...(v.lines.length ? ["## Findings", "", ...v.lines.map((l) => `- ${l}`), "", ...results.flatMap((r) => Object.entries(r.crops).map(([k, fs]) => `- ${r.surface} ${k}: ${fs.join(", ")}`))] : ["No flicker."]),
			"",
		].join("\n"),
	);
	console.error(`report: ${join(out, "report.md")}`);
	if (v.code) {
		console.error(`flicker found${hotSurfaces.length ? ` (the host was loaded while recording ${hotSurfaces.join(", ")}: re-run on a quiet machine before trusting those)` : ""}:\n  ${v.lines.join("\n  ")}`);
		process.exit(1);
	}
	console.error(`${results.length} surfaces, no flicker`);
}

await main();
