import { m } from "#lib/i18n/m.ts";

/** The machine-wide heavy-job queue (`atlas run --heavy`) as the `heavy_status` command returns it (snake_case, as serialized by Rust). */
export interface HeavyJob {
	id: string;
	pid: number;
	label: string;
	command: string;
	cwd: string;
	project: string;
	state: "queued" | "running";
	/** Unix ms. */
	queued_at: number;
	started_at: number | null;
	slot: number | null;
	/** Seconds running (running) or waiting (queued). */
	age_s: number;
	/** Average seconds of the label's recent runs. */
	avg_s: number | null;
	/** Seconds until a running job ends, or a queued one starts. */
	eta_s: number | null;
}

export interface HeavyStatus {
	slots: number;
	load: number | null;
	/** Percent of memory free, null when the OS cannot tell. */
	mem_available_pct: number | null;
	running: HeavyJob[];
	queued: HeavyJob[];
}

/** The queue is worth a place in the drawer only while something runs or waits. */
export function active(s: HeavyStatus | null): s is HeavyStatus {
	return !!s && s.running.length + s.queued.length > 0;
}

/** `1m05s`, `45s`, `2h03m`. */
export function human(secs: number): string {
	if (secs < 60) return `${secs}s`;
	if (secs < 3600) return `${Math.floor(secs / 60)}m${String(secs % 60).padStart(2, "0")}s`;
	return `${Math.floor(secs / 3600)}h${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}m`;
}

/** One line: who holds the slots and how many wait ("2 of 2 slots busy · 3 waiting"). */
export function summary(s: HeavyStatus): string {
	const args = { running: s.running.length, slots: s.slots, queued: s.queued.length };
	return s.queued.length ? m.panel_activity_heavy_busy_waiting(args) : m.panel_activity_heavy_busy(args);
}

/** Row title and detail for a job: what it is, where, and how long (with the ETA when history knows it). */
export function describe(j: HeavyJob): { title: string; detail: string } {
	const args = { project: j.project, age: human(j.age_s), eta: j.eta_s == null ? "" : human(j.eta_s) };
	const detail =
		j.state === "running"
			? j.eta_s == null
				? m.panel_activity_heavy_running({ ...args })
				: m.panel_activity_heavy_running_eta({ ...args })
			: j.eta_s == null
				? m.panel_activity_heavy_waiting({ ...args })
				: m.panel_activity_heavy_waiting_eta({ ...args });
	return { title: j.label, detail };
}
