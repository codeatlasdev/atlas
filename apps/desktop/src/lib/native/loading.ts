/**
 * The loading state of one native surface, owned by the core for every kind (a browser page load, a terminal that is
 * opening, a future kind's own). Adapters only report events (`start`, `progress`, `finish`, `fail`); this decides what
 * the user sees and when to give up. Pure and timer-injected: the surface wires it to the indicator (`indicator.ts`).
 *
 *  - The indicator shows only after `SHOW_AFTER_MS` of loading, so a fast load never flashes a bar.
 *  - It ends on `finish` or `fail`, and on a stall: `stallMs` (default `STALL_MS`) without ANY event ends the load and
 *    calls `onStall` once. Every `start` (a redirect's `nav`), `progress` and `finish` of another source re-arms that
 *    timer, so a slow page that keeps talking is never marked failed; only silence stalls.
 *  - Several sources may load at once (the core's own `open`, the page's `load`): the surface is loading until all end.
 */

/** A load must be this long before its indicator appears (no flash on a fast load). */
export const SHOW_AFTER_MS = 150;
/** Silence this long ends a load as stalled. */
export const STALL_MS = 15_000;

export interface LoadingState {
	/** Something is loading (any source). */
	active: boolean;
	/** The indicator is up: active for longer than `SHOW_AFTER_MS`. It fades out by CSS when this turns false. */
	shown: boolean;
}

export interface Timers {
	set(fn: () => void, ms: number): unknown;
	clear(id: unknown): void;
}

export interface LoadingOptions {
	/** The state changed (`active` or `shown`). */
	onChange?: (s: LoadingState) => void;
	/** No event for `stallMs`: the load is over, it never finished. Called once per load. */
	onStall?: () => void;
	stallMs?: number;
	/** `setTimeout`/`clearTimeout`; injected by the tests. */
	timers?: Timers;
}

export interface Loading {
	/** Begin loading under `source` (default `"load"`). Starting a source that is already loading re-arms the stall timer only. */
	start(source?: string): void;
	/** The load is alive (a redirect, a title, bytes): re-arm the stall timer. */
	progress(source?: string): void;
	/** `source` ended well. */
	finish(source?: string): void;
	/** `source` ended badly; same as `finish` for the indicator (the adapter shows the failure itself). */
	fail(source?: string): void;
	state(): LoadingState;
	/** Cancel every timer; nothing is reported afterwards. */
	dispose(): void;
}

const real: Timers = { set: (fn, ms) => setTimeout(fn, ms), clear: (id) => clearTimeout(id as number) };

export function createLoading(o: LoadingOptions = {}): Loading {
	const timers = o.timers ?? real;
	const stallMs = o.stallMs ?? STALL_MS;
	const sources = new Set<string>();
	let shown = false;
	let showTimer: unknown;
	let stallTimer: unknown;
	let disposed = false;

	const state = (): LoadingState => ({ active: sources.size > 0, shown });
	const emit = () => {
		if (!disposed) o.onChange?.(state());
	};

	function disarm() {
		timers.clear(showTimer);
		timers.clear(stallTimer);
		showTimer = stallTimer = undefined;
	}
	function arm() {
		timers.clear(stallTimer);
		stallTimer = timers.set(() => {
			stallTimer = undefined;
			end(true);
		}, stallMs);
	}
	function end(stalled: boolean) {
		disarm();
		sources.clear();
		shown = false;
		emit();
		if (stalled && !disposed) o.onStall?.();
	}
	function drop(source: string) {
		if (!sources.delete(source)) return;
		if (sources.size > 0) return arm();
		disarm();
		shown = false;
		emit();
	}

	return {
		start(source = "load") {
			if (disposed) return;
			const first = sources.size === 0;
			sources.add(source);
			arm();
			if (first) {
				emit();
				showTimer = timers.set(() => {
					showTimer = undefined;
					shown = true;
					emit();
				}, SHOW_AFTER_MS);
			}
		},
		progress() {
			if (!disposed && sources.size > 0) arm();
		},
		finish: (source = "load") => drop(source),
		fail: (source = "load") => drop(source),
		state,
		dispose() {
			disposed = true;
			disarm();
			sources.clear();
			shown = false;
		},
	};
}
