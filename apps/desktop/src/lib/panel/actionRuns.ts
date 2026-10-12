/**
 * Output of non-session actions, buffered per project: one `Run` per execution of an
 * action (label + start time), oldest first. The runner streams `action_progress` one line
 * at a time and `action_done` at the end; a client that attaches mid-run gets a bounded
 * tail of each recent run in the snapshot (`action_runs`), merged here.
 */
import { Ring } from "#lib/logs.ts";

/** Lines kept per run. */
export const MAX_RUN_LINES = 5000;
/** Runs kept per project. */
export const MAX_RUNS = 10;
/** A snapshot run and a local run of the same label this close in time are the same run. */
const SAME_RUN_MS = 2000;

export type RunStatus = "running" | "ok" | "failed";

export interface RunLine {
	/** Raw line, escapes included (rendered through `lib/ansi.ts`). */
	text: string;
	ts: number;
}

export interface Run {
	/** Unique per project, ascending with start. */
	id: number;
	label: string;
	status: RunStatus;
	start: number;
	/** Set once the action finished. */
	end: number | null;
	lines: Ring<RunLine>;
	/** Lines the runner dropped before this client attached (snapshot tail only). */
	truncated: number;
}

/** One entry of `snapshot.action_runs` (runner.rs; absent on older runners). */
export interface SnapshotRun {
	label: string;
	/** Epoch ms the runner saw the action start. */
	start?: number;
	status?: RunStatus;
	end?: number | null;
	lines?: string[];
	/** Output lines before `lines`: the runner keeps a bounded tail. */
	skipped?: number;
}

/** What the store keeps per project. */
export interface Runs {
	list: Run[];
	next: number;
	/** Bumped on every change so views track one number (the lines live in non-reactive rings). */
	rev: number;
}

export function emptyRuns(): Runs {
	return { list: [], next: 1, rev: 0 };
}

function makeRun(id: number, label: string, start: number, status: RunStatus, end: number | null): Run {
	return { id, label, status, start, end, lines: new Ring<RunLine>(MAX_RUN_LINES), truncated: 0 };
}

function open(runs: Runs, label: string, start: number): Run {
	const run = makeRun(runs.next++, label, start, "running", null);
	runs.list.push(run);
	if (runs.list.length > MAX_RUNS) runs.list.splice(0, runs.list.length - MAX_RUNS);
	return run;
}

/** The unfinished run of `label` (a label has at most one in flight). */
export function activeRun(runs: Runs, label: string): Run | undefined {
	return runs.list.findLast((r) => r.label === label && r.status === "running");
}

/** One output line; the first line of an action starts its run. `started` tells the caller. */
export function pushLine(
	runs: Runs,
	label: string,
	text: string,
	ts: number,
): { run: Run; started: boolean } {
	let run = activeRun(runs, label);
	const started = !run;
	run ??= open(runs, label, ts);
	run.lines.push([{ text, ts }]);
	runs.rev++;
	return { run, started };
}

/**
 * The action finished. One that never printed a line is created and closed at once so it
 * still shows up. `null` for the runner's empty-label resume pings (not an action).
 */
export function finish(runs: Runs, label: string, ok: boolean, ts: number): Run | null {
	if (!label) return null;
	const run = activeRun(runs, label) ?? open(runs, label, ts);
	run.status = ok ? "ok" : "failed";
	run.end = ts;
	runs.rev++;
	return run;
}

/** The runner went away: whatever was running will not report back. */
export function abandon(runs: Runs, ts: number) {
	const running = runs.list.filter((r) => r.status === "running");
	for (const r of running) {
		r.status = "failed";
		r.end = ts;
	}
	if (running.length) runs.rev++;
}

/**
 * Merge a snapshot's runs after a (re)sync (the desktop re-requests it whenever it
 * attaches). A run both sides know keeps the longer buffer (live events may already have
 * filled it) and adopts a final status; runs started before this client attached are added
 * ahead of the local ones. A run still "running" locally that the runner no longer lists
 * ended while the client was away and is closed as failed (the outcome is unknowable).
 * `snap === undefined` (older runner) leaves everything untouched.
 */
export function mergeSnapshot(runs: Runs, snap: SnapshotRun[] | undefined, now: number) {
	if (!snap) return;
	const known = new Set<Run>();
	const added: Run[] = [];
	for (const s of snap) {
		const start = s.start ?? now;
		const status = s.status ?? "running";
		const lines = (s.lines ?? []).map((text) => ({ text, ts: start }));
		let run = runs.list.find((r) => r.label === s.label && Math.abs(r.start - start) < SAME_RUN_MS);
		if (run) {
			if (run.lines.items.length < lines.length) {
				run.lines = new Ring<RunLine>(MAX_RUN_LINES);
				run.lines.push(lines);
			}
			if (run.status === "running" && status !== "running") {
				run.status = status;
				run.end = s.end ?? now;
			}
		} else {
			run = makeRun(0, s.label, start, status, status === "running" ? null : (s.end ?? now));
			run.lines.push(lines);
			added.push(run);
		}
		run.truncated = Math.max(run.truncated, s.skipped ?? 0);
		known.add(run);
	}
	for (const r of runs.list) {
		if (r.status === "running" && !known.has(r)) {
			r.status = "failed";
			r.end = now;
		}
	}
	const merged = [...added, ...runs.list].sort((a, b) => a.start - b.start || a.id - b.id);
	for (const r of merged) r.id ||= runs.next++;
	runs.list = merged.slice(-MAX_RUNS);
	runs.rev++;
}

/** `12s`, `3m 04s`, `1h 02m`: how long a run took (or has been running). */
export function duration(run: Pick<Run, "start" | "end">, now: number): string {
	const s = Math.max(0, Math.round(((run.end ?? now) - run.start) / 1000));
	if (s < 60) return `${s}s`;
	const m = Math.floor(s / 60);
	if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
	return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}
