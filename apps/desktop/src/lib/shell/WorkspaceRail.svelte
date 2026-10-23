<script lang="ts">
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { SHOW_DELAY_MS, HIDE_DELAY_MS } from "./workspaceRail.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { Layers } from "@lucide/svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// visible = rail is shown; everything else in the window slides right by RAIL_W to make room
	let visible = $state(false);
	// pinned = keep expanded while tooltip/menu is open
	let pinned = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let showTimer: ReturnType<typeof setTimeout> | undefined;
	let railEl: HTMLElement | undefined;

	const RAIL_W = 72;
	const SLIDE_MS = 180;

	const shortcut = $derived(shortcutFor("atlas::ToggleBotsWorkspace") ?? "Cmd+Shift+B");
	const needsYouCount = $derived(bots.needsYouCount);

	// "Push everything aside" without reflowing: animating a width relays out the sidebar, the page and every
	// terminal on each frame, which stuttered. The siblings slide by `translate` on the compositor instead.
	// Native terminals can't follow a CSS transform, and parking them for the slide blinked: they are told
	// (`atlas:rail-slide`) and play the same slide on their own layer (`term_slide`). Other native views
	// (browser tabs) re-sync once at the end (a `resize` makes every `watchSlot` measure again).
	let running: Animation[] = [];
	let first = true;
	$effect(() => {
		const on = visible;
		const app = railEl?.parentElement;
		if (!app) return;
		if (first) {
			first = false;
			return;
		}
		const kids = [...app.children].filter(
			(k): k is HTMLElement => k instanceof HTMLElement && k !== railEl && !k.classList.contains("hot-zone"),
		);
		const from = kids.map((k) => getComputedStyle(k).translate);
		const ms = reducedMotion ? 0 : SLIDE_MS;
		for (const a of running) a.cancel();
		const to = on ? `${RAIL_W}px 0` : "0px 0";
		window.dispatchEvent(new CustomEvent("atlas:rail-slide", { detail: { dx: on ? RAIL_W : -RAIL_W, ms } }));
		running = kids.map((k, i) =>
			k.animate([{ translate: from[i] === "none" ? "0px 0" : from[i] }, { translate: to }], {
				duration: ms,
				easing: "ease-out",
				fill: "forwards",
			}),
		);
		const mine = running;
		void Promise.all(mine.map((a) => a.finished))
			.then(() => {
				if (running !== mine) return;
				if (!on) for (const a of mine) a.cancel();
				window.dispatchEvent(new Event("resize"));
			})
			.catch(() => {});
	});

	function onHotZoneEnter() {
		clearTimeout(hideTimer);
		showTimer = setTimeout(() => {
			visible = true;
		}, SHOW_DELAY_MS);
	}

	function onRailLeave() {
		clearTimeout(showTimer);
		if (pinned) return;
		hideTimer = setTimeout(() => {
			visible = false;
		}, HIDE_DELAY_MS);
	}

	function onRailEnter() {
		clearTimeout(hideTimer);
	}

	function setMode(mode: "ide" | "bots") {
		ws.appMode = mode;
		visible = false;
		pinned = false;
	}
</script>

<!-- Hot-zone: thin fixed strip at left edge, only active when rail is hidden -->
<div
	class="hot-zone"
	class:inactive={visible}
	aria-hidden="true"
	onmouseenter={onHotZoneEnter}
	onmouseleave={onRailLeave}
></div>

<!-- Rail: a fixed column that slides in; the rest of the window slides aside by the same width -->
<div
	bind:this={railEl}
	class="rail"
	class:visible
	role="navigation"
	aria-label="Workspace switcher"
	onmouseenter={onRailEnter}
	onmouseleave={onRailLeave}
>
	<!-- IDE workspace -->
	<button
		class="item"
		class:active={ws.appMode === "ide"}
		onclick={() => setMode("ide")}
		aria-label="IDE workspace"
		title="IDE  {shortcut}"
	>
		<div class="pill" aria-hidden="true"></div>
		<Layers size={22} />
	</button>

	<div class="separator" aria-hidden="true"></div>

	<!-- Mugiwaras workspace -->
	<button
		class="item"
		class:active={ws.appMode === "bots"}
		onclick={() => setMode("bots")}
		aria-label="Mugiwaras workspace"
		title="Mugiwaras  {shortcut}"
	>
		<div class="pill" aria-hidden="true"></div>
		<ShipFlag size={22} />
		{#if needsYouCount > 0}
			<span class="needs-badge" aria-label="{needsYouCount} crewmates need your attention">
				{needsYouCount > 9 ? "9+" : needsYouCount}
			</span>
		{/if}
	</button>
</div>

<style>
	/* Hot-zone: fixed thin strip at window left edge, triggers rail expand */
	.hot-zone {
		position: fixed;
		left: 0;
		top: 0;
		width: 6px;
		height: 100%;
		z-index: 199;
		pointer-events: all;
		cursor: default;
	}

	/* When rail is expanded, hot-zone becomes inactive so it doesn't steal events */
	.hot-zone.inactive {
		pointer-events: none;
	}

	/* Rail: a fixed column under everything else's slide; it moves by transform only (no layout per frame). */
	.rail {
		position: fixed;
		left: 0;
		top: 0;
		bottom: 0;
		width: 72px;
		z-index: 1;
		overflow: hidden;
		transform: translateX(-100%);
		/* ease-out = CoreAnimation's easeOut, which the native terminals use for the same slide (term_slide). */
		transition: transform 180ms ease-out;

		background: color-mix(in oklab, var(--ca-surface-sunken) 100%, black 4%);
		border-right: 0.5px solid var(--ca-outline-subtle);

		display: flex;
		flex-direction: column;
		align-items: center;
		/* Below the macOS traffic lights, like the sidebar's 52 px drag band. */
		padding: 52px 0 8px;
		gap: 8px;
	}

	.rail.visible {
		transform: none;
	}
	@media (prefers-reduced-motion: reduce) {
		.rail {
			transition: none;
		}
	}
	/* Windows and Linux: a native title bar above, no traffic lights to clear. */
	:global(html.opaque-window) .rail {
		padding-top: 8px;
	}

	/* Each workspace tile: 48x48, circle → rounded-square on hover/active */
	.item {
		position: relative;
		width: 48px;
		height: 48px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: default;
		border-radius: 50%;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			border-radius 150ms ease,
			background 150ms ease,
			color 150ms ease;
	}

	.item:hover {
		border-radius: 16px;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	.item.active {
		border-radius: 16px;
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
	}

	/* Discord pill: on the rail's left edge */
	.pill {
		position: absolute;
		left: 0;
		top: 50%;
		transform: translateY(-50%) translateX(-50%);
		width: 4px;
		border-radius: 0 2px 2px 0;
		background: var(--ca-text-primary);
		transition: height 150ms ease;
		height: 0;
		/* The pill left half is clipped by rail overflow:hidden — appears as a bar on the left edge */
	}

	.item.active .pill {
		height: 40px;
	}

	.item:hover:not(.active) .pill {
		height: 20px;
	}

	/* Separator line between IDE and Bots (32px wide, 2px tall, centered) */
	.separator {
		width: 32px;
		height: 2px;
		border-radius: 1px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		flex: none;
		margin: 2px 0;
	}

	/* Needs-you badge: red circle at bottom-right of tile */
	.needs-badge {
		position: absolute;
		bottom: 2px;
		right: 2px;
		min-width: 16px;
		height: 16px;
		border-radius: 8px;
		background: var(--ca-semantic-error, #e03131);
		/* Dark ring cut in the rail color */
		box-shadow: 0 0 0 2px color-mix(in oklab, var(--ca-surface-sunken) 100%, black 4%);
		color: var(--ca-surface-base);
		font-size: 9px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 3px;
		pointer-events: none;
	}
</style>
