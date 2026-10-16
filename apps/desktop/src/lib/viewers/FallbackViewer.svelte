<script lang="ts">
	import { formatBytes, hexDump } from "#lib/viewers/kind.ts";
	import ViewerActions from "#lib/viewers/ViewerActions.svelte";

	/** Large or binary file: what it is, what to do with it, and the first bytes when we have them. */
	let {
		reason,
		size,
		head,
		onopen,
		onreveal,
	}: {
		reason: "large" | "binary";
		size: number;
		head?: Uint8Array;
		onopen: () => void;
		onreveal: () => void;
	} = $props();

	let hex = $derived(head ? hexDump(head) : "");
</script>

<div class="fb">
	<div class="card">
		<h3>{reason === "large" ? "Too large to edit here" : "Binary file"}</h3>
		<p>
			{#if reason === "large"}
				This file is {formatBytes(size)}. Atlas opens text files up to 32 MB.
			{:else}
				{formatBytes(size)} · no preview for this format.
			{/if}
		</p>
		<ViewerActions {onopen} {onreveal} />
	</div>
	{#if hex}
		<div class="hexwrap">
			<div class="label">First {formatBytes(head!.length)}</div>
			<pre class="hex ca-selectable">{hex}</pre>
		</div>
	{/if}
</div>

<style>
	.fb {
		flex: 1;
		min-height: 0;
		overflow: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 20px;
		padding: 40px 20px;
	}
	.card {
		text-align: center;
	}
	h3 {
		margin: 0 0 6px;
		font-size: 15px;
		font-weight: 650;
	}
	p {
		margin: 0 0 16px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.hexwrap {
		width: min(100%, 720px);
	}
	.label {
		margin-bottom: 6px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.hex {
		margin: 0;
		padding: 12px 14px;
		overflow: auto;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-text-primary) 4.5%, transparent);
		font-family: var(--mono);
		font-size: 11.5px;
		line-height: 1.55;
		color: var(--ca-text-secondary);
	}
</style>
