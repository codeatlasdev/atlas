<script lang="ts">
	let { data = [] }: { data: number[] } = $props();

	const MAX_BARS = 14;
	const bars = $derived(
		(() => {
			const d = data.slice(-MAX_BARS);
			const max = Math.max(...d, 1);
			return d.map((v) => ({ value: v, pct: Math.round((v / max) * 100) }));
		})(),
	);
</script>

<div class="sparkline" role="img" aria-label="Token usage over 14 days">
	{#each bars as bar, i (i)}
		<div class="spark-bar" style:height="{bar.pct}%" title="{bar.value.toLocaleString()} tokens"></div>
	{/each}
</div>

<style>
	.sparkline {
		display: flex;
		align-items: flex-end;
		gap: 2px;
		height: 32px;
		padding: 0 2px;
	}
	.spark-bar {
		flex: 1;
		min-height: 2px;
		background: var(--ca-brand-main);
		border-radius: 1px;
		opacity: 0.7;
		transition:
			height 300ms ease,
			opacity 150ms;
	}
	.spark-bar:hover {
		opacity: 1;
	}
</style>
