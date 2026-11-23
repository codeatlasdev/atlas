<script lang="ts">
	/** Rendered markdown, block by block: a streaming message only rebuilds the blocks that changed (markdownBlocks.ts). */
	import { markdownBlocks } from "./markdownBlocks.ts";

	let { text }: { text: string } = $props();
	const blocks = $derived(markdownBlocks(text));
</script>

{#each blocks as html, i (`${i}:${html}`)}{@html html}{/each}
