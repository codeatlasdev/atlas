<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		HANDLES,
		HUD_MODES,
		initialState,
		labelPlacement,
		loupeOrigin,
		positionLabel,
		reduce,
		selectionAria,
		sizeLabel,
		type Handle,
		type OverlayEffect,
		type OverlayEvent,
		type OverlayState,
	} from "#lib/shots/overlay.ts";
	import type { LastMode, Mode, OverlayInit } from "#lib/shots/types.ts";

	/** The loupe shows a 15x15 px block drawn 8x larger. */
	const LOUPE_GRID = 15;
	const LOUPE_ZOOM = 8;
	const LOUPE = LOUPE_GRID * LOUPE_ZOOM;
	/** The hex colour line under the loupe. */
	const LOUPE_HEX = 22;
	const LABEL = { w: 150, h: 24 };
	/** The toolbar's buttons, in `HUD_MODES` order (keys 1..6). */
	const MODE_LABELS: Record<LastMode, string> = {
		region: "Region",
		window: "Window",
		screen: "Screen",
		record: "Record",
		gif: "GIF",
		text: "Text (OCR)",
	};

	let st = $state<OverlayState>(initialState());
	// The page is exactly the display's size; until init answers, the window's own size stands in for it.
	let vw = $state(typeof window === "undefined" ? 0 : window.innerWidth);
	let vh = $state(typeof window === "undefined" ? 0 : window.innerHeight);
	let loupe = $state<string | null>(null);
	let centre = $state<string | null>(null);
	let failure = $state<string | null>(null);

	let bounds = $derived({ x: 0, y: 0, w: st.display?.frame.w ?? vw, h: st.display?.frame.h ?? vh });
	// The held selection wins over the live drag rectangle (they are never both set).
	let rect = $derived(st.held ?? st.rect);
	// Window mode shows the hovered window's frame as the selection.
	let sel = $derived(st.pick === "window" ? (st.hover?.frame ?? null) : rect);
	let scale = $derived(st.display?.scale ?? 1);
	let label = $derived(sel && st.display ? labelPlacement(sel, LABEL, bounds) : null);
	let pill = $derived(
		st.pick === "window" && st.hover
			? `${st.hover.title || "Window"}`
			: rect && st.display
				? `${sizeLabel(rect, scale)} · ${positionLabel(rect, scale)}`
				: null,
	);
	let lp = $derived(st.pointer ? loupeOrigin(st.pointer, LOUPE + LOUPE_HEX, bounds) : null);
	let cross = $derived(st.pointer !== null && st.drag === null && st.edit === null && st.pick === "region");
	// Handles show on a held selection only.
	let handles = $derived(st.held && st.pick === "region" ? HANDLES : []);

	function dispatch(ev: OverlayEvent) {
		const r = reduce(st, ev);
		st = r.state;
		for (const e of r.effects) void run(e);
	}

	async function run(e: OverlayEffect) {
		try {
			if (e.type === "commit") await invoke("shots_overlay_commit", { commit: e.payload });
			else if (e.type === "previous") await invoke("shots_overlay_previous");
			else await invoke("shots_overlay_cancel");
		} catch (err) {
			failure = errText(err);
		}
	}

	// The timer is a saved setting: Rust advances it and tells every display's overlay (`shots://hud-timer`), so a
	// click on one screen shows on all of them.
	async function cycleTimer() {
		try {
			await invoke<number>("shots_timer_cycle");
		} catch (err) {
			failure = errText(err);
		}
	}

	// The loupe: one request in flight at most, always for the latest pointer (rAF-throttled). A rejection or a null
	// answer (no frozen frame yet, non-macOS stub) hides the loupe silently.
	let frame = 0;
	let asking = false;
	let want: { x: number; y: number } | null = null;
	// Read through a function: TS would narrow `want` to `null` across the await, but `scheduleLoupe` refills it.
	const pending = () => want;

	function scheduleLoupe(x: number, y: number) {
		want = { x, y };
		if (frame) return;
		frame = requestAnimationFrame(() => {
			frame = 0;
			void fetchLoupe();
		});
	}

	async function fetchLoupe() {
		if (asking || !want) return;
		const at = want;
		want = null;
		asking = true;
		try {
			// global points, like every other command
			const f = st.display?.frame;
			// the block and the colour of its middle pixel, both from Rust: the page decodes nothing per move
			const l = await invoke<{ url: string; hex: string } | null>("shots_overlay_loupe", {
				x: at.x + (f?.x ?? 0),
				y: at.y + (f?.y ?? 0),
			});
			loupe = l?.url ?? null;
			centre = l?.hex ?? null;
		} catch {
			loupe = null;
			centre = null;
		} finally {
			asking = false;
			const next = pending();
			if (next) scheduleLoupe(next.x, next.y);
		}
	}

	// The toolbar handles its own pointer events; everything else is the selection surface.
	const onToolbar = (e: Event) => (e.target as Element | null)?.closest?.(".hud") != null;

	function onMove(e: PointerEvent) {
		dispatch({ type: "move", x: e.clientX, y: e.clientY, shift: e.shiftKey, alt: e.altKey });
		scheduleLoupe(e.clientX, e.clientY);
	}

	function onDown(e: PointerEvent) {
		if (onToolbar(e) && e.button === 0) return;
		e.preventDefault();
		dispatch({
			type: "down",
			x: e.clientX,
			y: e.clientY,
			button: e.button,
			shift: e.shiftKey,
			alt: e.altKey,
			clicks: e.detail,
		});
	}

	function onUp(e: PointerEvent) {
		if (onToolbar(e)) return;
		if (e.button === 0) dispatch({ type: "up", x: e.clientX, y: e.clientY });
	}

	// The keys the reducer owns: Esc, Space, Return, Tab and 1..6 (HUD), the arrows (a held selection).
	const KEYS = new Set(["Escape", " ", "Enter", "Tab", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"]);

	function onKey(e: KeyboardEvent, up: boolean) {
		const digit = st.hud !== null && e.key.length === 1 && e.key >= "1" && e.key <= String(HUD_MODES.length);
		if (!KEYS.has(e.key) && !digit) return;
		// a focused toolbar button keeps Return/Space for itself (its own click)
		if ((e.key === "Enter" || e.key === " ") && (e.target as Element | null)?.closest?.(".hud button"))
			return;
		e.preventDefault();
		// Space is a toggle; the arrows repeat on purpose (hold to nudge)
		if (e.repeat && !e.key.startsWith("Arrow")) return;
		dispatch({ type: "key", key: e.key, up, shift: e.shiftKey, alt: e.altKey });
	}

	/** The resize cursor of each handle. */
	const CURSORS: Record<Handle, string> = {
		nw: "nwse-resize",
		se: "nwse-resize",
		ne: "nesw-resize",
		sw: "nesw-resize",
		n: "ns-resize",
		s: "ns-resize",
		e: "ew-resize",
		w: "ew-resize",
	};

	/** Where a handle sits on the held selection: `[left, top]` as a percentage of it. */
	const HANDLE_AT: Record<Handle, [number, number]> = {
		nw: [0, 0],
		n: [50, 0],
		ne: [100, 0],
		e: [100, 50],
		se: [100, 100],
		s: [50, 100],
		sw: [0, 100],
		w: [0, 50],
	};

	onMount(() => {
		const offs: Array<() => void> = [];
		let alive = true;
		const onResize = () => {
			vw = window.innerWidth;
			vh = window.innerHeight;
		};
		window.addEventListener("resize", onResize);
		// The window is built once and reused, so a session reaches it by event (`shots://overlay-init`, twice: at once,
		// then with the windows a pick can snap to). A window that is only now loading missed those and asks; `null`
		// means no session is open yet.
		const begin = (init: OverlayInit) => {
			dispatch({ type: "init", init });
			if (st.pointer) scheduleLoupe(st.pointer.x, st.pointer.y);
		};
		invoke<OverlayInit | null>("shots_overlay_init")
			.then((init) => {
				if (alive && init) begin(init);
			})
			.catch((err) => (failure = errText(err)));
		void listen<OverlayInit>("shots://overlay-init", (ev) => begin(ev.payload)).then((off) =>
			alive ? offs.push(off) : off(),
		);
		// The window was hidden (a commit, a cancel): nothing of this session may show when it is next ordered in.
		void listen("shots://overlay-reset", () => {
			loupe = null;
			centre = null;
			failure = null;
			dispatch({ type: "reset" });
		}).then((off) => (alive ? offs.push(off) : off()));
		// Space toggles the mode inside the page; Rust's event keeps sibling displays' pages in step. The HUD picks
		// its modes on the toolbar instead.
		void listen<Mode>("shots://overlay-mode", (ev) => {
			const want = ev.payload === "window" ? "window" : "region";
			if (st.pick !== want && !st.drag && !st.hud) dispatch({ type: "key", key: " " });
		}).then((off) => (alive ? offs.push(off) : off()));
		void listen<number>("shots://hud-timer", (ev) => dispatch({ type: "timer", secs: ev.payload })).then(
			(off) => (alive ? offs.push(off) : off()),
		);
		return () => {
			alive = false;
			window.removeEventListener("resize", onResize);
			for (const off of offs) off();
			if (frame) cancelAnimationFrame(frame);
		};
	});
</script>

<svelte:window
	onpointermove={onMove}
	onpointerdown={onDown}
	onpointerup={onUp}
	onkeydown={(e) => onKey(e, false)}
	onkeyup={(e) => onKey(e, true)}
	oncontextmenu={(e) => e.preventDefault()}
/>

<!-- The mask is rendered before init answers: the first frame is the dim and nothing else. -->
<div class="root">
	{#if sel}
		<div class="dim" style:left="0" style:top="0" style:width="100%" style:height="{sel.y}px"></div>
		<div class="dim" style:left="0" style:top="{sel.y + sel.h}px" style:width="100%" style:bottom="0"></div>
		<div
			class="dim"
			style:left="0"
			style:top="{sel.y}px"
			style:width="{sel.x}px"
			style:height="{sel.h}px"
		></div>
		<div
			class="dim"
			style:left="{sel.x + sel.w}px"
			style:top="{sel.y}px"
			style:right="0"
			style:height="{sel.h}px"
		></div>
		<div
			class="frame"
			class:win={st.pick === "window"}
			class:held={st.held !== null && st.pick === "region"}
			role={st.held ? "group" : undefined}
			aria-label={st.held ? selectionAria(st.held, scale) : undefined}
			style:left="{sel.x}px"
			style:top="{sel.y}px"
			style:width="{sel.w}px"
			style:height="{sel.h}px"
		>
			{#each handles as h (h)}
				<div
					class="handle"
					data-handle={h}
					style:left="{HANDLE_AT[h][0]}%"
					style:top="{HANDLE_AT[h][1]}%"
					style:cursor={CURSORS[h]}
				></div>
			{/each}
		</div>
	{:else}
		<div class="dim" style:inset="0"></div>
	{/if}

	{#if cross && st.pointer}
		<div class="cross h" style:top="{st.pointer.y}px"></div>
		<div class="cross v" style:left="{st.pointer.x}px"></div>
	{/if}

	{#if sel && label && pill}
		<div
			class="pill"
			class:win={st.pick === "window"}
			style:left="{label.x}px"
			style:top="{label.y}px"
			style:min-width="{LABEL.w}px"
		>
			{pill}
		</div>
	{/if}

	{#if loupe && lp}
		<div class="loupe" style:left="{lp.x}px" style:top="{lp.y}px" style:width="{LOUPE}px">
			<div class="lens" style:height="{LOUPE}px">
				<img src={loupe} alt="" width={LOUPE} height={LOUPE} draggable="false" />
				<!-- a 1px line between source pixels -->
				<div class="grid" style:--cell="{LOUPE_ZOOM}px"></div>
				<!-- the centre pixel -->
				<div class="reticle" style:width="{LOUPE_ZOOM}px" style:height="{LOUPE_ZOOM}px"></div>
			</div>
			{#if centre}
				<div class="hex" style:height="{LOUPE_HEX}px">
					<span class="swatch" style:background={centre}></span>
					{centre}
				</div>
			{/if}
		</div>
	{/if}

	{#if st.hud}
		<div class="hud" role="toolbar" aria-label="Capture mode">
			{#each HUD_MODES as m, i (m)}
				<button
					type="button"
					class:on={st.hud.mode === m}
					aria-pressed={st.hud.mode === m}
					aria-label={MODE_LABELS[m]}
					title="{MODE_LABELS[m]} ({i + 1})"
					onclick={() => dispatch({ type: "mode", mode: m })}
				>
					{MODE_LABELS[m]}
				</button>
			{/each}
			<span class="sep" aria-hidden="true"></span>
			<button
				type="button"
				class="timer"
				title="Self-timer: click to change"
				aria-label="Timer {st.hud.timerSecs > 0 ? `${st.hud.timerSecs} seconds` : 'off'}. Click to change"
				onclick={cycleTimer}
			>
				Timer: {st.hud.timerSecs > 0 ? `${st.hud.timerSecs}s` : "Off"}
			</button>
			<span class="sep" aria-hidden="true"></span>
			<button
				type="button"
				aria-label="Previous area"
				title="Capture the previous area"
				disabled={!st.hud.hasPrevious}
				onclick={() => dispatch({ type: "previous" })}
			>
				Previous area
			</button>
		</div>
	{/if}

	{#if failure}
		<div class="fail" role="alert">{failure}</div>
	{/if}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		margin: 0;
		overflow: hidden;
	}
	.root {
		position: fixed;
		inset: 0;
		cursor: crosshair;
		user-select: none;
		-webkit-user-select: none;
		overflow: hidden;
	}
	.dim {
		position: absolute;
		background: rgb(0 0 0 / 0.38);
		pointer-events: none;
	}
	/* white 1px edge with a dark hairline outside it so the frame reads on a white page too */
	.frame {
		position: absolute;
		box-sizing: border-box;
		border: 1px solid #fff;
		box-shadow: 0 0 0 1px rgb(0 0 0 / 0.45);
		pointer-events: none;
	}
	/* a held selection can be moved: the cursor says so inside it */
	.frame.held {
		cursor: move;
		pointer-events: auto;
	}
	.frame.win {
		border: 2px solid var(--ca-accent, #4f8cff);
		background: color-mix(in srgb, var(--ca-accent, #4f8cff) 22%, transparent);
		box-shadow: none;
	}
	.handle {
		position: absolute;
		width: 9px;
		height: 9px;
		box-sizing: border-box;
		transform: translate(-50%, -50%);
		background: #fff;
		border: 1px solid rgb(0 0 0 / 0.65);
		border-radius: 2px;
	}
	.cross {
		position: absolute;
		background: rgb(255 255 255 / 0.55);
		box-shadow: 0 0 0 0.5px rgb(0 0 0 / 0.35);
		pointer-events: none;
	}
	.cross.h {
		left: 0;
		right: 0;
		height: 1px;
	}
	.cross.v {
		top: 0;
		bottom: 0;
		width: 1px;
	}
	.pill {
		position: absolute;
		box-sizing: border-box;
		height: 24px;
		padding: 0 10px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 12px;
		background: color-mix(in oklab, var(--ca-surface-raised) 92%, transparent);
		color: var(--ca-text-primary);
		font:
			600 12px/1 ui-monospace,
			SFMono-Regular,
			Menlo,
			monospace;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		max-width: 320px;
		overflow: hidden;
		text-overflow: ellipsis;
		pointer-events: none;
	}
	.pill.win {
		font-family: inherit;
		font-weight: 500;
	}
	.loupe {
		position: absolute;
		box-sizing: border-box;
		overflow: hidden;
		border-radius: 8px;
		border: 2px solid #fff;
		box-shadow:
			0 0 0 1px rgb(0 0 0 / 0.5),
			0 6px 20px rgb(0 0 0 / 0.4);
		background: #000;
		pointer-events: none;
	}
	.lens {
		position: relative;
	}
	.loupe img {
		display: block;
		width: 100%;
		height: 100%;
		image-rendering: pixelated;
	}
	/* the 1px line between source pixels: two repeating gradients, one per axis */
	.grid {
		position: absolute;
		inset: 0;
		background-image:
			linear-gradient(to right, rgb(0 0 0 / 0.28) 1px, transparent 1px),
			linear-gradient(to bottom, rgb(0 0 0 / 0.28) 1px, transparent 1px);
		background-size: var(--cell) var(--cell);
	}
	.reticle {
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		box-sizing: border-box;
		border: 1px solid #fff;
		outline: 1px solid rgb(0 0 0 / 0.85);
		pointer-events: none;
	}
	.hex {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		background: rgb(0 0 0 / 0.78);
		--shot-ink: rgb(255 255 255);
		color: var(--shot-ink);
		font:
			600 12px/1 ui-monospace,
			SFMono-Regular,
			Menlo,
			monospace;
		text-transform: uppercase;
	}
	.swatch {
		width: 10px;
		height: 10px;
		border-radius: 2px;
		border: 1px solid rgb(255 255 255 / 0.7);
	}
	.hud {
		position: absolute;
		top: 28px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px;
		border-radius: 14px;
		background: color-mix(in oklab, var(--ca-surface-raised) 94%, transparent);
		box-shadow:
			0 0 0 1px rgb(0 0 0 / 0.35),
			0 10px 30px rgb(0 0 0 / 0.4);
		color: var(--ca-text-primary);
		font:
			500 13px/1 system-ui,
			sans-serif;
		cursor: default;
	}
	.hud button {
		all: unset;
		box-sizing: border-box;
		padding: 8px 12px;
		border-radius: 9px;
		cursor: pointer;
		white-space: nowrap;
	}
	.hud button:hover:not(:disabled) {
		background: color-mix(in srgb, var(--ca-text-primary) 12%, transparent);
	}
	.hud button.on {
		background: var(--ca-accent, #4f8cff);
		color: var(--ca-brand-contrast);
	}
	.hud button:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.hud button:focus-visible {
		outline: 2px solid var(--ca-accent, #4f8cff);
		outline-offset: 1px;
	}
	.sep {
		width: 1px;
		align-self: stretch;
		margin: 4px 4px;
		background: color-mix(in srgb, var(--ca-text-primary) 22%, transparent);
	}
	.timer {
		padding: 0 6px;
		opacity: 0.8;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.fail {
		position: absolute;
		left: 50%;
		bottom: 24px;
		transform: translateX(-50%);
		max-width: 60%;
		padding: 8px 14px;
		border-radius: 10px;
		background: var(--ca-semantic-danger, #ff453a);
		color: var(--ca-brand-contrast);
		font:
			500 13px/1.3 system-ui,
			sans-serif;
		pointer-events: none;
	}
</style>
