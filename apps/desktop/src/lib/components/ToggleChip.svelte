<script lang="ts">
	/** Small pressed/unpressed text toggle ("Aa", ".*") for find bars. */
	import type { Snippet } from "svelte";

	let {
		on,
		label,
		title = label,
		onclick,
		children,
	}: { on: boolean; label: string; title?: string; onclick: () => void; children: Snippet } = $props();
</script>

<button class="toggle" class:on {title} aria-label={label} aria-pressed={on} {onclick}>
	{@render children()}
</button>

<style>
	.toggle {
		font: inherit;
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary, var(--ca-text-primary));
		cursor: pointer;
	}
	.toggle.on {
		background: var(--ca-surface-raised, var(--ca-bg-default));
		color: var(--ca-text-primary);
		border-color: var(--ca-text-secondary, var(--ca-text-primary));
	}
</style>
