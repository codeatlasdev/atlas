<script lang="ts">
	let { title, description }: { title: string; description?: string } = $props();
</script>

<div class="phead">
	<h2>{title}</h2>
	{#if description}<p>{description}</p>{/if}
</div>

<style>
	.phead h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	.phead p {
		margin: 3px 0 0;
		font-size: 12.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
</style>
