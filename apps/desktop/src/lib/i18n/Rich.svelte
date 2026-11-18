<script lang="ts">
	/**
	 * One message with markup tags, drawn whole: `<Rich parts={m.key.parts({ n })} tags={{ em: "em" }} />`. A tag maps to
	 * an element name (`"em"`, `"strong"`, `"code"`) or to a snippet that receives the tag's content, for a link or a
	 * styled span. A tag the message uses and `tags` does not name renders its content plain.
	 */
	import type { Snippet } from "svelte";
	import type { MessagePart } from "#lib/paraglide/runtime.js";
	import { richTree, type RichNode } from "./rich.ts";

	let { parts, tags }: { parts: MessagePart[]; tags: Record<string, string | Snippet<[Snippet]>> } = $props();

	const tree = $derived(richTree(parts));
</script>

{#snippet draw(nodes: RichNode[])}
	{#each nodes as node, i (i)}
		{#if node.kind === "text"}
			{node.value}
		{:else}
			{@const tag = tags[node.name]}
			{#snippet content()}{@render draw(node.children)}{/snippet}
			{#if typeof tag === "string"}
				<svelte:element this={tag}>{@render content()}</svelte:element>
			{:else if tag}
				{@render tag(content)}
			{:else}
				{@render content()}
			{/if}
		{/if}
	{/each}
{/snippet}

{@render draw(tree)}
