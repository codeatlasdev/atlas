/**
 * Pure logic behind the Activity drawer: parse the runner's timeline events, fold bursts
 * ("server → stopped / starting / running" within seconds) into one row, group rows by how
 * long ago they happened and filter them. No Svelte, no DOM.
 */
import { dateTime, relative, upperFirst } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { stateLabel } from "./stateLabel.ts";

/** Same shape as the store's `TimelineEvent` (kept structural so this file has no store import). */
export interface RawEvent {
	ts: number;
	kind: "alert" | "note" | "state" | "tunnel" | "action";
	text: string;
}

type Tone = "ok" | "warn" | "fail" | "info" | "off";
export type Filter = "all" | "errors" | "restarts" | "actions";

export interface Entry {
	ts: number;
	/** Service the event is about; null for tunnel events. */
	service: string | null;
	kind: RawEvent["kind"];
	/** Target state for `state` events. */
	state: string | null;
	/** Message for alerts/notes/tunnel (without the `service:` prefix). */
	message: string;
	/** Pushed by the app when an action starts/ends (`<label>: started|finished|failed`); `service` is its label. */
	action: boolean;
}

export interface Row {
	/** Stable id for keyed lists: first entry's timestamp + service + position. */
	id: string;
	/** Newest entry's time (the row sorts and groups by it). */
	ts: number;
	/** Oldest entry's time; differs from `ts` for a folded burst. */
	from: number;
	service: string | null;
	tone: Tone;
	title: string;
	detail: string | null;
	/** How many raw entries were folded into this row (1 = not folded). */
	count: number;
	/** What the row is, for the filters. */
	type: "restart" | "error" | "action" | "state" | "note";
	entries: Entry[];
}

export interface Group {
	label: string;
	rows: Row[];
}

const STATE_TONE: Record<string, Tone> = {
	running: "ok",
	starting: "warn",
	failed: "fail",
	unhealthy: "fail",
	degraded: "warn",
	stopped: "off",
	disabled: "off",
};

function stateTone(state: string): Tone {
	return STATE_TONE[state] ?? "info";
}

/** `"api → running"` → service + state; `"api: boom"` → service + message. */
export function parse(ev: RawEvent): Entry {
	if (ev.kind === "state") {
		const hit = /^(.+?) → (\S+)$/.exec(ev.text);
		if (hit) return { ts: ev.ts, service: hit[1], kind: "state", state: hit[2], message: "", action: false };
	}
	if (ev.kind === "tunnel") {
		return { ts: ev.ts, service: null, kind: "tunnel", state: null, message: ev.text, action: false };
	}
	const hit = /^([^:]+): (.*)$/s.exec(ev.text);
	return {
		ts: ev.ts,
		service: hit ? hit[1] : null,
		kind: ev.kind,
		state: null,
		message: hit ? hit[2] : ev.text,
		action: ev.kind === "action",
	};
}

/** Two state changes of one service closer than this belong to the same burst. */
const BURST_MS = 60_000;

/**
 * A burst of one service's state changes becomes one row. It is a "restart" when the service
 * came back after being down (stopped/failed/starting … running) and it settled in a state;
 * otherwise it is a plain transition.
 */
function stateRow(service: string, chain: Entry[]): Row {
	const last = chain[chain.length - 1];
	const first = chain[0];
	const states = chain.map((e) => e.state ?? "");
	const downs = states.filter((s) => s === "stopped" || s === "failed").length;
	const ups = states.filter((s) => s === "running").length;
	const base = {
		id: `${first.ts}:${service}:${chain.length}`,
		ts: last.ts,
		from: first.ts,
		service,
		entries: chain,
	};
	if (chain.length > 1 && downs > 0 && ups > 0) {
		const n = Math.min(downs, ups);
		const settled = last.state ?? "";
		return {
			...base,
			tone: stateTone(settled),
			title:
				n > 1 ? m.panel_activity_restarted_n({ service, count: n }) : m.panel_activity_restarted({ service }),
			detail: settled === "running" ? null : m.panel_activity_now_state({ state: stateLabel(settled) }),
			count: chain.length,
			type: "restart",
		};
	}
	const failed = states.includes("failed");
	if (chain.length > 1) {
		// e.g. starting → running: show the destination, keep the path as detail.
		return {
			...base,
			tone: stateTone(last.state ?? ""),
			title: `${service} → ${stateLabel(last.state ?? "")}`,
			detail: states.map(stateLabel).join(" → "),
			count: chain.length,
			type: failed ? "error" : "state",
		};
	}
	return {
		...base,
		tone: stateTone(last.state ?? ""),
		title: `${service} → ${stateLabel(last.state ?? "")}`,
		detail: null,
		count: 1,
		type: last.state === "failed" ? "error" : "state",
	};
}

/**
 * The title of a row that is not a state change. An action's `started` / `finished` / `failed` are the runner's own
 * words (they decide the tone above), shown translated; any other text (an alert's message) is the runner's, as sent.
 */
function rowTitle(e: Entry): string {
	if (e.action && e.service) {
		const label = e.service;
		if (e.message === "started") return m.panel_activity_action_started({ label });
		if (e.message === "finished") return m.panel_activity_action_finished({ label });
		if (e.message === "failed") return m.panel_activity_action_failed({ label });
	}
	return e.service ? `${e.service}: ${e.message}` : e.message;
}

/** Fold `events` (oldest first) into display rows, newest first. */
export function fold(events: RawEvent[], burstMs = BURST_MS): Row[] {
	const rows: Row[] = [];
	// Open chain per service: extended while the next state change lands within the burst window.
	const open = new Map<string, Entry[]>();
	const flush = (service: string) => {
		const chain = open.get(service);
		if (chain) rows.push(stateRow(service, chain));
		open.delete(service);
	};
	for (const ev of events) {
		const e = parse(ev);
		if (e.kind === "state" && e.service) {
			const chain = open.get(e.service);
			if (chain && e.ts - chain[chain.length - 1].ts <= burstMs) chain.push(e);
			else {
				flush(e.service);
				open.set(e.service, [e]);
			}
			continue;
		}
		const isErr = e.kind === "alert";
		const failedAction = e.action && e.message === "failed";
		rows.push({
			id: `${e.ts}:${e.service ?? ""}:${e.kind}:${rows.length}`,
			ts: e.ts,
			from: e.ts,
			service: e.service,
			tone:
				isErr || failedAction
					? "fail"
					: e.action
						? e.message === "finished"
							? "ok"
							: "info"
						: e.kind === "tunnel"
							? "info"
							: "warn",
			title: rowTitle(e),
			detail: null,
			count: 1,
			type: e.action ? "action" : isErr ? "error" : "note",
			entries: [e],
		});
	}
	for (const s of [...open.keys()]) flush(s);
	// Identical consecutive alerts fold too ("boom ×4").
	rows.sort((a, b) => b.ts - a.ts || b.from - a.from);
	return mergeRepeats(rows);
}

/** Adjacent rows with the same title and type (crash-loop alerts) collapse into one with a count. */
function mergeRepeats(rows: Row[]): Row[] {
	const out: Row[] = [];
	for (const r of rows) {
		const p = out[out.length - 1];
		if (p && p.type !== "restart" && p.type === r.type && p.title === r.title && p.service === r.service) {
			p.count += r.count;
			p.from = Math.min(p.from, r.from);
			p.entries = [...p.entries, ...r.entries];
			p.detail = m.panel_activity_n_times({ count: p.count });
		} else out.push({ ...r });
	}
	return out;
}

function matches(row: Row, f: Filter): boolean {
	switch (f) {
		case "all":
			return true;
		case "errors":
			return row.type === "error" || row.tone === "fail";
		case "restarts":
			return row.type === "restart";
		case "actions":
			return row.type === "action";
	}
}

export function filterRows(rows: Row[], f: Filter): Row[] {
	return f === "all" ? rows : rows.filter((r) => matches(r, f));
}

export function counts(rows: Row[]): Record<Filter, number> {
	return {
		all: rows.length,
		errors: rows.filter((r) => matches(r, "errors")).length,
		restarts: rows.filter((r) => matches(r, "restarts")).length,
		actions: rows.filter((r) => matches(r, "actions")).length,
	};
}

const pad = (n: number) => String(n).padStart(2, "0");

export function clock(ts: number): string {
	const d = new Date(ts);
	return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function sameDay(a: number, b: number): boolean {
	const x = new Date(a);
	const y = new Date(b);
	return x.getFullYear() === y.getFullYear() && x.getMonth() === y.getMonth() && x.getDate() === y.getDate();
}

/** Group label for a row at `ts` seen from `now`: Now / 5 min ago / Today 19:00 / Yesterday 21:00 / Mon, Oct 10. */
export function bucket(ts: number, now: number): string {
	const age = Math.max(0, now - ts);
	if (age < 60_000) return upperFirst(relative(0, "second"));
	if (age < 3_600_000) {
		const mins = Math.floor(age / 60_000);
		// Coarse steps so the headers don't multiply.
		return upperFirst(relative(-(mins < 5 ? 1 : mins < 15 ? 5 : mins < 30 ? 15 : 30), "minute"));
	}
	const hour = new Date(ts).setMinutes(0, 0, 0);
	const hhmm = dateTime(hour, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
	if (sameDay(ts, now)) return `${upperFirst(relative(0, "day"))} ${hhmm}`;
	const yesterday = now - 86_400_000;
	if (sameDay(ts, yesterday)) return `${upperFirst(relative(-1, "day"))} ${hhmm}`;
	return dateTime(ts, { weekday: "short", day: "numeric", month: "short" });
}

/** Group newest-first rows by `bucket`. Rows keep their order; groups follow the first row. */
export function group(rows: Row[], now: number): Group[] {
	const out: Group[] = [];
	for (const r of rows) {
		const label = bucket(r.ts, now);
		const g = out[out.length - 1];
		if (g && g.label === label) g.rows.push(r);
		else out.push({ label, rows: [r] });
	}
	return out;
}

/**
 * Rows newer than `seenAt` that need attention: failures and warnings (a crash, a restart, a degraded service, a failed
 * action). Normal starts and stops are history, not news: counting them put a red "9+" on the Activity button of a
 * brand-new user whose stack had just come up (1.5.0-alpha.26). `errors` is the part that is a failure, for the tone.
 */
export function unread(rows: Row[], seenAt: number): { n: number; errors: number } {
	let n = 0;
	let errors = 0;
	for (const r of rows) {
		if (r.ts <= seenAt) break;
		if (r.tone !== "fail" && r.tone !== "warn") continue;
		n++;
		if (r.tone === "fail") errors++;
	}
	return { n, errors };
}

/** `1h 4m` / `3m` / `12s` — a row's span when it folded several entries. */
export function span(from: number, to: number): string {
	const s = Math.max(0, Math.round((to - from) / 1000));
	if (s < 60) return `${s}s`;
	if (s < 3600) return `${Math.floor(s / 60)}m`;
	return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}
