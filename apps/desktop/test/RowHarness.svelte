<script lang="ts">
	// Test host for a clickable row that carries a "…" menu, as ProjectHead / WorktreeRow / Explorer do.
	// `legacy` is the markup those rows had: the kit `trigger` snippet with a span inside the row.
	import { DropdownMenu, DropdownMenuItem } from "@codeatlasdev/ui";
	import RowMore from "#lib/components/RowMore.svelte";
	import { fromMenu } from "#lib/components/rowEvent.ts";

	/** Right-click on the row opens its menu through the `…` button, as ProjectHead does. */
	let button: HTMLButtonElement | undefined = $state();

	let {
		legacy = false,
		onactivate,
		onpick,
	}: { legacy?: boolean; onactivate: () => void; onpick: () => void } = $props();
</script>

<div
	class="row"
	role="button"
	tabindex="0"
	oncontextmenu={(e) => {
		e.preventDefault();
		if (!fromMenu(e)) button?.click();
	}}
	onclick={(e) => fromMenu(e) || onactivate()}
	onkeydown={(e) => e.key === "Enter" && !fromMenu(e) && onactivate()}
>
	<span class="name">row</span>
	{#if legacy}
		<DropdownMenu align="end">
			{#snippet trigger()}
				<span class="more" aria-label="Menu">…</span>
			{/snippet}
			<DropdownMenuItem onclick={onpick}>Pick</DropdownMenuItem>
		</DropdownMenu>
	{:else}
		<RowMore label="Menu" class="more" bind:el={button}>
			{#snippet icon()}…{/snippet}
			<DropdownMenuItem onclick={onpick}>Pick</DropdownMenuItem>
		</RowMore>
	{/if}
</div>
