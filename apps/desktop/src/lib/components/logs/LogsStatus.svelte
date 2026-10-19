<script lang="ts">
	import { kbd, modClick } from "#lib/keys.ts";
	import { Kbd, Spinner } from "@codeatlasdev/ui";

	let {
		total,
		ofTotal,
		errors,
		warns,
		filtering,
	}: {
		total: number;
		/** Buffered lines when a filter narrows the view, else null. */
		ofTotal: number | null;
		errors: number;
		warns: number;
		filtering: boolean;
	} = $props();
</script>

<div class="status">
	<span>
		{total.toLocaleString()} lines{ofTotal === null ? "" : ` of ${ofTotal.toLocaleString()}`}
	</span>
	{#if errors}<span class="e">{errors.toLocaleString()} errors</span>{/if}
	{#if warns}<span class="w">{warns.toLocaleString()} warnings</span>{/if}
	{#if filtering}<span class="busy"><Spinner size={11} label="Working" /> filtering…</span>{/if}
	<span class="hint"
		><Kbd>{kbd("cmd-f")}</Kbd> find · <Kbd>e</Kbd>
		<Kbd>w</Kbd> next error / warn · {modClick()} links ·
		<Kbd>{kbd("cmd-l")}</Kbd> logs</span
	>
</div>

<style>
	.status {
		display: flex;
		gap: 12px;
		align-items: center;
		padding: 5px 16px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.status .e {
		color: var(--ca-semantic-error);
	}
	.status .w {
		color: var(--ca-semantic-warning);
	}
	.busy {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.hint {
		margin-left: auto;
	}
</style>
