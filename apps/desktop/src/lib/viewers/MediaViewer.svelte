<script lang="ts">
	import { formatBytes, extOf } from "#lib/viewers/kind.ts";

	let { url, path, size, video }: { url: string; path: string; size: number; video: boolean } = $props();
	let name = $derived(path.split("/").pop() ?? path);
</script>

<div class="media">
	{#if video}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video src={url} controls preload="metadata" class="v"></video>
	{:else}
		<div class="card">
			<div class="title">{name}</div>
			<div class="meta">{extOf(path).toUpperCase()} · {formatBytes(size)}</div>
			<audio src={url} controls preload="metadata"></audio>
		</div>
	{/if}
</div>

<style>
	.media {
		flex: 1;
		min-height: 0;
		display: grid;
		place-items: center;
		padding: 16px;
		overflow: hidden;
	}
	.v {
		max-width: 100%;
		max-height: 100%;
		border-radius: 10px;
		background: black;
		box-shadow: 0 12px 40px color-mix(in oklab, black 30%, transparent);
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: min(440px, 100%);
		padding: 20px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 16px;
		background: color-mix(in oklab, var(--ca-text-primary) 4%, transparent);
	}
	.title {
		font-size: 14px;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		margin-bottom: 8px;
	}
	audio {
		width: 100%;
	}
</style>
