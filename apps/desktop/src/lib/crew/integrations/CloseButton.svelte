<script lang="ts">
	/** CloseButton — the round X in the corner of the integrations dialog (catalog and detail views). */
	import { X } from "@lucide/svelte";

	let { onclose }: { onclose: () => void } = $props();
</script>

<button class="close-btn" onclick={onclose} aria-label="Close integrations">
	<X size={16} />
</button>

<style>
	.close-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: none;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-secondary);
		cursor: default;
		flex: none;
		padding: 0;
		transition: background 120ms;
	}
	.close-btn:hover {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-primary);
	}
</style>
