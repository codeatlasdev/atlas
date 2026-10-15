import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { api, errText, type ProjectConfig, type ProjectView } from "#lib/api.ts";
import { sound } from "#lib/sound/sound.svelte.ts";
import { stackSound } from "#lib/sound/triggers.ts";
import { Ring, toLogLine, type LogLine, type RawLog } from "#lib/logs/model.ts";
import {
	abandon,
	emptyRuns,
	finish,
	mergeSnapshot,
	pushLine,
	type Runs,
	type SnapshotRun,
} from "#lib/panel/actionRuns.ts";

/** One row of the runner's `service_json` (runner.rs). */
export interface Service {
	name: string;
	state: string;
	exit_code: number | null;
	restarts: number;
	note: string | null;
	paused_by?: string | null;
	deselected?: boolean;
	completed_ok?: boolean;
	/** Process-tree memory (MB) and CPU (% of a core), sampled by the runner every 5s. */
	mem_mb?: number | null;
	cpu?: number | null;
}

/** Field-wise compare: runner `state` events repeat on every health tick. */
function rowChanged(a: Service, b: Service): boolean {
	return (
		a.state !== b.state ||
		a.exit_code !== b.exit_code ||
		a.restarts !== b.restarts ||
		a.note !== b.note ||
		a.paused_by !== b.paused_by ||
		a.deselected !== b.deselected ||
		a.completed_ok !== b.completed_ok ||
		a.mem_mb !== b.mem_mb ||
		a.cpu !== b.cpu
	);
}

export interface TimelineEvent {
	ts: number;
	kind: "alert" | "note" | "state" | "tunnel" | "action";
	text: string;
}

export interface Tunnel {
	state: string;
	local_port: number;
	remote: string | null;
	ssh_host: string;
}

export interface Live {
	services: Service[];
	tunnel: Tunnel | null;
	/** Bumped at most once per frame when the log buffer (`store.logs(id)`) changes. */
	logRev: number;
	/** The first snapshot (with its history) arrived: distinguishes "loading" from "no output". */
	logsLoaded: boolean;
	events: TimelineEvent[];
	operation: { label: string; detail: string } | null;
	/** Recent action runs with their output (rings are not reactive: track `runs.rev`). */
	runs: Runs;
	mode: string | null;
	version: string | null;
	configChanged: number;
}

/** Runner JSON lines (runner.rs) plus the desktop's own `snapshot`/`poll`/`config_changed`. */
type RunnerEvent =
	| {
			t: "snapshot";
			services?: Service[];
			tunnel?: Tunnel | null;
			mode?: string;
			version?: string;
			operation?: Live["operation"];
			action_runs?: SnapshotRun[];
			logs?: RawLog[];
			events?: RunnerEvent[];
	  }
	| { t: "poll"; services?: Service[]; tunnel?: Tunnel | null; mode?: string }
	| { t: "rows"; services?: Service[]; tunnel?: Tunnel | null }
	| ({ t: "state" } & Service)
	| ({ t: "log" } & Omit<RawLog, "ts"> & { ts?: number })
	| { t: "logs"; lines: RawLog[] }
	| { t: "gap"; skipped: number }
	| { t: "note"; name: string; message: string }
	| { t: "alert"; service: string; message: string }
	| { t: "tunnel"; state: string; tunnel: Tunnel | null }
	| { t: "action_progress"; label: string; line: string }
	| { t: "action_done"; label: string; ok: boolean }
	| { t: "started" | "bye" | "config_changed" };

/** Lines kept per project; the ring trims in slabs of 1/8 of this. */
const MAX_LOGS = 50_000;
/** Same toast/alert at most once per this window. */
const ALERT_EVERY_MS = 60_000;
/** Window focus re-checks projects on disk at most this often. */
const FOCUS_EVERY_MS = 5_000;
const MAX_EVENTS = 200;
let seq = 0;

function emptyLive(): Live {
	return {
		services: [],
		tunnel: null,
		logRev: 0,
		logsLoaded: false,
		events: [],
		operation: null,
		runs: emptyRuns(),
		mode: null,
		version: null,
		configChanged: 0,
	};
}

/** Runner events carry no ts; history replays and live ones use arrival time. */
function timeline(ev: RunnerEvent): TimelineEvent | null {
	const ts = Date.now();
	switch (ev.t) {
		case "alert":
			return { ts, kind: "alert", text: `${ev.service}: ${ev.message}` };
		case "note":
			return { ts, kind: "note", text: `${ev.name}: ${ev.message}` };
		case "state":
			return { ts, kind: "state", text: `${ev.name} → ${ev.state}` };
		case "tunnel":
			return { ts, kind: "tunnel", text: `tunnel ${ev.state}` };
	}
	return null;
}

class Store {
	projects = $state<ProjectView[]>([]);
	live = $state<Record<string, Live>>({});
	configs = $state<Record<string, ProjectConfig>>({});
	selected = $state<string | null>(null);
	busy = $state<Record<string, boolean>>({});
	focusedService = $state<string | null>(null);
	loaded = $state(false);

	#unlisten = new Map<string, Promise<UnlistenFn>>();
	/**
	 * Log buffers, deliberately NOT reactive: 20k deep-proxied rows per project cost
	 * hundreds of MB and an O(n) signal storm on every trim. Readers track `logRev`.
	 */
	#logs = new Map<string, Ring<LogLine>>();
	#dirty = new Set<string>();
	#frame = 0;
	#lastAlert = new Map<string, number>();

	/** Read inside a reactive scope after reading `live[id].logRev`. */
	logs(id: string): Ring<LogLine> {
		let r = this.#logs.get(id);
		if (!r) this.#logs.set(id, (r = new Ring<LogLine>(MAX_LOGS)));
		return r;
	}

	#touchLogs(id: string) {
		this.#dirty.add(id);
		if (this.#frame) return;
		this.#frame = requestAnimationFrame(() => {
			this.#frame = 0;
			for (const d of this.#dirty) {
				const s = this.live[d];
				if (s) s.logRev++;
			}
			this.#dirty.clear();
		});
	}

	/** Toasts for the same message at most once per minute (crash loops repeat them). */
	#alert(key: string, text: string) {
		const now = Date.now();
		if (now - (this.#lastAlert.get(key) ?? 0) < ALERT_EVERY_MS) return;
		this.#lastAlert.set(key, now);
		toast.error(text);
	}

	sorted = $derived(
		[...this.projects].sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.name.localeCompare(b.name)),
	);
	current = $derived(this.projects.find((p) => p.id === this.selected) ?? null);
	/** Local projects and worktrees whose folder (or atlas.yaml) is gone. */
	missing = $derived(this.projects.filter((p) => p.missing));

	#lastFocus = 0;

	/** Window focus: re-check the disk, at most every few seconds (focus can flap). */
	onFocus() {
		const now = Date.now();
		if (now - this.#lastFocus < FOCUS_EVERY_MS) return;
		this.#lastFocus = now;
		void this.refresh();
	}

	#started = false;
	#unlistenChanged: UnlistenFn | null = null;
	#unlistenError: UnlistenFn | null = null;

	async init() {
		if (this.#started) return;
		this.#started = true;
		// Listeners and the list are independent IPCs: one round of waiting, not three. The UI only
		// needs the list; attaching to each runner (`resync` replays its logs) continues behind it.
		const listeners = Promise.all([
			listen("projects://changed", () => this.refresh()),
			listen<string>("projects://error", (e) => toast.error(e.payload)),
		]).then(([changed, error]) => {
			this.#unlistenChanged = changed;
			this.#unlistenError = error;
		});
		const listed = () => {
			this.loaded = true;
			if (!this.selected && this.sorted.length) this.selected = this.sorted[0].id;
		};
		await this.refresh(listed);
		// A failed list was already toasted; the UI still leaves its loading state.
		if (!this.loaded) listed();
		await listeners;
	}

	/** Drop every listener so a re-mount (HMR) starts clean. */
	dispose() {
		this.#unlistenChanged?.();
		this.#unlistenError?.();
		this.#unlistenChanged = null;
		this.#unlistenError = null;
		for (const off of this.#unlisten.values()) off.then((f) => f());
		this.#unlisten.clear();
		this.#started = false;
	}

	/** `onListed` runs as soon as the project list is in, before the runners are attached. */
	async refresh(onListed?: () => void) {
		try {
			this.projects = await api.list();
			onListed?.();
		} catch (e) {
			toast.error(errText(e));
			return;
		}
		const attached: Promise<unknown>[] = [];
		for (const p of this.projects) {
			if (!this.live[p.id]) this.live[p.id] = emptyLive();
			// A missing checkout has nothing to watch or load; it is wired once it is found again.
			if (p.missing) continue;
			if (!this.#unlisten.has(p.id)) {
				// Reserve the slot before awaiting: refresh() runs concurrently (init, focus, events).
				const off = listen<RunnerEvent>(`project://${p.id}`, (e) => this.apply(p.id, e.payload));
				this.#unlisten.set(p.id, off);
				attached.push(off.then(() => api.resync(p.id)).catch((e) => toast.error(errText(e))));
			}
			this.loadConfig(p.id);
		}
		await Promise.all(attached);
		for (const [id, off] of this.#unlisten) {
			if (!this.projects.some((p) => p.id === id)) {
				off.then((f) => f());
				this.#unlisten.delete(id);
				delete this.live[id];
				delete this.configs[id];
				this.#logs.delete(id);
				for (const k of this.#lastAlert.keys()) if (k.startsWith(`${id}:`)) this.#lastAlert.delete(k);
			}
		}
		if (this.selected && !this.projects.some((p) => p.id === this.selected)) {
			this.selected = this.sorted[0]?.id ?? null;
		}
	}

	async loadConfig(id: string) {
		try {
			this.configs[id] = await api.projectConfig(id);
		} catch {
			// Invalid config: the project's `error` field already says why.
		}
	}

	/** The event reducer: runner JSON lines → live state. */
	apply(id: string, ev: RunnerEvent) {
		const s = this.live[id];
		if (!s) return;
		// Snapshots are history replays (resync); only live row events may sound.
		const before = ev.t === "snapshot" ? undefined : s.services.map((x) => ({ ...x }));
		this.#reduce(id, s, ev);
		const cue = before && stackSound(before, s.services);
		if (cue) sound.play(cue);
	}

	#reduce(id: string, s: Live, ev: RunnerEvent) {
		switch (ev.t) {
			case "snapshot":
				s.services = ev.services ?? [];
				s.tunnel = ev.tunnel ?? null;
				s.mode = ev.mode ?? null;
				s.version = ev.version ?? null;
				s.operation = ev.operation ?? null;
				mergeSnapshot(s.runs, ev.action_runs, Date.now());
				{
					// A resync replaces the buffer; the ring object is replaced too so views rebuild.
					const ring = new Ring<LogLine>(MAX_LOGS);
					ring.push((ev.logs ?? []).slice(-MAX_LOGS).map((l) => toLogLine(l, seq++)));
					this.#logs.set(id, ring);
					s.logsLoaded = true;
					this.#touchLogs(id);
				}
				s.events = (ev.events ?? [])
					.map(timeline)
					.filter((e, k, all): e is TimelineEvent => e !== null && e.text !== all[k - 1]?.text);
				break;
			case "poll":
				s.services = ev.services ?? [];
				s.tunnel = ev.tunnel ?? null;
				s.mode = ev.mode ?? null;
				break;
			case "rows":
				s.services = ev.services ?? [];
				s.tunnel = ev.tunnel ?? null;
				break;
			case "state": {
				const i = s.services.findIndex((x) => x.name === ev.name);
				const { t: _, ...row } = ev;
				// The runner re-sends `state` on health ticks; only transitions go to the timeline.
				const prev = i >= 0 ? s.services[i] : undefined;
				if (!prev || prev.state !== row.state) this.#event(s, ev);
				if (!prev) s.services.push(row);
				else if (rowChanged(prev, row)) s.services[i] = row;
				break;
			}
			case "log":
				this.#append(id, [{ ...ev, ts: ev.ts ?? Date.now() }]);
				break;
			case "logs":
				this.#append(id, ev.lines);
				break;
			// The runner skipped lines while this client fell behind (they are in .logs/).
			case "gap":
				this.#append(id, [
					{
						name: "",
						line: `${ev.skipped} lines skipped while catching up (full output in .logs/)`,
						stream: "stdout",
						ts: Date.now(),
						kind: "mark",
					},
				]);
				break;
			case "note":
				this.#event(s, ev);
				break;
			case "tunnel":
				s.tunnel = ev.tunnel ?? null;
				this.#event(s, ev);
				break;
			case "alert":
				this.#event(s, ev);
				this.#alert(`${id}:${ev.service}:${ev.message}`, `${ev.service}: ${ev.message}`);
				break;
			case "action_progress": {
				const { run, started } = pushLine(s.runs, ev.label, ev.line, Date.now());
				// First line of a new operation = the action started.
				if (started) this.#push(s, { ts: run.start, kind: "action", text: `${ev.label}: started` });
				s.operation = { label: ev.label, detail: ev.line };
				break;
			}
			case "action_done": {
				s.operation = null;
				// Empty label = a session's resume ping, not an action.
				if (!finish(s.runs, ev.label, ev.ok, Date.now())) break;
				this.#push(s, {
					ts: Date.now(),
					kind: "action",
					text: `${ev.label}: ${ev.ok ? "finished" : "failed"}`,
				});
				// The selected project shows the result chip with its "View output" button instead.
				if (id === this.selected) break;
				if (ev.ok) toast.success(`${ev.label} finished`);
				else toast.error(`${ev.label} failed`);
				break;
			}
			case "bye":
				s.services = s.services.map((x) => ({ ...x, state: "stopped" }));
				s.mode = null;
				s.operation = null;
				abandon(s.runs, Date.now());
				this.refresh();
				break;
			case "config_changed":
				s.configChanged++;
				this.loadConfig(id);
				this.refresh();
				break;
		}
	}

	#append(id: string, raw: RawLog[]) {
		const lines = new Array<LogLine>(raw.length);
		for (let i = 0; i < raw.length; i++) lines[i] = toLogLine(raw[i], seq++);
		this.logs(id).push(lines);
		this.#touchLogs(id);
	}

	#event(s: Live, ev: RunnerEvent) {
		const t = timeline(ev);
		// Crash loops repeat the same line: fold into the latest instead of flooding.
		if (t && s.events.at(-1)?.text !== t.text) this.#push(s, t);
	}

	#push(s: Live, t: TimelineEvent) {
		s.events.push(t);
		if (s.events.length > MAX_EVENTS) s.events.splice(0, s.events.length - MAX_EVENTS);
	}

	/** Run an async op with a busy flag and an error toast. */
	async run<T>(key: string, op: () => Promise<T>): Promise<T | undefined> {
		this.busy[key] = true;
		try {
			return await op();
		} catch (e) {
			toast.error(errText(e), 8000);
		} finally {
			this.busy[key] = false;
		}
	}
}

export const store = new Store();

export { health } from "./health.ts";
