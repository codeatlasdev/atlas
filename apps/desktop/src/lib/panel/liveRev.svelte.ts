import { getContext, setContext } from "svelte";
import { metrics } from "./metrics.svelte.ts";

/**
 * The metrics revision a panel's charts redraw on. `metrics.rev` moves for every sample (3 s) whether or not anyone
 * can see the panel; a running stack paid 2.5-3 % of a core in the web process for charts nobody looked at (#289,
 * docs/perf/linux-webkit-round2.md). The panel provides a revision that follows `metrics.rev` only while `on()` holds
 * (its tab is the visible one and the window is focused) and stands still otherwise, so a chart derived from it does
 * not recompute at all while hidden, and takes the newest sample, once, the moment `on()` turns true again.
 */
export interface Revision {
	readonly rev: number;
}

const KEY = Symbol("panel live revision");

export function provideLiveRev(on: () => boolean): Revision {
	let last = metrics.rev;
	const rev = $derived.by(() => {
		if (on()) last = metrics.rev;
		return last;
	});
	const live: Revision = {
		get rev() {
			return rev;
		},
	};
	setContext(KEY, live);
	return live;
}

/** What a chart under a panel reads instead of `metrics.rev`; outside a panel it is `metrics` itself (always live). */
export function liveRev(): Revision {
	return getContext<Revision | undefined>(KEY) ?? metrics;
}
