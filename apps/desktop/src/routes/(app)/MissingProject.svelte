<script lang="ts">
	/** What a project shows when its folder is gone: why, the last known path and the ways out. */
	import { Button, EmptyState, Spinner } from "@codeatlasdev/ui";
	import { FolderSearch, RefreshCw } from "@lucide/svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import {
		locateProject,
		refreshProjects,
		relinkProject,
		removeAllMissing,
		removeProject,
	} from "#lib/components/sidebar/projects.ts";
	import type { Found } from "#lib/components/sidebar/discovery.ts";
	import { tilde } from "#lib/format.ts";
	import { store } from "#lib/stores/projects.svelte.ts";

	let { p }: { p: ProjectView } = $props();

	/** The moved copy Atlas found (same remote or name); `undefined` while looking, `null` = none. */
	let suggestion = $state<Found | null | undefined>(undefined);
	let searched = $state("");

	const why = $derived(
		p.missing === "config"
			? "The folder is still there, but its atlas.yaml is gone."
			: "This folder is no longer on this computer. It may have been deleted, moved or live on a disconnected drive.",
	);

	// Look for a moved copy once per missing project (and again if it is relinked elsewhere).
	$effect(() => {
		const id = p.id;
		if (searched === `${id}:${p.root}`) return;
		searched = `${id}:${p.root}`;
		suggestion = undefined;
		api
			.findMoved(id)
			.then((f) => {
				if (searched === `${id}:${p.root}`) suggestion = f;
			})
			.catch(() => (suggestion = null));
	});
</script>

<div class="drag-strip" data-tauri-drag-region></div>
<div class="wrap">
	<EmptyState icon={FolderSearch} title="{p.name} was not found" description={why}>
		{#snippet actions()}
			<Button onclick={() => locateProject(p.id)}>Locate…</Button>
			<Button variant="ghost" onclick={() => refreshProjects()}><RefreshCw size={13} /> Check again</Button>
			<Button variant="ghost" onclick={() => removeProject(p.id)}>Remove from Atlas</Button>
		{/snippet}
	</EmptyState>
	<p class="path mono ca-selectable" title="Last known path">{tilde(p.root)}</p>

	{#if suggestion === undefined}
		<p class="hint"><Spinner size={12} /> Looking for a moved copy…</p>
	{:else if suggestion}
		<div class="suggest" role="group" aria-label="Moved copy found">
			<div>
				<strong>Probably the moved copy of {p.name}</strong>
				<span class="mono ca-selectable">{tilde(suggestion.path)}</span>
				<small>
					{suggestion.remote ? "Same git remote" : "Same name"}
					{#if suggestion.branch}· {suggestion.branch}{/if}
				</small>
			</div>
			<Button size="sm" onclick={() => suggestion && relinkProject(p.id, suggestion.path)}>Relink</Button>
		</div>
	{/if}

	{#if store.missing.length > 1}
		<Button variant="ghost" size="sm" onclick={() => removeAllMissing()}>
			Remove all {store.missing.length} missing projects…
		</Button>
	{/if}
</div>

<style>
	.drag-strip {
		height: 40px;
	}
	.wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 14px;
		padding: 0 24px 60px;
	}
	.path {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
		max-width: 560px;
		overflow-wrap: anywhere;
		text-align: center;
	}
	.hint {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.suggest {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		max-width: 560px;
		padding: 10px 12px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		background: var(--ca-vibrancy-fill);
	}
	.suggest div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		font-size: 12.5px;
	}
	.suggest .mono {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow-wrap: anywhere;
	}
	.suggest small {
		color: var(--ca-text-secondary);
	}
</style>
