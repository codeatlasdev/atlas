<script lang="ts">
	import { updates } from "#lib/stores/updates.svelte.ts";

	let version = $derived(updates.info?.app.version ?? "");
	let label = $derived(`Dev build ${version} — updates paused (atlas unlink to resume)`);
</script>

{#if updates.dev}
	<!-- A 2px amber line across the very top: never affects layout, never takes clicks. -->
	<div class="line" aria-hidden="true"></div>
	<!-- A thin hover target on the top edge carries the tooltip and the accessible name. -->
	<button class="edge" aria-label={label} title={label} onclick={() => updates.show("updates")}></button>
{/if}

<style>
	.line {
		position: fixed;
		inset: 0 0 auto 0;
		height: 2px;
		z-index: 2000;
		pointer-events: none;
		background: linear-gradient(
			90deg,
			var(--ca-semantic-warning),
			color-mix(in oklab, var(--ca-semantic-warning) 70%, white) 50%,
			var(--ca-semantic-warning)
		);
		box-shadow: 0 0 8px color-mix(in oklab, var(--ca-semantic-warning) 55%, transparent);
	}
	.edge {
		position: fixed;
		top: 0;
		left: 50%;
		translate: -50% 0;
		width: 160px;
		height: 6px;
		z-index: 2001;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: default;
	}
</style>
