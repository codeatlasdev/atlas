<script lang="ts">
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { Bot, Layers } from "@lucide/svelte";

	let visible = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let showTimer: ReturnType<typeof setTimeout> | undefined;

	const shortcut = $derived(shortcutFor("atlas::ToggleBotsWorkspace") ?? "⌘⇧B");

	function onHotZoneEnter() {
		clearTimeout(hideTimer);
		showTimer = setTimeout(() => {
			visible = true;
		}, 180);
	}

	function onRailLeave() {
		clearTimeout(showTimer);
		hideTimer = setTimeout(() => {
			visible = false;
		}, 300);
	}

	function onRailEnter() {
		clearTimeout(hideTimer);
	}

	function setMode(mode: "ide" | "bots") {
		ws.appMode = mode;
		visible = false;
	}
</script>

<!-- Hot-zone: thin strip at the very left edge that catches pointer events when rail is hidden -->
<div class="hot-zone" aria-hidden="true" onmouseenter={onHotZoneEnter} onmouseleave={onRailLeave}></div>

<!-- Rail: slides in from left as a fixed overlay, never shifts layout -->
<div
	class="rail"
	class:visible
	role="navigation"
	aria-label="Workspace switcher"
	onmouseenter={onRailEnter}
	onmouseleave={onRailLeave}
>
	<!-- IDE workspace item -->
	<button
		class="item"
		class:active={ws.appMode === "ide"}
		onclick={() => setMode("ide")}
		aria-label="IDE workspace"
		title="IDE  {shortcut}"
	>
		<div class="pill" aria-hidden="true"></div>
		<Layers size={20} />
	</button>

	<!-- Bots workspace item -->
	<button
		class="item"
		class:active={ws.appMode === "bots"}
		onclick={() => setMode("bots")}
		aria-label="Bots workspace"
		title="Bots  {shortcut}"
	>
		<div class="pill" aria-hidden="true"></div>
		<Bot size={20} />
	</button>
</div>

<style>
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

	.rail {
		position: fixed;
		left: 0;
		top: 0;
		width: 56px;
		height: 100%;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
		z-index: 200;
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 8px 0;
		gap: 6px;
		transform: translateX(-100%);
		transition: transform 200ms cubic-bezier(0.4, 0, 0.2, 1);
		pointer-events: none;
	}

	.rail.visible {
		transform: translateX(0);
		pointer-events: all;
	}

	.item {
		position: relative;
		width: 40px;
		height: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: default;
		border-radius: 50%;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			border-radius 200ms ease,
			background 200ms ease,
			color 200ms ease;
	}

	.item:hover {
		border-radius: 12px;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	.item.active {
		border-radius: 12px;
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
	}

	.pill {
		position: absolute;
		left: -8px;
		width: 4px;
		border-radius: 0 2px 2px 0;
		background: var(--ca-text-primary);
		transition: height 200ms ease;
		height: 0;
	}

	.item.active .pill {
		height: 36px;
	}

	.item:hover:not(.active) .pill {
		height: 16px;
	}
</style>
