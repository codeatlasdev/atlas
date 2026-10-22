<script lang="ts">
	import Contain from "#lib/errors/Contain.svelte";
	import AgentsView from "#lib/components/AgentsView.svelte";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import ConfirmAction from "./ConfirmAction.svelte";
	import NoProject from "./NoProject.svelte";
	import SpacePager from "./SpacePager.svelte";

	let spaces = $derived(ws.view === "worktree" && !!store.current);
</script>

<!-- The pager stays mounted under the agents/dashboard view: coming back is a visibility flip, not a remount. -->
{#if store.current}<div class="host" hidden={!spaces}><SpacePager /></div>{/if}
{#if ws.view !== "worktree"}
	<Contain where="Agents view" title="This view hit an error" hint="The rest of Atlas is unaffected.">
		<AgentsView mode={ws.view} />
	</Contain>
{:else if !store.current}
	<NoProject />
{/if}
{#if !spaces}<ConfirmAction />{/if}

<style>
	.host {
		display: flex;
		flex: 1;
		min-height: 0;
		min-width: 0;
	}
	.host[hidden] {
		display: none;
	}
</style>
