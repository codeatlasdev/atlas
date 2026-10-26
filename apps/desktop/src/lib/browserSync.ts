/**
 * Frontend half of the embedded-browser lifecycle. The native child webview outlives this page (reload, HMR,
 * ⌘W/⌘⇧T), so two rules keep it exactly over its pane or hidden:
 *
 * - `reconcile`: on boot, close native views whose tab is gone; the rest stay parked until their `BrowserTab`
 *   measures itself.
 * - `browserSurface`: the browser's ADAPTER over the native-view core (`native/surface.ts`). The core owns the
 *   serialized open → place → release chain (always with the rect measured when the call *runs*, never a stale one;
 *   after `dispose` nothing can un-park the view again), the slot hole in the page, the loading strip and the
 *   reconciler; this file only says how a browser view is opened, placed and parked, and when it may open.
 *
 * No Tauri import: `invoke` is injected, so this is unit-testable.
 */

import { createNativeSurface, type NativeSurface, type SurfaceOptions } from "./native/surface.ts";

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

export interface BrowserSurfaceOptions {
	tab: string;
	invoke: Invoke;
	/** The pane the page stands in for. */
	slot: () => HTMLElement | undefined;
	/** Should the page be on screen right now (active tab)? A swipe or an overlay is the core's business. */
	shown: () => boolean;
	/** The page failed to load: it is parked behind the tab's own error screen, never a white rectangle. */
	failed?: () => boolean;
	/** URL to load when the view has to be created; empty = nothing to open yet. */
	url: () => string | undefined;
	onError: (e: unknown) => void;
	/** A load that never reported back in `stallMs`: the page's error screen. */
	onStall?: () => void;
	stallMs?: number;
}

export function browserSurface(o: BrowserSurfaceOptions, opts?: SurfaceOptions): NativeSurface {
	let url = "";
	return createNativeSurface(
		{
			kind: "browser",
			id: o.tab,
			slot: o.slot,
			visible: o.shown,
			extraHidden: o.failed,
			// Created parked (a restored background tab never flashes over the UI), but only once there is a page to load.
			canOpen: () => {
				url = o.url() ?? "";
				return !!url;
			},
			// `browser_open` is idempotent in Rust (reuse + reposition), so calling it on every mount is safe and
			// re-adopts a view that survived a reload.
			open: (rect, visible) => o.invoke("browser_open", { tab: o.tab, url, ...rect, visible }),
			place: (rect, visible) => o.invoke("browser_bounds", { tab: o.tab, ...rect, visible }),
			// Hidden, not closed: the tab may only be re-parented (split/move). `closeTab` closes it.
			release: () => o.invoke("browser_park", { tab: o.tab }).catch(() => {}),
			focus: () => void o.invoke("browser_focus", { tab: o.tab }).catch(() => {}),
			onError: (e) => o.onError(e),
			onStall: o.onStall,
			stallMs: o.stallMs,
		},
		opts,
	);
}
