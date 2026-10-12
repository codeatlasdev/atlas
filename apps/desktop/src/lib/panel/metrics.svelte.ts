import { store } from "#lib/stores/projects.svelte.ts";
import {
	LogCursor,
	STEP_MS,
	ServiceHistory,
	bandOf,
	slotOf,
	stackSum,
	sumLatest,
	windowSum,
} from "./metrics.ts";

/** Charts re-render at most this often (ms), however fast samples arrive. */
const PUBLISH_MS = 1000;

interface ProjectMetrics {
	services: Map<string, ServiceHistory>;
	cursor: LogCursor;
	/** The ring object last read: a resync replaces it and the cursor must restart. */
	ring: object | null;
	/** Restart timestamps per service, for "restarts in the last hour". */
	lastRestarts: Map<string, number>;
	restartTimes: number[];
	/** When the stack was last seen going from stopped to up. */
	upSince: number | null;
	lastSlot: number;
}

/**
 * Samples every project's live rows into fixed rings. One timer for the whole app (3 s,
 * the runner's own resolution) plus a log tail per tick that only walks lines appended since
 * the previous one. `rev` is bumped at most once per `PUBLISH_MS`, so charts stay cheap.
 */
class Metrics {
	/** Reactive revision: read it inside a chart `$derived` to re-run after a new sample. */
	rev = $state(0);

	#projects = new Map<string, ProjectMetrics>();
	#timer: ReturnType<typeof setInterval> | null = null;
	#lastPublish = 0;

	#of(id: string): ProjectMetrics {
		let m = this.#projects.get(id);
		if (!m) {
			m = {
				services: new Map(),
				cursor: new LogCursor(),
				ring: null,
				lastRestarts: new Map(),
				restartTimes: [],
				upSince: null,
				lastSlot: -1,
			};
			this.#projects.set(id, m);
		}
		return m;
	}

	/** The history of one service (created on first read; reactive through `rev`). */
	service(id: string, name: string): ServiceHistory {
		const m = this.#of(id);
		let h = m.services.get(name);
		if (!h) m.services.set(name, (h = new ServiceHistory()));
		return h;
	}

	/** Services with history, in the order given by `names`. */
	tracks(id: string, names: string[]): ServiceHistory[] {
		return names.map((n) => this.service(id, n));
	}

	/** Latest slot sampled for a project (the x axis' right edge). */
	end(id: string): number {
		return this.#of(id).lastSlot;
	}

	/** When the stack came up, for the uptime KPI (`null` while stopped). */
	upSince(id: string): number | null {
		return this.#of(id).upSince;
	}

	restartsLastHour(id: string, now: number): number {
		return this.#of(id).restartTimes.filter((t) => now - t < 3_600_000).length;
	}

	errorsLast(id: string, windowMs: number): number {
		const m = this.#of(id);
		let t = 0;
		for (const h of m.services.values()) t += windowSum(h.errors, windowMs, m.lastSlot);
		return t;
	}

	totals(id: string): { mem: number; cpu: number } {
		const hs = [...this.#of(id).services.values()];
		return { mem: sumLatest(hs.map((h) => h.mem)), cpu: sumLatest(hs.map((h) => h.cpu)) };
	}

	stacked(id: string, names: string[], pick: "cpu" | "mem" | "errors", n: number): number[] {
		return stackSum(
			names.map((name) => this.service(id, name)[pick]),
			n,
			this.#of(id).lastSlot,
		);
	}

	/** Start the shared sampler; idempotent. Returns a stop function for the caller's cleanup. */
	start(): () => void {
		if (this.#timer) return () => {};
		this.sample();
		this.#timer = setInterval(() => this.sample(), STEP_MS);
		return () => {
			clearInterval(this.#timer ?? undefined);
			this.#timer = null;
		};
	}

	/** Take one sample of every project now. Exposed so tests and the timer share one path. */
	sample(now = Date.now()) {
		const slot = slotOf(now);
		for (const p of store.projects) {
			const live = store.live[p.id];
			if (!live) continue;
			const m = this.#of(p.id);
			m.lastSlot = slot;

			for (const s of live.services) {
				const h = this.service(p.id, s.name);
				h.cpu.set(slot, s.cpu ?? 0);
				h.mem.set(slot, s.mem_mb ?? 0);
				h.state(bandOf(s.state), now);
				const prev = m.lastRestarts.get(s.name);
				if (prev != null && s.restarts > prev) m.restartTimes.push(now);
				m.lastRestarts.set(s.name, s.restarts);
			}
			if (m.restartTimes.length > 200) m.restartTimes.splice(0, m.restartTimes.length - 200);

			const up = p.running && live.services.some((s) => s.state === "running" || s.state === "starting");
			if (up && m.upSince == null) m.upSince = now;
			else if (!up) m.upSince = null;

			this.#tail(p.id, m, slot);
		}
		if (now - this.#lastPublish >= PUBLISH_MS) {
			this.#lastPublish = now;
			this.rev++;
		}
	}

	/** Count lines (and error-level ones) appended since the previous tick, per service. */
	#tail(id: string, m: ProjectMetrics, slot: number) {
		const ring = store.logs(id);
		if (m.ring !== ring) {
			m.ring = ring;
			m.cursor.reset();
		}
		m.cursor.read(ring, (l) => {
			if (!l.name || l.mark) return;
			const h = this.service(id, l.name);
			h.lines.add(slot, 1);
			if (l.level === "error") h.errors.add(slot, 1);
		});
	}
}

export const metrics = new Metrics();
