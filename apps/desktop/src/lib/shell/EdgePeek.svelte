<script lang="ts">
	import type { Snippet } from "svelte";
	import { peek } from "#lib/stores/docks.svelte.ts";
	import {
		CLOSE_DELAY,
		OPEN_DELAY,
		peekAction,
		regionOf,
		sameRegion,
		type PeekAction,
		type PeekRegion,
	} from "#lib/shell/peek.ts";

	/**
	 * Arc-style peek for a hidden sidebar. The panel is always mounted and pre-rendered; showing
	 * it only flips a transform (compositor-only: no layout, no mount, and the backdrop blur
	 * layer is static, it is translated, never re-rasterized).
	 *
	 * The pointer handler is O(1) and allocation-free: it runs in a rAF-coalesced tick, keeps the
	 * hot-zone state in plain variables and touches reactive state only when the pointer crosses
	 * a region boundary (enter / leave), never per move.
	 *
	 * `active` is true while the sidebar is hidden (the peek is armed); `size` is the panel width.
	 */
	let {
		edge,
		size,
		label,
		active = true,
		children,
	}: { edge: "left" | "right"; size: number; label: string; active?: boolean; children: Snippet } = $props();

	let panel: HTMLElement | undefined;
	let timer = 0;
	let raf = 0;
	// Plain (non-reactive) hot-path state.
	let mx = 0;
	let lastX = 0;
	let lastT = 0;
	let speed = 0;
	let region: PeekRegion = { hot: false, over: false };
	let open = false;

	const clear = () => (clearTimeout(timer), (timer = 0));

	/** The single write that crosses into reactivity and the DOM. */
	function set(on: boolean) {
		if (open === on) return;
		open = on;
		peek[edge] = on;
		if (panel) {
			panel.classList.toggle("shown", on);
			panel.toggleAttribute("inert", !on);
			panel.setAttribute("aria-hidden", String(!on));
			// Native views (browser, terminal) cannot be covered by CSS: `watchSlot` sees this flip and parks them.
			panel.dataset.state = on ? "open" : "closed";
		}
	}

	/** Grace-delayed close; a menu or dialog opened from the panel lives in a portal, so wait for it. */
	function closeSoon() {
		if (document.querySelector("[role=menu], [role=dialog], [role=listbox]")) {
			timer = window.setTimeout(closeSoon, CLOSE_DELAY);
			return;
		}
		set(false);
	}

	function act(a: PeekAction) {
		if (a.do === "none") return;
		clear();
		if (a.do === "schedule-open") timer = window.setTimeout(() => set(true), OPEN_DELAY);
		else if (a.do === "open-now") set(true);
		else if (a.do === "schedule-close") timer = window.setTimeout(closeSoon, CLOSE_DELAY);
		else if (a.do === "close-now") set(false);
	}

	/** Runs at most once per frame, with the latest pointer position. */
	function tick() {
		raf = 0;
		const w = window.innerWidth;
		const next = regionOf(edge, mx, w, size, open);
		if (sameRegion(next, region)) return; // same region: nothing to decide, nothing to write
		region = next;
		// Speed toward the edge, px/ms (left edge: moving left is approaching).
		const toward = edge === "left" ? -speed : speed;
		act(peekAction({ open }, { t: "move", hot: next.hot, over: next.over, speed: toward }));
	}

	function onmove(e: PointerEvent) {
		if (e.buttons) return; // dragging something (a split, a selection): never pop a panel
		const now = e.timeStamp;
		const dt = now - lastT;
		if (dt > 0) speed = speed * 0.5 + ((e.clientX - lastX) / dt) * 0.5;
		lastX = mx = e.clientX;
		lastT = now;
		if (!raf) raf = requestAnimationFrame(tick);
	}

	$effect(() => {
		if (!active) return;
		window.addEventListener("pointermove", onmove, { passive: true });
		const leave = () => act(peekAction({ open }, { t: "leave-window" }));
		document.documentElement.addEventListener("pointerleave", leave);
		return () => {
			window.removeEventListener("pointermove", onmove);
			document.documentElement.removeEventListener("pointerleave", leave);
			cancelAnimationFrame(raf);
			raf = 0;
			clear();
			region = { hot: false, over: false };
			set(false);
		};
	});

	// Something else dropped the peek (toggleDock / toggleSidebar): resync the plain flag and the DOM.
	$effect(() => {
		if (!peek[edge] && open) set(false);
	});

	// Escape dismisses a peek, as it does any transient surface.
	function onkeydown(e: KeyboardEvent) {
		if (e.key === "Escape" && open) act(peekAction({ open: true }, { t: "force-close" }));
	}
</script>

<svelte:window {onkeydown} />

{#if active}<div class="edge-zone {edge}" aria-hidden="true"></div>{/if}
<div
	bind:this={panel}
	class="peek-panel {edge}"
	style:--sidebar-w="{size}px"
	role="complementary"
	aria-label={label}
	aria-hidden="true"
	inert
>
	<div class="peek-glass" aria-hidden="true"></div>
	<div class="peek-body">
		{#if active}{@render children()}{/if}
	</div>
</div>
