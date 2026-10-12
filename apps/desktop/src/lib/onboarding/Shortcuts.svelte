<script lang="ts">
	import { Dialog, Kbd } from "@codeatlasdev/ui";
	import { Search } from "@lucide/svelte";
	import { allCommands, bindingLabels } from "#lib/keymap.svelte.ts";
	import { onboarding } from "./state.svelte.ts";
	import { buildSheet } from "./shortcuts.ts";

	let query = $state("");
	let input = $state<HTMLInputElement>();
	let groups = $derived(onboarding.helpOpen ? buildSheet(allCommands(), bindingLabels, query) : []);

	$effect(() => {
		if (onboarding.helpOpen) {
			query = "";
			queueMicrotask(() => input?.focus());
		}
	});
</script>

<Dialog bind:open={onboarding.helpOpen} size="lg" title="Keyboard shortcuts">
	<div class="sheet">
		<label class="search">
			<Search size={13} />
			<input bind:this={input} bind:value={query} placeholder="Search shortcuts" spellcheck="false" />
		</label>
		<div class="cols">
			{#each groups as g (g.title)}
				<section>
					<h4>{g.title}</h4>
					{#each g.rows as r (r.id)}
						<div class="row">
							<span class="label">{r.label}</span>
							<span class="keys"
								>{#each r.keys as k (k)}<Kbd>{k}</Kbd>{/each}</span
							>
						</div>
					{/each}
				</section>
			{:else}
				<p class="none">No shortcut matches “{query}”.</p>
			{/each}
		</div>
		<p class="foot">Change any of these in Settings → Keyboard shortcuts.</p>
	</div>
</Dialog>

<style>
	.sheet {
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-height: min(70vh, 560px);
	}
	.search {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 10px;
		height: 32px;
		border-radius: 9px;
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: var(--ca-text-secondary);
	}
	.search input {
		flex: 1;
		border: 0;
		outline: 0;
		background: none;
		font: inherit;
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.cols {
		columns: 2 260px;
		column-gap: 28px;
		overflow: auto;
		padding-right: 4px;
	}
	section {
		break-inside: avoid;
		margin-bottom: 14px;
	}
	h4 {
		margin: 0 0 4px;
		font-size: 11px;
		font-weight: 650;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--ca-text-secondary);
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 4px 0;
		font-size: 12.5px;
	}
	.keys {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
	}
	.none,
	.foot {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
