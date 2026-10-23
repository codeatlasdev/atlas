<script lang="ts">
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { SHOW_DELAY_MS, HIDE_DELAY_MS } from "./workspaceRail.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { Layers } from "@lucide/svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";

	// visible = rail is expanded (72px); false = collapsed (0px)
	let visible = $state(false);
	// pinned = keep expanded while tooltip/menu is open
	let pinned = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let showTimer: ReturnType<typeof setTimeout> | undefined;

	const shortcut = $derived(shortcutFor("atlas::ToggleBotsWorkspace") ?? "Cmd+Shift+B");
	const needsYouCount = $derived(bots.needsYouCount);

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

<!-- Rail: real in-flow flex column, width 0 → 72px -->
<div
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

	/* Rail: real in-flow flex column — takes space and pushes sidebar+content right */
	.rail {
		/* Layout: in-flow, NOT position:fixed */
		flex: none;
		width: 0;
		overflow: hidden;
		height: 100%;
		/* Animate width: 160ms ease-out for expand and collapse */
		transition: width 160ms ease-out;

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
		width: 72px;
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
