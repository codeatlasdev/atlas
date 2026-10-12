/**
 * Frontend half of the embedded-browser lifecycle. The native child webview outlives this
 * page (reload, HMR, ⌘W/⌘⇧T), so two rules keep it exactly over its pane or hidden:
 *
 * - `reconcile`: on boot, close native views whose tab is gone; the rest stay parked until
 *   their `BrowserTab` measures itself.
 * - `paneSync`: one tab's open/bounds/park calls run strictly in order (observers fire
 *   concurrently), always with the rect measured when the call *runs*, never a stale one;
 *   after `dispose` nothing can un-park the view again.
 *
 * No DOM or Tauri imports: both are injected, so this is unit-testable.
 */

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

/** Native views (tab ids) with no tab behind them. */
export const staleViews = (live: string[], tabIds: Iterable<string>): string[] => {
	const keep = new Set(tabIds);
	return live.filter((id) => !keep.has(id));
};

/** Close every native browser view whose tab no longer exists; returns the closed ids. */
export async function reconcile(invoke: Invoke, tabIds: Iterable<string>): Promise<string[]> {
	const live = (await invoke("browser_list")) as string[];
	const stale = staleViews(live, tabIds);
	await Promise.all(stale.map((tab) => invoke("browser_close", { tab }).catch(() => {})));
	return stale;
}

export interface PaneSyncOptions {
	tab: string;
	invoke: Invoke;
	/** Current rect of the pane, or null when it is not laid out (no element). */
	measure: () => Rect | null;
	/** Should the view be on screen right now (active tab, no overlay)? */
	shown: () => boolean;
	/** URL to load when the view has to be created; empty = nothing to open yet. */
	url: () => string | undefined;
	onError: (e: unknown) => void;
}

export interface PaneSync {
	/** Queue a bounds sync. Coalesces: at most one run waits behind the running one. */
	sync: () => Promise<void>;
	/** Park the view and stop all further syncs (component destroyed). */
	dispose: () => Promise<void>;
	/** Resolves when everything queued so far has run. */
	settled: () => Promise<void>;
	/** The native view exists (this component opened or re-adopted it). */
	isOpen: () => boolean;
}

export function paneSync(o: PaneSyncOptions): PaneSync {
	let chain: Promise<void> = Promise.resolve();
	let queued = false;
	let disposed = false;
	let opened = false;

	async function apply() {
		queued = false;
		if (disposed) return;
		const url = o.url();
		const r = o.measure();
		if (!url || !r) return;
		try {
			// `browser_open` is idempotent in Rust (reuse + reposition), so calling it on every
			// mount is safe and re-adopts a view that survived a reload.
			if (!opened) {
				await o.invoke("browser_open", { tab: o.tab, url, ...r, visible: o.shown() });
				opened = true;
			}
			// Re-measure after the await: layout may have moved while open was in flight.
			const now = o.measure() ?? r;
			if (disposed) return;
			await o.invoke("browser_bounds", { tab: o.tab, ...now, visible: o.shown() });
		} catch (e) {
			o.onError(e);
		}
	}

	function sync() {
		if (disposed) return chain;
		// A run is already waiting and will measure fresh when it starts: no need for another.
		if (!queued) {
			queued = true;
			chain = chain.then(apply).catch(o.onError);
		}
		return chain;
	}

	async function dispose() {
		disposed = true;
		await chain;
		// Hidden, not closed: the tab may only be re-parented (split/move). closeTab closes it.
		await o.invoke("browser_park", { tab: o.tab }).catch(() => {});
	}

	return { sync, dispose, settled: () => chain, isOpen: () => opened };
}
