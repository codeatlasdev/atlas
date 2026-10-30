<script lang="ts">
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { onMount } from "svelte";

	// Windows draws no title bar (`decorations: false` in tauri.windows.conf.json): the page carries the caption
	// buttons in the title row, like Windows 11's own apps. Drag and double-click-to-maximize come from the
	// `data-tauri-drag-region` rows; the window keeps its native resize borders, shadow and rounded corners.
	const win = getCurrentWindow();
	let maximized = $state(false);

	onMount(() => {
		const sync = () => void win.isMaximized().then((m) => (maximized = m));
		sync();
		const off = win.onResized(sync);
		return () => void off.then((f) => f());
	});
</script>

<div class="controls" role="group" aria-label="Window">
	<button aria-label="Minimize" title="Minimize" onclick={() => void win.minimize()}>
		<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
	</button>
	<button
		aria-label={maximized ? "Restore" : "Maximize"}
		title={maximized ? "Restore" : "Maximize"}
		onclick={() => void win.toggleMaximize()}
	>
		{#if maximized}
			<svg viewBox="0 0 10 10" aria-hidden="true">
				<path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
				<rect x="0.5" y="2.5" width="7" height="7" rx="1" />
			</svg>
		{:else}
			<svg viewBox="0 0 10 10" aria-hidden="true"><rect x="0.5" y="0.5" width="9" height="9" rx="1" /></svg>
		{/if}
	</button>
	<button class="close" aria-label="Close" title="Close" onclick={() => void win.close()}>
		<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" /></svg>
	</button>
</div>

<style>
	/* Spans the window pad and the title row (app.css): the buttons reach the top and right edges, so a flick of the
	   pointer into the corner hits Close (Fitts), as in every Windows app. Width: --caption-controls. */
	.controls {
		position: fixed;
		top: 0;
		right: 0;
		z-index: 2000;
		display: flex;
		height: calc(var(--window-pad) + var(--title-row));
		width: var(--caption-controls);
	}
	button {
		flex: 1;
		display: grid;
		place-items: center;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--ca-text-secondary, currentColor);
		transition:
			background-color 80ms linear,
			color 80ms linear;
	}
	button:hover {
		background: color-mix(in oklab, currentColor 10%, transparent);
		color: var(--ca-text-primary, currentColor);
	}
	button:active {
		background: color-mix(in oklab, currentColor 6%, transparent);
	}
	/* Windows' red Close: the theme's error colour, text in the on-colour the kit pairs with filled surfaces. */
	.close:hover {
		background: var(--ca-semantic-error);
		color: var(--ca-brand-contrast);
	}
	.close:active {
		background: color-mix(in oklab, var(--ca-semantic-error) 80%, transparent);
		color: var(--ca-brand-contrast);
	}
	svg {
		width: 10px;
		height: 10px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1;
		shape-rendering: geometricPrecision;
	}
</style>
