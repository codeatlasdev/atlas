<script lang="ts">
	/** IntegrationCard — a titled bordered card of the integration detail page (accounts, skills). */
	import type { Snippet } from "svelte";

	let { title, count, children }: { title: string; count?: number; children: Snippet } = $props();
</script>

<div class="detail-card">
	<h3 class="card-title">
		{title}
		{#if count !== undefined}<span class="card-count">{count}</span>{/if}
	</h3>
	{@render children()}
</div>

<style>
	.detail-card {
		margin: 14px 20px 0;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		overflow: hidden;
	}

	.card-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		margin: 0;
		padding: 10px 14px;
		display: flex;
		align-items: center;
		gap: 6px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-outline-subtle) 30%, transparent);
	}

	.card-count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 18px;
		height: 18px;
		padding: 0 5px;
		border-radius: 999px;
		background: var(--ca-vibrancy-fill);
		font-size: 10.5px;
		font-weight: 700;
		color: var(--ca-text-secondary);
	}
</style>
