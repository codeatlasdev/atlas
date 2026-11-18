<script lang="ts">
	/** Preview of an `ImportResult` before it is saved: the same miniature a picker card draws. */
	import type { ImportResult } from "#lib/theme/import/color.ts";
	import ThemePreview from "./ThemePreview.svelte";
	import { previewModel } from "./preview.ts";

	let { result, name }: { result: ImportResult; name: string } = $props();

	let model = $derived(previewModel({ style: result.style, appearance: result.appearance }));
</script>

<figure class="card">
	<ThemePreview {model} />
	<figcaption>{name}</figcaption>
</figure>

<style>
	.card {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 6px 6px 8px;
		border-radius: 12px;
		background: var(--ca-surface-base);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	figcaption {
		padding: 0 4px;
		font-size: 12px;
		font-weight: 550;
		color: var(--ca-text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
