<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import { Check, X } from "@lucide/svelte";
	import { fly } from "svelte/transition";
	import { updates } from "#lib/stores/updates.svelte.ts";

	// Shown once per update; the store clears it when dismissed or when the sheet opens.
	let v = $derived(updates.updatedTo);
</script>

{#if v && !updates.whatsNewOpen}
	<div class="card" role="status" transition:fly={{ y: 12, duration: 220 }}>
		<span class="ok"><Check size={13} strokeWidth={3} /></span>
		<div class="text">
			<b>Updated to {v}</b>
			<span>Atlas is on the latest version.</span>
		</div>
		<Button size="sm" variant="glass" onclick={updates.showWhatsNew}>See what's new</Button>
		<button class="x" aria-label="Dismiss" onclick={updates.dismissUpdated}><X size={13} /></button>
	</div>
{/if}

<style>
	.card {
		position: fixed;
		right: 16px;
		bottom: 16px;
		z-index: 1500;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 10px 10px 12px;
		border-radius: 12px;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-success) 45%, var(--ca-outline-subtle)),
			0 10px 30px -10px rgb(0 0 0 / 0.35);
	}
	.ok {
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		color: white;
		background: var(--ca-semantic-success);
	}
	.text {
		display: flex;
		flex-direction: column;
		line-height: 1.3;
		font-size: 12.5px;
	}
	.text span {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.x {
		display: grid;
		place-items: center;
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
</style>
