<script lang="ts">
	// Keystroke pill (label `shots-keys`, 420x64, click-through). It is part of the recorded picture (Rust does NOT
	// exclude this window from the capture): it only draws the labels Rust sends in `shots://keys` (already
	// formatted, `⌘⇧4`, and already faded after 1.5 s). It reads no key itself and stores nothing.
	import { listen } from "@tauri-apps/api/event";
	import { onMount } from "svelte";

	let keys = $state<string[]>([]);

	onMount(() => {
		let off: (() => void) | undefined;
		let gone = false;
		void listen<string[]>("shots://keys", (e) => (keys = Array.isArray(e.payload) ? e.payload : [])).then(
			(f) => (gone ? f() : (off = f)),
		);
		return () => {
			gone = true;
			off?.();
		};
	});
</script>

<svelte:head><title>Atlas Keystrokes</title></svelte:head>

<div class="wrap" aria-hidden="true">
	{#if keys.length > 0}
		<div class="pill">
			{#each keys as k, i (i)}<kbd>{k}</kbd>{/each}
		</div>
	{/if}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
	}
	.wrap {
		display: grid;
		place-items: center;
		width: 100vw;
		height: 100vh;
		pointer-events: none;
		user-select: none;
	}
	/* Dark pill with light text whatever the theme: it sits over arbitrary recorded content. */
	.pill {
		display: flex;
		align-items: center;
		gap: 8px;
		max-width: 100%;
		padding: 10px 16px;
		border-radius: 14px;
		color: var(--shot-ink);
		--shot-ink: rgb(255 255 255);
		background: rgb(20 20 22 / 0.82);
		box-shadow: 0 4px 18px rgb(0 0 0 / 0.35);
		font:
			600 20px/1 -apple-system,
			BlinkMacSystemFont,
			sans-serif;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	kbd {
		font: inherit;
	}
</style>
