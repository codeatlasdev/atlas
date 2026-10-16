<script lang="ts">
	/**
	 * The content area as a pager: the current space and its previous/next neighbours are mounted
	 * as full-width layers. A switch never mounts the incoming space; it only moves transforms
	 * (`spacePager`, pager.ts) and flips roles. Neighbours rest hidden (`content-visibility`,
	 * `inert`) so they cost no layout or paint until a gesture (or `warm`) shows them.
	 */
	import { invoke } from "@tauri-apps/api/core";
	import { onMount, tick } from "svelte";
	import { spacePager } from "#lib/shell/pager.ts";
	import { engineGo, nativeActive, initNativeSwipe, pushZones } from "#lib/shell/nativeSwipe.ts";
	import { createStageSync } from "#lib/shell/stageSync.ts";
	import { dirtyPage, pagesChanged, type PagesState } from "#lib/shell/pageDirty.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { neighbour } from "#lib/shell/space.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { nativeRect } from "#lib/zoom.ts";
	import SpaceSlot from "./SpaceSlot.svelte";

	let root: HTMLElement | undefined = $state();
	/** A gesture or spring is running: natives parked, the ids in `kept` stay mounted. */
	let moving = $state(false);
	let kept = $state<string[]>([]);
	const layers: Record<string, HTMLElement> = {};

	/** Which side a layer parks on at rest: the previous neighbour (wrap included) on the left, the rest on the right. */
	const side = (id: string) =>
		id === neighbour(shell.spaceIds, shell.activeId, -1) &&
		id !== neighbour(shell.spaceIds, shell.activeId, 1)
			? "prev"
			: "next";

	/** The mounted window (`shell.mounted`: active ± 1) + whatever a running gesture still needs. */
	let ids = $derived.by(() => {
		if (!shell.activeId) return [];
		const list = [...shell.mounted];
		for (const k of kept) if (k && !list.includes(k)) list.push(k);
		return list;
	});

	function layer(node: HTMLElement, id: string) {
		layers[id] = node;
		return {
			destroy() {
				if (layers[id] === node) delete layers[id];
			},
		};
	}

	/**
	 * Snapshots render at this fraction of the pager's width in points (so a quarter of the pixels at 0.5). Measured
	 * on a 2x display, 1264x774 pt: 36 ms at 1.0, 22 ms at 0.75, 11 ms at 0.5, 8 ms at 0.35. `takeSnapshot` runs on
	 * the main thread, so at 0.5 a swipe that begins during a warm-up waits one frame instead of two; the slide is
	 * 300 ms of motion, where 1x pixel density on a retina panel reads as motion blur, not as softness.
	 */
	const SNAP_SCALE = 0.5;

	/** Two animation frames: the DOM change has been laid out, painted and committed to the compositor. */
	const painted = () => {
		const { promise, resolve } = Promise.withResolvers<void>();
		requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
		return promise;
	};

	/** Last selection, gesture or content change: warm-ups wait for the app to be quiet (stageSync QUIET_MS). */
	let lastActivity = performance.now();
	const touch = () => (lastActivity = performance.now());

	/** Is a native-drawn or JS-drawn gesture in flight (warm-ups must not touch the DOM then). */
	let gesture = false;

	const sync = createStageSync({
		ids: () => shell.spaceIds,
		active: () => shell.activeId,
		show(id) {
			const el = layers[id];
			if (!el) return;
			// On top of the origin, in view: invisible to the user behind the native cover.
			el.style.transform = "translate3d(0,0,0)";
			el.style.zIndex = "1";
			el.style.visibility = "visible";
		},
		park(id) {
			const el = layers[id];
			if (!el) return;
			el.style.removeProperty("transform");
			el.style.removeProperty("z-index");
			el.style.removeProperty("visibility");
		},
		nextFrame: painted,
		cover: () => invoke<boolean>("pager_cover"),
		uncover: () => invoke<void>("pager_uncover"),
		snapshot: (id) => invoke<boolean>("pager_snapshot", { id, scale: SNAP_SCALE }),
		pages: pushPages,
		idle: (f) =>
			typeof requestIdleCallback === "function"
				? requestIdleCallback(() => void f(), { timeout: 800 })
				: setTimeout(() => void f(), 50),
		gestureActive: () => gesture,
		quietMs: () => performance.now() - lastActivity,
	});

	let pushedPages: PagesState | null = null;
	/**
	 * Tell native the order and the pager rect. `measure` reads the rect (forces layout): only when the size may
	 * have changed (resize, first push) and never for a plain selection, which changes neither.
	 */
	function pushPages(measure = false) {
		const order = [...shell.spaceIds];
		let rect = pushedPages?.rect;
		if (measure || !rect) {
			const r = root?.getBoundingClientRect();
			if (!r) return Promise.resolve();
			// Native lays its snapshot layers out in window points; the page measures CSS px (they differ under layout zoom).
			const n = nativeRect({ x: r.x, y: r.y, w: r.width, h: r.height });
			rect = [n.x, n.y, n.w, n.h];
		}
		const next = { order, rect };
		if (!pagesChanged(pushedPages, next)) return Promise.resolve();
		pushedPages = next;
		return invoke<void>("pager_pages", {
			order,
			active: shell.activeId,
			rect: { x: rect[0], y: rect[1], w: rect[2], h: rect[3] },
		});
	}

	/** The pages to keep snapshotted: the page on screen first, then its neighbours. */
	const wanted = () =>
		shell.mounted.length ? [shell.activeId!, ...shell.mounted.filter((x) => x !== shell.activeId)] : [];

	onMount(() => {
		const detach = spacePager.attach({
			layer: (id) => layers[id],
			width: () => root?.clientWidth ?? 0,
			ids: () => shell.spaceIds,
			active: () => shell.activeId,
			commit: (id) => shell.commitSpace(id),
			moving(on, keep) {
				kept = on ? (keep ?? []) : [];
				moving = on;
				gesture = on;
				touch();
				if (on) sync.abort();
				else sync.resume();
			},
			flush: () => tick(),
			ack: (seq) => void painted().then(() => invoke("pager_ready", { seq })),
			engineGo: (dir) => engineGo("space", dir),
		});
		let stopped = false;
		void initNativeSwipe().then(async () => {
			if (stopped || !nativeActive()) return;
			await pushPages(true);
			sync.warm(wanted());
		});
		// Geometry follows layout; content changes make a page's snapshot stale.
		const ro = new ResizeObserver(() => {
			void pushPages(true);
			for (const id of shell.spaceIds) sync.dirty(id);
			sync.warm(wanted());
		});
		if (root) ro.observe(root);
		const mo = new MutationObserver((recs) => {
			for (const r of recs) {
				const t = r.target as HTMLElement;
				const layer = t.closest?.(".space-layer");
				const id = dirtyPage({
					type: r.type,
					attributeName: r.attributeName,
					targetIsLayerRoot: t === layer,
					page: layer?.getAttribute("data-space") ?? null,
				});
				if (id) {
					sync.dirty(id);
					touch();
				}
			}
			sync.warmIfStale(wanted());
		});
		if (root)
			mo.observe(root, {
				subtree: true,
				childList: true,
				characterData: true,
				attributes: true,
				attributeFilter: ["class", "src", "data-state"],
			});
		return () => {
			stopped = true;
			ro.disconnect();
			mo.disconnect();
			detach();
		};
	});

	// The order, the active page or the mounted window changed: tell native and refresh what moved into reach.
	// Never in the selection's synchronous flush (it reads layout): after the next paint, coalesced.
	let pushed = 0;
	$effect(() => {
		void [shell.spaceIds, shell.activeId, shell.mounted];
		touch();
		if (!nativeActive()) return;
		// which neighbours exist depends on the active space (no wrap): the sidebar's zone must say so
		pushZones();
		cancelAnimationFrame(pushed);
		pushed = requestAnimationFrame(() => {
			pushed = requestAnimationFrame(() => void pushPages().then(() => sync.warmIfStale(wanted())));
		});
		return () => cancelAnimationFrame(pushed);
	});
</script>

<div class="pager" bind:this={root} class:moving>
	{#each ids as id (id)}
		{@const current = id === shell.activeId}
		{@const v = shell.layerVars(id)}
		<div
			class="space-layer"
			style:--space-accent={v.accent}
			style:--space-accent-text={v.text}
			style:--space-on-accent={v.onAccent}
			style:--space-tint={v.tint}
			data-space={id}
			data-role={current ? "current" : "near"}
			data-side={current ? undefined : side(id)}
			inert={!current}
			aria-hidden={current ? undefined : true}
			use:layer={id}
		>
			<SpaceSlot {id} {current} {moving} />
		</div>
	{/each}
</div>

<style>
	.pager {
		position: relative;
		flex: 1;
		min-height: 0;
		min-width: 0;
		overflow: hidden;
	}
	/* Layers are stacked, each the size of the pager; position comes from `translate3d`, written
	   by pager.ts only while a switch runs. At rest the current layer has no transform at all. */
	.space-layer {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	/* At rest a neighbour is parked one page away, laid out and composited but out of reach (inert,
	   no hit-testing, not in the a11y tree), so a gesture only has to move a transform: no layout
	   or first paint at gesture start. `contain` keeps its updates from touching the current layer. */
	.space-layer[data-role="near"] {
		contain: layout paint style;
		pointer-events: none;
		will-change: transform;
		transform: translate3d(100%, 0, 0);
	}
	.space-layer[data-side="prev"] {
		transform: translate3d(-100%, 0, 0);
	}
	/* Inline `transform` (pager.ts) wins over the parked offset while a switch runs. */
	.pager.moving .space-layer {
		will-change: transform;
	}
</style>
