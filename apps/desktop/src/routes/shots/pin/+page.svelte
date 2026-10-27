<script lang="ts">
	// Pinned screenshot (`shots-pin-<n>`): a borderless window floating above everything. Rust owns the window
	// (`shots_pin_*`); this page shows the picture and turns keys, wheel and pointer into the pure model of
	// `#lib/shots/pin.ts`. Drag the body to move it; wheel or pinch to zoom (25%–400%), Alt + wheel or [ ] to fade
	// (20%–100%), + / − to zoom, double click for 100%, Esc or the hover button to close. The window is a
	// non-activating panel, so keys only arrive after a click gave it the keyboard (`shots_pin_keys`), which it
	// hands back when it loses focus.
	import { invoke } from "@tauri-apps/api/core";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { TriangleAlert, X } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		fitZoom,
		initialState,
		keyEvent,
		percent,
		reduce,
		type PinEffect,
		type PinEvent,
		type PinState,
	} from "#lib/shots/pin.ts";
	import type { EditorDoc } from "#lib/shots/types.ts";

	let doc = $state<EditorDoc | null>(null);
	let st = $state<PinState>(initialState({ w: 1, h: 1 }));
	let failure = $state<string | null>(null);
	/** The zoom label shows for a moment after a change, then fades. */
	let badge = $state(false);
	let badgeTimer = 0;

	function dispatch(ev: PinEvent) {
		const r = reduce(st, ev);
		st = r.state;
		if (ev.type !== "applied" && ev.type !== "close" && r.effects.length > 0) flash();
		for (const e of r.effects) void run(e);
	}

	async function run(e: PinEffect) {
		try {
			if (e.type === "close") {
				await invoke("shots_pin_close");
			} else {
				const applied = await invoke<[number, number]>("shots_pin_resize", {
					width: e.size.w,
					height: e.size.h,
				});
				dispatch({ type: "applied", size: { w: applied[0], h: applied[1] } });
			}
		} catch (err) {
			failure = errText(err);
		}
	}

	function flash() {
		badge = true;
		window.clearTimeout(badgeTimer);
		badgeTimer = window.setTimeout(() => (badge = false), 900);
	}

	/** Press on the picture: take the keyboard (for + − [ ] Esc) and start moving the window. */
	function onPointerDown(ev: PointerEvent) {
		if (ev.button !== 0 || (ev.target as HTMLElement).closest("button")) return;
		void invoke("shots_pin_keys", { on: true }).catch(() => {});
		// A double click is a zoom reset, not the start of a drag.
		if (ev.detail >= 2) return;
		void getCurrentWindow()
			.startDragging()
			.catch(() => {});
	}

	function onWheel(ev: WheelEvent) {
		ev.preventDefault();
		// Alt can turn the vertical delta into a horizontal one on some mice; take whichever moved.
		const deltaY = ev.deltaY !== 0 ? ev.deltaY : ev.deltaX;
		dispatch({ type: "wheel", deltaY, alt: ev.altKey, pinch: ev.ctrlKey });
	}

	function onKey(ev: KeyboardEvent) {
		const e = keyEvent(ev);
		if (!e) return;
		ev.preventDefault();
		dispatch(e);
	}

	onMount(() => {
		const q = new URLSearchParams(location.search);
		const path = q.get("path") ?? "";
		void invoke<EditorDoc>("shots_pin_load", { path }).then(
			(d) => {
				doc = d;
				// 100% is the picture's size in points; the window opened at the fitted size already.
				const image = {
					w: d.width / (window.devicePixelRatio || 1),
					h: d.height / (window.devicePixelRatio || 1),
				};
				const zoom = fitZoom(image, { w: screen.availWidth, h: screen.availHeight });
				st = initialState(image, zoom);
			},
			(err) => (failure = errText(err)),
		);
		window.addEventListener("keydown", onKey);
		window.addEventListener("wheel", onWheel, { passive: false });
		// The panel gives the keyboard back when another window is clicked.
		const blur = () => void invoke("shots_pin_keys", { on: false }).catch(() => {});
		window.addEventListener("blur", blur);
		return () => {
			window.removeEventListener("keydown", onKey);
			window.removeEventListener("wheel", onWheel);
			window.removeEventListener("blur", blur);
			window.clearTimeout(badgeTimer);
		};
	});
</script>

<svelte:head><title>Pinned screenshot</title></svelte:head>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="pin"
	role="group"
	aria-label="Pinned screenshot"
	style:opacity={st.opacity}
	onpointerdown={onPointerDown}
	ondblclick={() => dispatch({ type: "reset" })}
>
	{#if doc}
		<img src={doc.src} alt="Pinned screenshot" draggable="false" />
	{/if}
	<button class="x" aria-label="Close pin" onclick={() => dispatch({ type: "close" })}
		><X size={12} aria-hidden="true" /></button
	>
	{#if badge}
		<span class="badge" role="status">{percent(st.zoom)} · {percent(st.opacity)}</span>
	{/if}
	{#if failure}
		<p class="err" role="alert"><TriangleAlert size={12} aria-hidden="true" /> {failure}</p>
	{/if}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		margin: 0;
		overflow: hidden;
	}
	.pin {
		position: fixed;
		inset: 0;
		border-radius: 6px;
		overflow: hidden;
		cursor: grab;
		user-select: none;
		-webkit-user-select: none;
		box-shadow: inset 0 0 0 1px color-mix(in oklab, black 30%, transparent);
	}
	img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: fill;
		pointer-events: none;
	}
	.x {
		position: absolute;
		top: 6px;
		left: 6px;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-surface-raised) 88%, transparent);
		box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
		cursor: pointer;
		opacity: 0;
	}
	.pin:hover .x,
	.x:focus-visible {
		opacity: 1;
	}
	.x:focus-visible {
		outline: 2px solid var(--ca-accent, #3b82f6);
		outline-offset: 1px;
	}
	.badge {
		position: absolute;
		right: 6px;
		bottom: 6px;
		padding: 2px 8px;
		border-radius: 999px;
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-surface-raised) 88%, transparent);
		box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
	}
	.err {
		position: absolute;
		left: 6px;
		right: 6px;
		bottom: 6px;
		display: flex;
		align-items: flex-start;
		gap: 4px;
		margin: 0;
		padding: 4px 6px;
		border-radius: 6px;
		font-size: 11px;
		color: var(--ca-danger, #ef4444);
		background: color-mix(in oklab, var(--ca-surface-raised) 92%, transparent);
	}
	@media (prefers-reduced-motion: no-preference) {
		.x {
			transition: opacity 0.12s;
		}
	}
</style>
