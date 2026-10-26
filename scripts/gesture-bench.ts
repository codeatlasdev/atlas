#!/usr/bin/env bun
/**
 * Gesture bench: replays every trace of the suite into the REAL running app (an agent instance, never the
 * user's window and never OS input) and holds it to the fluidity budget.
 *
 *   bun run desktop:agent -- --name gesture --scratch      # prints ATLAS_DEBUG_SOCK=…
 *   ATLAS_DEBUG_SOCK=… bun scripts/gesture-bench.ts [--only name,name] [--hz 60] [--json out.json] [--repeat N]
 *
 * Per trace the app is put on a known space (the middle one of ≥ 3), the trace is fed to the swipe engine
 * through the same entry point a real trackpad event uses (after the NSEvent→Sample conversion, which the bench
 * cannot cover: see docs/desktop.md), and the page records frames, stalls, forced layouts and layer motion.
 * `assess` (lib/debug/gestureProbe.ts, unit-tested) turns that into violations: wrong or double commits,
 * non-monotonic tracking, settle outside 250-350 ms, overshoot, frame gaps, main-thread stalls, layout thrash,
 * natives left parked, inconsistent end state. Prints a table; exits 1 on any violation.
 *
 * `--hz` is the refresh the budget is held to. The agent's virtual display is 60 Hz; the engine itself ticks at
 * the display's rate, so on a 120 Hz panel pass `--hz 120` (see the docs for what that can and cannot prove).
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { join } from "node:path";
import {
	assess,
	bestRun,
	isTimingOnly,
	noiseFloor,
	type Floor,
	type Run,
	type Violation,
} from "../apps/desktop/src/lib/debug/gestureProbe.ts";
import type { Trace } from "../apps/desktop/src/lib/shell/trace.ts";
import { aim, type LiveZone } from "../apps/desktop/src/lib/shell/traceAim.ts";

const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
const dir = join(root, "apps/desktop/src/lib/shell/__fixtures__/traces");

function arg(name: string): string | undefined {
	const i = process.argv.indexOf(name);
	return i >= 0 ? process.argv[i + 1] : undefined;
}

export function evalIn(js: string): unknown {
	const r = spawnSync(atlas, ["desktop", "debug", "eval", js], {
		encoding: "utf8",
		maxBuffer: 64 << 20,
	});
	if (r.status !== 0)
		throw new Error(`debug eval failed: ${r.stderr || r.stdout}`);
	return JSON.parse(r.stdout);
}

export function loadTraces(only?: string[]): Trace[] {
	return readdirSync(dir)
		.filter((f) => f.endsWith(".json"))
		.map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")) as Trace)
		.filter((t) => !only || only.includes(t.name))
		.sort((a, b) => a.name.localeCompare(b.name));
}

type Measured = Run & { hz: number; gestures: number; problems: string[] };

/** How many spaces the bench wants: both directions exist from the middle one, and a wrap is possible. */
const SPACES = 5;

/**
 * Make sure the instance has at least `SPACES` spaces: tiny git repos with an `atlas.yaml`, registered through the
 * same `projects_add` the UI uses. Idempotent (a repo already registered is skipped).
 */
export function seed(): void {
	const have =
		(evalIn(
			`return window.__atlas.store.sorted.filter((p) => !p.parent).length`,
		) as number) ?? 0;
	for (let i = have; i < SPACES; i++) {
		const dir = join(tmpdir(), "atlas-gesture-bench", `space${i + 1}`);
		mkdirSync(join(dir, "src"), { recursive: true });
		writeFileSync(join(dir, "src/a.ts"), `export const space = ${i + 1};\n`);
		writeFileSync(
			join(dir, "atlas.yaml"),
			`name: bench${i + 1}\nservices:\n  web:\n    command: "sleep 3600"\n`,
		);
		const git = (...a: string[]) =>
			spawnSync("git", [
				"-C",
				dir,
				"-c",
				"user.email=b@b",
				"-c",
				"user.name=bench",
				...a,
			]);
		git("init", "-q");
		git("add", "-A");
		git("commit", "-qm", "bench");
		evalIn(
			`return await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(dir)} }).then((p) => p.id, (e) => String(e))`,
		);
	}
	// the sidebar needs a moment to list them and mount the new layers
	evalIn(
		`await new Promise((r) => setTimeout(r, 1500)); return window.__atlas.store.sorted.length`,
	);
}

/** The zones the engine hit-tests right now, read back from the app (after the layout settled). */
function zonesNow(): LiveZone[] {
	return (
		(evalIn(
			`return await window.__TAURI_INTERNALS__.invoke("swipe_zones_now")`,
		) as LiveZone[]) ?? []
	);
}

/** Put the app on the middle, first or last space (≥ 3 spaces: both directions exist from the middle). */
function reset(at: "mid" | "first" | "last"): {
	order: string[];
	start: string;
} {
	const pick = {
		mid: "Math.floor(ids.length / 2)",
		first: "0",
		last: "ids.length - 1",
	}[at];
	return evalIn(`
		const a = window.__atlas, ids = a.store.sorted.filter((p) => !p.parent).map((p) => p.id);
		a.docks.sidebar = true; // space swipes happen over it; a copied desktop setting may have it hidden
		if (ids.length < 3) throw new Error("the bench needs at least 3 spaces, has " + ids.length);
		const id = ids[${pick}];
		a.store.selected = id; a.ws.select(id);
		await new Promise((r) => setTimeout(r, 900));
		return { order: ids, start: id };`) as { order: string[]; start: string };
}

function runOne(t: Trace, hz: number): Measured {
	reset(t.expect.edge ?? "mid");
	// The tab zone pages through the focused group's tabs (22 for the many-tabs trace, else 5); every other zone
	// is held against the space pager (a swipe over a native view or outside any zone must leave it alone).
	const zone = t.expect.zone === "tab" ? "tab" : "space";
	const tabs = t.name.includes("many-tabs") ? 22 : 5;
	// Aim every sample at the zone the trace is meant for, in THIS layout (read back from the engine's table).
	const aimed = t.expect.aim
		? aim(
				t.events,
				t.expect.aim,
				zonesNow(
					{ sidebar: "space", strip: "tab", history: "history", outside: "" }[
						t.expect.aim
					],
				),
			)
		: t.events;
	if (!aimed)
		throw new Error(
			`${t.name}: the layout has no zone to aim at (${t.expect.aim})`,
		);
	const run = { ...t, events: aimed };
	return evalIn(
		`return await window.__atlasDebug.gesture({ hz: ${hz}, zone: ${JSON.stringify(zone)}, tabs: ${tabs}, trace: ${JSON.stringify(run)} })`,
	) as Measured;
}

const pad = (s: string, n: number) =>
	s.length >= n ? s : s + " ".repeat(n - s.length);
const num = (x: number, d = 0) => (Number.isFinite(x) ? x.toFixed(d) : "-");

/** Every rule the run is held to: the budget (`assess`) plus what the engine and the page reported. */
function verdict(
	t: Trace,
	m: Measured,
	hz: number,
	floor?: Floor,
): Violation[] {
	const v = assess(m, { hz, floor });
	for (const p of m.problems) v.push({ rule: "end-state", detail: p });
	const want = t.expect.gestures ?? (t.expect.steps === 0 ? undefined : 1);
	if (want !== undefined && m.gestures !== want)
		v.push({
			rule: "gestures",
			detail: `engine claimed ${m.gestures} gesture(s), expected ${want}`,
		});
	if (t.expect.untouched && m.gestures !== 0)
		v.push({
			rule: "claimed-untouched",
			detail: `engine claimed ${m.gestures} gesture(s) for a trace that must not swipe`,
		});
	return v;
}

function main() {
	if (!process.env.ATLAS_DEBUG_SOCK) {
		console.error(
			"ATLAS_DEBUG_SOCK is not set: start an agent instance (bun run desktop:agent -- --name gesture --scratch)",
		);
		process.exit(2);
	}
	const hz = Number(arg("--hz") ?? 60);
	const repeat = Number(arg("--repeat") ?? 1);
	const only = arg("--only")?.split(",");
	seed();
	const traces = loadTraces(only);
	if (!traces.length) {
		console.error("no traces matched");
		process.exit(2);
	}
	const retries = Number(arg("--retries") ?? 4);
	// The host's own jitter with the page idle: timing rules are judged against it (see `assess`).
	const floor = noiseFloor(
		evalIn(`return await window.__atlasDebug.idleFloor(3000)`) as Parameters<
			typeof noiseFloor
		>[0],
	);
	const rows: {
		name: string;
		v: Violation[];
		m: Measured;
		attempts: number;
	}[] = [];
	for (const t of traces) {
		for (let i = 0; i < repeat; i++) {
			// A loaded host (other builds running) shows up as stalls and long frames: a trace that failed ONLY on
			// timing gets up to `retries` more attempts and reports its best. A wrong page is never retried.
			const attempts: { m: Measured; v: Violation[] }[] = [];
			do {
				const m = runOne(t, hz);
				attempts.push({ m, v: verdict(t, m, hz, floor) });
			} while (isTimingOnly(attempts.at(-1)!.v) && attempts.length <= retries);
			const best = bestRun(attempts);
			rows.push({
				name: t.name,
				v: best.v,
				m: best.m,
				attempts: attempts.length,
			});
		}
	}
	const w = Math.max(...rows.map((r) => r.name.length)) + 1;
	console.log(
		`idle floor: frame gap p95 ${floor.gapP95.toFixed(1)} ms, worst stall ${floor.worstStall} ms over ${floor.frames} frames`,
	);
	console.log(
		`host load ${loadavg()
			.map((x) => x.toFixed(1))
			.join(
				" ",
			)} (a loaded host adds stalls; timing-only failures are retried up to ${retries}x)`,
	);
	console.log(
		`${pad("trace", w)} ${pad("steps", 6)} ${pad("settle", 7)} ${pad("1st-mv", 7)} ${pad("p95 gap", 8)} ${pad("max gap", 8)} ${pad("stall", 6)} ${pad("reflow", 7)} result`,
	);
	let bad = 0;
	for (const { name, v, m, attempts } of rows) {
		const gaps = m.frames.map((f) => f.gap).sort((a, b) => a - b);
		const p95 =
			gaps[Math.min(gaps.length - 1, Math.floor(gaps.length * 0.95))] ?? 0;
		const net = m.order.indexOf(m.end) - m.order.indexOf(m.start);
		console.log(
			`${pad(name, w)} ${pad((net >= 0 ? "+" : "") + net, 6)} ${pad(num(m.settleMs), 7)} ${pad(num(m.firstMoveMs, 0), 7)} ${pad(num(p95, 1), 8)} ${pad(num(gaps.at(-1) ?? 0, 1), 8)} ${pad(String(m.stalls.length), 6)} ${pad(String(m.reflows), 7)} ${v.length ? "FAIL" : "ok"}${attempts > 1 ? ` (${attempts} tries)` : ""}`,
		);
		for (const x of v) console.log(`    ✗ ${x.rule}: ${x.detail}`);
		bad += v.length ? 1 : 0;
	}
	const out = arg("--json");
	if (out) writeFileSync(out, JSON.stringify(rows, null, 1));
	console.log(
		`\n${rows.length - bad}/${rows.length} runs within budget (display ${hz} Hz)`,
	);
	process.exit(bad ? 1 : 0);
}

if (import.meta.main) main();
