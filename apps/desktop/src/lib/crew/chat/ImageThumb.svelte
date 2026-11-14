<script lang="ts">
	/**
	 * ImageThumb — one image from its bytes, as a square thumbnail. The blob URL lives exactly as long as the
	 * thumbnail: made when the bytes arrive, revoked when they change or the thumbnail goes away.
	 */
	interface Props {
		bytes: Uint8Array;
		mime: string;
		alt: string;
		/** Side of the square, in px. */
		size: number;
		onclick?: () => void;
	}

	let { bytes, mime, alt, size, onclick }: Props = $props();

	const src = $derived(URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime })));

	$effect(() => {
		const url = src;
		return () => URL.revokeObjectURL(url);
	});
</script>

{#if onclick}
	<button class="thumb" style:width="{size}px" style:height="{size}px" aria-label={`Open ${alt}`} {onclick}>
		<img {src} {alt} />
	</button>
{:else}
	<span class="thumb" style:width="{size}px" style:height="{size}px"><img {src} {alt} /></span>
{/if}

<style>
	.thumb {
		display: block;
		overflow: hidden;
		border-radius: 10px;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-surface));
		padding: 0;
		cursor: default;
		flex: none;
	}
	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
</style>
