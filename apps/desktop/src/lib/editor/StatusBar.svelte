<script lang="ts">
	import type { Snippet } from "svelte";
	import { statusItems, statusRank, type StatusInput, type StatusItem } from "#lib/editor/statusbar.ts";

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
	const leading = (it: StatusItem) => it.id === "position" || it.id === "problems";
	let left = $derived(items.filter(leading));
	let right = $derived(items.filter((i) => !leading(i)));
</script>

{#snippet cell(it: StatusItem)}
	{#if it.kind === "info"}
		<span class="item" data-rank={statusRank(it.id)} title={it.title}>{it.text}</span>
	{:else}
		<button
			class="item btn"
			class:warning={it.kind === "warning"}
			class:error={it.kind === "error"}
			data-rank={statusRank(it.id)}
			title={it.title}
			onclick={(e) => onaction(it.id, e.currentTarget)}>{it.text}</button
		>
	{/if}
{/snippet}

{#snippet server()}
	<span class="slot" data-rank={statusRank("lsp")}>{@render lsp?.()}</span>
{/snippet}

<div class="status" role="status" aria-label="Editor status">
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
	.item {
		display: block;
		flex: 0 1 auto;
		min-width: 0;
		height: 18px;
		line-height: 18px;
		padding: 0 6px;
		border: 0;
		border-radius: 4px;
		background: none;
		font: inherit;
		color: inherit;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.slot {
		display: inline-flex;
		min-width: 0;
	}
	/* Ranks come from `statusRank`: 1 = language and server, 2 = indent, 3 = encoding and line endings. */
	@container (max-width: 640px) {
		[data-rank="3"] {
			display: none;
		}
	}
	@container (max-width: 520px) {
		[data-rank="2"] {
			display: none;
		}
	}
	@container (max-width: 400px) {
		[data-rank="1"] {
			display: none;
		}
	}
	.btn:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.warning {
		color: var(--ca-semantic-warning);
	}
	.error {
		color: var(--ca-semantic-error);
	}
</style>
