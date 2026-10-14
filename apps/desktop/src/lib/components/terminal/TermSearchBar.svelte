<script lang="ts">
	/**
	 * ⌘F find bar of the GPU terminal. The native view paints above the webview, so the bar
	 * takes its own row (the terminal shrinks by its height) instead of floating over the text.
	 */
	import { invoke } from "@tauri-apps/api/core";
	import { primaryKey } from "#lib/keys.ts";
	import FindInput from "../FindInput.svelte";
	import ToggleChip from "../ToggleChip.svelte";

	let {
		tab,
		query = $bindable(""),
		useRegex = $bindable(false),
		matchCase = $bindable(false),
		onclose,
	}: {
		tab: string;
		/** Kept by the parent so the last search survives closing and reopening the bar. */
		query?: string;
		useRegex?: boolean;
		matchCase?: boolean;
		onclose: () => void;
	} = $props();

	let input = $state<HTMLInputElement>();
	let stat = $state<{ index: number; total: number } | null>(null);
	let error = $state(false);

	/** Focus and select the field (opening the bar, or ⌘F again while it is open). */
	export function focus() {
		input?.focus();
		input?.select();
	}

	/** ⌘G / ⇧⌘G: step to the next / previous match without leaving the terminal. */
	export function next(forward: boolean) {
		void find(forward);
	}

	async function find(forward: boolean) {
		if (!query) {
			stat = null;
			error = false;
			return void invoke("term_search_clear", { tab }).catch(() => {});
		}
		try {
			stat = await invoke<{ index: number; total: number }>("term_search", {
				tab,
				query,
				regex: useRegex,
				case: matchCase,
				forward,
			});
			error = false;
		} catch {
			// Invalid regex while typing: keep the last highlights, flag the field.
			error = true;
		}
	}

	function close() {
		void invoke("term_search_clear", { tab }).catch(() => {});
		onclose();
	}

	function onkeydown(e: KeyboardEvent) {
		e.stopPropagation();
		if (e.key === "Enter") {
			e.preventDefault();
			void find(!e.shiftKey);
		} else if (e.key === "Escape") {
			e.preventDefault();
			close();
		} else if (primaryKey(e) && e.key === "f") {
			e.preventDefault();
			input?.select();
		}
	}
</script>

<div class="search" role="search">
	<FindInput
		bind:input
		bind:value={query}
		invalid={error}
		placeholder="Find"
		aria-label="Find in terminal"
		spellcheck="false"
		autocomplete="off"
		oninput={() => void find(true)}
		{onkeydown}
	/>
	<ToggleChip
		on={matchCase}
		label="Match case"
		onclick={() => {
			matchCase = !matchCase;
			void find(true);
		}}>Aa</ToggleChip
	>
	<ToggleChip
		on={useRegex}
		label="Regular expression"
		onclick={() => {
			useRegex = !useRegex;
			void find(true);
		}}>.*</ToggleChip
	>
	<span class="count" aria-live="polite">
		{#if error}invalid{:else if stat && query}{stat.total ? `${stat.index}/${stat.total}` : "0"}{/if}
	</span>
</div>

<style>
	.search {
		display: flex;
		align-items: center;
		gap: 4px;
		justify-content: flex-end;
		padding: 6px 14px 0 10px;
	}
	.count {
		min-width: 36px;
		font-size: 11px;
		color: var(--ca-text-secondary, var(--ca-text-primary));
		font-variant-numeric: tabular-nums;
	}
</style>
