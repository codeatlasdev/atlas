<script lang="ts">
	import { onMount } from "svelte";
	import { registerSearchInput, scheduleSearch, searchState } from "#lib/search/search.svelte.ts";
	import SearchForm from "./SearchForm.svelte";
	import SearchResults from "./SearchResults.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { worktreeId }: { worktreeId: string } = $props();

	let s = $derived(searchState(worktreeId));
	let input: HTMLInputElement | undefined = $state();
	let results: ReturnType<typeof SearchResults> | undefined = $state();

	onMount(() => {
		registerSearchInput(input);
		input?.focus();
		input?.select();
		return () => registerSearchInput(undefined);
	});

	function onInputKey(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			scheduleSearch(worktreeId, true);
		} else if (e.key === "ArrowDown" && results?.enter()) {
			e.preventDefault();
		} else if (e.key === "Escape" && s.query) {
			s.query = "";
			scheduleSearch(worktreeId);
		}
	}
</script>

<aside class="search">
	<div class="head">
		<span>{m.common_action_search()}</span>
	</div>
	<SearchForm {worktreeId} {s} bind:input onkeydown={onInputKey} />
	<SearchResults bind:this={results} {worktreeId} {s} onleaveup={() => input?.focus()} />
</aside>

<style>
	.search {
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		flex-direction: column;
		font-size: 12.5px;
	}
	.head {
		display: flex;
		align-items: center;
		height: 32px;
		flex-shrink: 0;
		padding: 0 12px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
</style>
