#!/usr/bin/env bun
/**
 * Terminal bench for coding-agent TUIs: replays six flood patterns that imitate what Claude Code, omp and codex print
 * into the GPU terminal of an agent instance (never the user's app), films the terminal at 60 fps, and judges what the
 * compositor showed, how the renderer paced itself, and whether scrolling lost anything.
 *
 *   bun scripts/term-agent-bench.ts [--name termbench] [--seconds 8] [--only ink-2026,slow-2026] [--hz 60]
 *       [--out /tmp/term-agent-bench] [--save docs/perf/term-agent.json] [--no-check] [--keep]
 *   bun scripts/term-agent-bench.ts --judge <report.json> [--no-check]        (re-judge a saved report, no app)
 *
 * Per pattern (`scripts/term-agent-gen.ts`: ink-2026, ink-no-2026, ratatui-diff, omp-boxes, slow-2026, scroll-stream):
 *  - `term_stats_reset`, then the recorder runs for --seconds (ScreenCaptureKit, needs Screen Recording permission) while
 *    the generator prints; the recording is cropped to the terminal's rect and decoded with the sweep's own decoder;
 *  - judged by `apps/desktop/src/lib/debug/flickerSweep.ts` (blips, blank frames, BT.1702 flashing: 0 allowed) and by the
 *    torn-frame oracle (every row of a frame starts with the frame's colour; two colours on one capture = a half-drawn
 *    frame; 0 allowed, for every pattern that wraps frames in 2026 and for the one that does not);
 *  - pacing from `term_stats` (`frames`, `interval_p95_us`): ≤ 1.05x the refresh rate in frames/s (coalescing) and the
 *    interval p95 ≤ 1.1x the refresh interval while flooding.
 * Scroll leg: a synthesized trackpad stream (`synth`, momentum included) is fed to the terminal as `term_wheel` calls at
 * its own pace, the same invoke the page's wheel handler makes, first over the streaming primary screen (rows moved vs
 * asked, the viewport must hold: #97), then over a mouse-reporting program that counts what it receives (reports = notches,
 * in order).
 *
 * Exits 1 on any budget line (`term-agent-judge.ts`), and when something was not measured: a crashed app is not a pass.
 * Heavy (a renderer, a recorder, bun producers): run it alone, through `scripts/heavy.sh`. The instance is stopped
 * explicitly at the end, also on failure (`--keep` leaves it). Method and numbers: docs/terminal.md.
 */
import { $ } from "bun";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { analyze, frameStarts, gridOf, GRID_H, GRID_W, type Frame } from "../apps/desktop/src/lib/debug/flickerSweep.ts";
import { synth } from "../apps/desktop/src/lib/shell/trace.ts";
import { flashWindows } from "../studio/src/lint/checks-video.ts";
import { check, colorIndex, parseRun, summarize, tornFrames, type Pattern, type Run, type Scroll } from "./term-agent-judge.ts";
import { PATTERNS } from "./term-agent-gen.ts";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const flag = (k: string) => process.argv.includes(`--${k}`);
const root = join(import.meta.dir, "..");

/** A terminal's cell area on the window, CSS px, with the window's own size and the grid it holds. */
interface TermRect {
	x: number;
	y: number;
	w: number;
	h: number;
	vw: number;
	vh: number;
	cols: number;
	rows: number;
	cellH: number;
}

/** `--judge`: re-judge a saved report with no app. */
function judging(path: string) {
	const parsed = parseRun(JSON.parse(readFileSync(path, "utf8")));
	if ("error" in parsed) {
		console.error(`${path}: ${parsed.error}`);
		process.exit(1);
	}
	verdict(parsed.run);
}

/** Exit 1 (message on stderr) when the budget is exceeded, unless `--no-check`. */
function verdict(run: Run) {
	const bad = check(run);
	if (bad.length && !flag("no-check")) {
		console.error(`terminal agent budget exceeded:\n  ${bad.join("\n  ")}`);
		process.exit(1);
	}
}

const judge = arg("judge");
if (judge) judging(judge);
else await main();

async function main() {
	const name = arg("name", "termbench")!;
	const seconds = Number(arg("seconds", "8"));
	const hz = Number(arg("hz", "60"));
	const only = arg("only")?.split(",");
	const out = arg("out", "/tmp/term-agent-bench")!;
	const save = arg("save");
	const atlas = join(root, "target/debug/atlas");
	const dir = `/private/tmp/atlas-agent-${name}`;
	const sock = `${dir}/debug.sock`;
	const env = { ...process.env, ATLAS_DEBUG_SOCK: sock };
	const debug = async (...a: string[]) => (await $`${atlas} desktop debug ${a}`.env(env).quiet()).text();
	const evalJs = async <T = unknown>(js: string): Promise<T> => JSON.parse(await debug("eval", js));
	const invoke = <T>(cmd: string, args = "{}") => evalJs<T>(`return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${args})`);
	const counts = `${dir}/termbench-counts`;

	const started = (await $`ls ${sock}`.quiet().nothrow()).exitCode === 0;
	if (started) throw new Error(`an agent instance named "${name}" is already running; stop it first (bun run desktop:agent -- --stop --name ${name})`);
	rmSync(out, { recursive: true, force: true });
	mkdirSync(out, { recursive: true });
	await $`bun run desktop:agent -- --name ${name} --scratch`.cwd(root).quiet();
	const run: Run = { hz, patterns: [], scroll: null, undriven: [] };
	try {
		mkdirSync(counts, { recursive: true });
		// One scratch project with one worktree to hold the terminals.
		const fx = "/private/tmp/term-agent-fixture";
		mkdirSync(fx, { recursive: true });
		writeFileSync(join(fx, "atlas.yaml"), 'name: term-agent\nservices:\n  web:\n    command: "sleep 100000"\n    port: 20998\n');
		await evalJs(`await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(fx)} }); await window.__atlas.store.refresh(); return 1`);
		const wt = await evalJs<string | null>(`for (let i = 0; i < 100; i++) {
			const w = window.__atlas.ws.doc.worktrees.find((x) => x.path === ${JSON.stringify(fx)} || x.path === ${JSON.stringify(fx.replace(/^\/private/, ""))});
			if (w) { window.__atlas.shell.select(w.id); return w.id; }
			await new Promise((r) => setTimeout(r, 100));
		}
		return window.__atlas.ws.doc.selected ?? null`);
		if (!wt) throw new Error("no worktree to open terminals in (is the instance on a project view?)");

		/** Open `argv` in a new terminal tab, show it, and wait for its native view. */
		const open = async (argv: string[], label: string, tabEnv: Record<string, string> = {}) => {
			const tab = await evalJs<string | null>(
				`const t = await window.__atlas.ws.newTerminal(${JSON.stringify(wt)}, { argv: ${JSON.stringify(argv)}, env: ${JSON.stringify(tabEnv)}, label: ${JSON.stringify(label)} }); if (!t) return null; window.__atlas.ws.activate(t.id); return t.id`,
			);
			if (!tab) throw new Error(`${label}: no terminal opened`);
			for (let i = 0; i < 100; i++) {
				if (tab in (await invoke<Record<string, unknown>>("term_stats"))) return tab;
				await Bun.sleep(100);
			}
			throw new Error(`${label}: the terminal never opened its native view`);
		};
		const close = (tab: string) => evalJs(`window.__atlas.ws.closeTab(${JSON.stringify(tab)}, true, true); return 1`);
		/** The terminal's rect on the window, CSS px (its cell area), and the window's CSS size. */
		const rectOf = (tab: string) =>
			evalJs<TermRect>(
				`const el = document.querySelector('textarea[data-term-tab=${JSON.stringify(tab).slice(1, -1)}]').parentElement; const r = el.getBoundingClientRect();
				const s = (await window.__TAURI_INTERNALS__.invoke("term_stats"))[${JSON.stringify(tab)}];
				return { x: r.x, y: r.y, w: r.width, h: r.height, vw: innerWidth, vh: innerHeight, cols: s.cols, rows: s.rows, cellH: s.cell_h }`,
		);
		const printed = (id: string): { bytes: number; produced: number } => {
			const f = join(counts, `${id}.json`);
			return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : { bytes: 0, produced: 0 };
		};

		/** Decode the recording cropped to the terminal: grid frames for the flicker judge, and the probe column per capture. */
		async function decode(file: string, r: TermRect) {
			const info: { streams: Array<{ width: number; height: number }> } = JSON.parse(
				await $`ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of json ${file}`.text(),
			);
			const { width: fw, height: fh } = info.streams[0]!;
			const sx = fw / r.vw;
			const sy = fh / r.vh;
			// Crop to the terminal; the cell area's first column, one row per terminal row, is the oracle's probe column.
			const cw = Math.max(2, Math.round(r.w * sx) & ~1);
			const ch = Math.max(2, Math.round(r.h * sy) & ~1);
			const crop = `crop=${cw}:${ch}:${Math.round(r.x * sx)}:${Math.round(r.y * sy)}`;
			const times = frameStarts(await $`ffprobe -v error -select_streams v:0 -show_entries frame=pts_time -of csv=p=0 ${file}`.text());
			const W = GRID_W * 8;
			const H = GRID_H * 8;
			const raw = new Uint8Array(await $`ffmpeg -v error -i ${file} -fps_mode passthrough -vf ${`${crop},scale=${W}:${H}:flags=area`} -f rawvideo -pix_fmt rgb24 -`.quiet().arrayBuffer());
			const frames: Frame[] = [];
			const size = W * H * 3;
			for (let i = 0; i < Math.floor(raw.length / size); i++) frames.push({ t: times[i] ?? times.at(-1) ?? 0, grid: gridOf(raw.subarray(i * size, (i + 1) * size), W, H) });
			// The probe column: the 3 px wide strip at the first cell, native resolution, one sample per terminal row.
			const cellW = (r.w / r.cols) * sx;
			const strip = new Uint8Array(await $`ffmpeg -v error -i ${file} -fps_mode passthrough -vf ${`${crop},crop=${Math.max(2, Math.round(cellW) & ~1)}:${ch}:0:0,scale=1:${r.rows}:flags=area`} -f rawvideo -pix_fmt rgb24 -`.quiet().arrayBuffer());
			const captures: number[][] = [];
			const per = r.rows * 3;
			for (let i = 0; i < Math.floor(strip.length / per); i++) {
				const col: number[] = [];
				for (let row = 0; row < r.rows; row++) {
					const o = i * per + row * 3;
					col.push(colorIndex([strip[o]!, strip[o + 1]!, strip[o + 2]!]));
				}
				captures.push(col);
			}
			return { frames, captures };
		}

		for (const p of PATTERNS.filter((x) => x.oracle || x.id !== "scroll-stream")) {
			if (only && !only.includes(p.id)) continue;
			const argv = ["bun", join(root, "scripts/term-agent-gen.ts"), "--pattern", p.id, "--seconds", String(seconds + 4), "--count-file", join(counts, `${p.id}.json`)];
			try {
				console.error(`▶ ${p.id}: ${p.covers}`);
				const tab = await open(argv, p.id);
				await Bun.sleep(1500); // the producer is up and the first frames drew
				const r = await rectOf(tab);
				await invoke("term_stats_reset");
				const before = printed(p.id);
				const file = join(out, `${p.id}.mov`);
				const rec = await $`${atlas} desktop debug record ${file} --seconds ${String(seconds)} --fps 60`.env(env).quiet().nothrow();
				if (rec.exitCode !== 0) throw new Error(`record failed: ${rec.stderr.toString().trim()}`);
				const stats = (await invoke<Record<string, { frames: number; interval_p95_us: number }>>("term_stats"))[tab];
				const after = printed(p.id);
				const { frames, captures } = await decode(file, r);
				const f = frames.length >= 3 ? analyze(frames, flashWindows) : { blips: [], blanks: [], flashes: [] };
				const pat: Pattern = {
					id: p.id,
					covers: p.covers,
					captures: frames.length,
					torn: p.oracle ? tornFrames(captures) : null,
					blips: f.blips.length,
					blanks: f.blanks.length,
					flashes: f.flashes.length,
					frames: stats?.frames ?? 0,
					seconds,
					intervalP95Us: stats?.interval_p95_us ?? 0,
					bytes: after.bytes - before.bytes,
					produced: after.produced - before.produced,
				};
				run.patterns.push(pat);
				console.error(`  ${frames.length} captures, torn ${pat.torn?.length ?? "-"}, ${pat.blips} blips, ${pat.blanks} blank, ${pat.flashes} flashing, ${(pat.frames / seconds).toFixed(1)} fps`);
				await close(tab);
			} catch (e) {
				run.undriven.push(`${p.id}: ${e instanceof Error ? e.message : String(e)}`);
				console.error(`  could not run ${p.id}: ${run.undriven.at(-1)}`);
				// a half-opened terminal must not stay on screen for the next pattern
				await evalJs(`for (const t of window.__atlas.ws.tabsOf(${JSON.stringify(wt)}).filter((t) => t.kind === "terminal")) window.__atlas.ws.closeTab(t.id, true, true); return 1`).catch(() => {});
			}
		}

		if (!only || only.includes("scroll-stream")) {
			try {
				run.scroll = await scrollLeg();
			} catch (e) {
				run.undriven.push(`scroll-stream: ${e instanceof Error ? e.message : String(e)}`);
			}
		}

		/** A synthesized trackpad stream (finger-down bell + AppKit-like momentum) as `term_wheel` calls at its own pace. */
		async function wheelStream(tab: string, cell: { cellH: number; cols: number }, perNotchCol: boolean): Promise<{ rows: number; notches: number }> {
			const events = synth({ x: 400, y: 300, travel: 900, ms: 400, momentum: 0.6, seed: 7, precise: true });
			const res = await evalJs<{ rows: number; notches: number }>(
				`const T = window.__TAURI_INTERNALS__; const ev = ${JSON.stringify(events)}; const cellH = ${cell.cellH}; const t0 = performance.now();
				let rows = 0, notches = 0, acc = 0;
				for (const e of ev) {
					const wait = e.t - (performance.now() - t0); if (wait > 0) await new Promise((r) => setTimeout(r, wait));
					if (!e.dy) continue;
					const r = e.dy / cellH; rows += r;
					acc += r; const whole = Math.trunc(acc); acc -= whole; notches += Math.abs(whole);
					await T.invoke("term_wheel", { tab: ${JSON.stringify(tab)}, e: { rows: r, precise: true, col: ${perNotchCol ? "Math.min(notches, " + (cell.cols - 1) + ")" : "0"}, row: 0, shift: false, alt: false, ctrl: false, readonly: false } });
				}
				return { rows: Math.round(rows), notches };`,
			);
			return res;
		}

		async function scrollLeg(): Promise<Scroll> {
			const argv = ["bun", join(root, "scripts/term-agent-gen.ts"), "--pattern", "scroll-stream", "--seconds", "40", "--count-file", join(counts, "scroll-stream.json")];
			const tab = await open(argv, "scroll-stream");
			// let some history build, then watch the viewport while a trackpad scrolls up through it
			await Bun.sleep(3000);
			const r = await rectOf(tab);
			await invoke("term_stats_reset");
			const b0 = printed("scroll-stream").bytes;
			const off0 = (await invoke<Record<string, { vt: { display_offset: number } }>>("term_stats"))[tab]!.vt.display_offset;
			const asked = await wheelStream(tab, r, false);
			const mid = (await invoke<Record<string, { vt: { display_offset: number } }>>("term_stats"))[tab]!.vt.display_offset;
			// output keeps streaming for a moment with the viewport scrolled up: it must stay where the user left it
			await Bun.sleep(1500);
			const held = (await invoke<Record<string, { vt: { display_offset: number } }>>("term_stats"))[tab]!.vt.display_offset;
			const st = (await invoke<Record<string, { interval_p95_us: number }>>("term_stats"))[tab]!;
			const streamed = printed("scroll-stream").bytes - b0;
			await close(tab);

			// A program that owns the mouse counts the reports.
			const mouse = await open(["bun", join(root, "scripts/term-agent-mouse.ts"), "--count-file", join(counts, "mouse.json"), "--seconds", "30"], "mouse-counter");
			await Bun.sleep(1000);
			const mr = await rectOf(mouse);
			const sent = await wheelStream(mouse, mr, true);
			await Bun.sleep(500);
			const m = JSON.parse(readFileSync(join(counts, "mouse.json"), "utf8")) as { seen: number; inOrder: boolean };
			await close(mouse);
			return {
				rowsAsked: asked.rows,
				// the viewport moves up into history, so the offset grows by the rows travelled (capped by the history there is)
				rowsMoved: mid - off0,
				notchesSent: sent.notches,
				reportsSeen: m.seen,
				reportsInOrder: m.inOrder,
				momentumIntervalsUs: st.interval_p95_us ? [st.interval_p95_us] : [],
				streamedBytes: streamed,
				viewportHeld: held >= mid,
			};
		}

		console.log(JSON.stringify({ when: new Date().toISOString(), hz, summary: summarize(run) }, null, 2));
		writeFileSync(join(out, "report.json"), JSON.stringify({ when: new Date().toISOString(), run }, null, 2));
		if (save) writeFileSync(resolve(root, save), `${JSON.stringify({ when: new Date().toISOString(), run }, null, 2)}\n`);
	} finally {
		// explicit, not a trap: a persistent shell never fires EXIT traps and the instance would leak
		if (!flag("keep")) await $`bun run desktop:agent -- --stop --name ${name}`.cwd(root).quiet().nothrow();
	}
	verdict(run);
}
