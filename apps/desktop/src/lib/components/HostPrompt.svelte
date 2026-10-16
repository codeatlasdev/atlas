<script lang="ts">
	/**
	 * "Restart the terminal host?": the running host is older than this Atlas and lacks features
	 * (see `host.rs`), and restarting it ends the terminals it runs. Never decided for the user:
	 * restart now, restart the moment nothing runs, or leave it (the sidebar chip stays).
	 */
	import { Button, Dialog } from "@codeatlasdev/ui";
	import { promptCopy } from "#lib/hostStatus.ts";
	import {
		dismissHostPrompt,
		hostPrompt,
		hostState,
		postponeHost,
		restartHost,
		restartHostWhenIdle,
	} from "#lib/stores/hostStatus.svelte.ts";

	let copy = $derived(promptCopy(hostState));
</script>

<Dialog
	bind:open={hostPrompt.open}
	title={copy.title}
	description={copy.body}
	size="md"
	onOpenChange={(o) => !o && dismissHostPrompt()}
>
	{#if copy.items.length}
		<ul class="ptys ca-selectable" aria-label="Running terminals">
			{#each copy.items as item, i (i)}
				<li class="mono">{item}</li>
			{/each}
		</ul>
	{/if}
	<p class="consequence">{copy.consequence}</p>
	{#snippet actions()}
		<Button variant="ghost" onclick={postponeHost}>Later</Button>
		<Button variant="destructive" onclick={restartHost}>Restart now</Button>
		<Button variant="filled" onclick={restartHostWhenIdle}>Restart when idle</Button>
	{/snippet}
</Dialog>

<style>
	.ptys {
		list-style: none;
		margin: 0 0 12px;
		padding: 4px;
		max-height: 176px;
		overflow-y: auto;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		box-shadow: inset 0 0 0 0.5px var(--ca-border-subtle, var(--ca-vibrancy-separator));
	}
	.ptys li {
		padding: 5px 8px;
		border-radius: 5px;
		font-size: 12px;
		color: var(--ca-text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ptys li + li {
		margin-top: 1px;
	}
	.consequence {
		margin: 0;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
</style>
