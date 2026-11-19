<script lang="ts">
	import type { Snippet } from "svelte";
	import StatusItem from "#lib/editor/StatusItem.svelte";
	import {
		statusItems,
		statusRank,
		type StatusInput,
		type StatusItem as Item,
	} from "#lib/editor/statusbar.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		input,
		onaction,
		lsp,
	}: {
		input: StatusInput;
		/** A clickable item was activated (`position`, `indent`, `eol`, `encoding`, `language`, `problems`). */
		onaction: (id: string, anchor: HTMLElement) => void;
		/** The language-server item (its own popover); rendered after the language mode. */
		lsp?: Snippet;
	} = $props();

	let items = $derived(statusItems(input));
	const leading = (it: Item) => it.id === "position" || it.id === "problems";
	let left = $derived(items.filter(leading));
	let right = $derived(items.filter((i) => !leading(i)));
</script>

{#snippet cell(it: Item)}
	{#if it.kind === "info"}
		<StatusItem data-rank={statusRank(it.id)} title={it.title}>{it.text}</StatusItem>
	{:else}
		<StatusItem
			button
			tone={it.kind === "warning" || it.kind === "error" ? it.kind : undefined}
			data-rank={statusRank(it.id)}
			title={it.title}
			onclick={(e) => onaction(it.id, e.currentTarget)}>{it.text}</StatusItem
		>
	{/if}
{/snippet}

{#snippet server()}
	<span class="slot" data-rank={statusRank("lsp")}>{@render lsp?.()}</span>
{/snippet}

<div class="status" role="status" aria-label={m.workbench_editor_status_bar()}>
	<div class="side lead">
		{#each left as it (it.id)}{@render cell(it)}{/each}
	</div>
	<div class="side">
		{#each right as it (it.id)}
			{#if it.id === "language"}
				{@render cell(it)}
				{@render server()}
			{:else}
				{@render cell(it)}
			{/if}
		{/each}
		{#if !right.some((i) => i.id === "language")}{@render server()}{/if}
	</div>
</div>

<style>
	.status {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 22px;
		padding: 0 6px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		line-height: 1;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		/* The bar answers to its own width (the editor sits in a pane of any size), so the rules below hide the
		   least useful items first instead of letting two items print over each other. */
		container-type: inline-size;
	}
	.side {
		display: flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
	}
	/* The position and problems come first and never give way to the right side; the right side shrinks. */
	.lead {
		flex: 0 1 auto;
	}
	.side:not(.lead) {
		flex: 0 1 auto;
		overflow: hidden;
	}
	.slot {
		display: inline-flex;
		min-width: 0;
	}
	/* Ranks come from `statusRank`: 1 = language and server, 2 = indent, 3 = encoding and line endings. */
	@container (max-width: 640px) {
		.status :global([data-rank="3"]) {
			display: none;
		}
	}
	@container (max-width: 520px) {
		.status :global([data-rank="2"]) {
			display: none;
		}
	}
	@container (max-width: 400px) {
		.status :global([data-rank="1"]) {
			display: none;
		}
	}
</style>
