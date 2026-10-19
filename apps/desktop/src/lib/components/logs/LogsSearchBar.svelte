<script lang="ts">
	import { kbd } from "#lib/keys.ts";
	import { ChevronDown, ChevronUp } from "@lucide/svelte";

	let {
		query = $bindable(),
		matchCase = $bindable(),
		useRegex = $bindable(),
		hideNonMatching = $bindable(),
		input = $bindable(),
		badRegex,
		matchCount,
		curOrdinal,
		searching,
		onstep,
		onescape,
	}: {
		query: string;
		matchCase: boolean;
		useRegex: boolean;
		hideNonMatching: boolean;
		input: HTMLInputElement | undefined;
		badRegex: boolean;
		matchCount: number;
		curOrdinal: number;
		searching: boolean;
		/** ⏎ / ⇧⏎ / arrows and the chevrons. */
		onstep: (dir: 1 | -1) => void;
		/** ⎋ on an already empty box hands focus back to the log body. */
		onescape: () => void;
	} = $props();

	/** First ⏎ after typing selects the match nearest the bottom (newest) when following, like a terminal. */
	function onkeydown(e: KeyboardEvent) {
		e.stopPropagation();
		if (e.key === "Enter") {
			e.preventDefault();
			onstep(e.shiftKey ? -1 : 1);
		} else if (e.key === "Escape") {
			e.preventDefault();
			if (query) query = "";
			else onescape();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			onstep(1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			onstep(-1);
		}
	}
</script>

<div class="search" class:bad={badRegex} role="search">
	<input
		bind:this={input}
		bind:value={query}
		placeholder={hideNonMatching ? "Filter" : "Find"}
		aria-label="Search logs"
		spellcheck="false"
		autocomplete="off"
		{onkeydown}
	/>
	<button
		class="toggle"
		class:on={matchCase}
		title="Match case"
		aria-label="Match case"
		aria-pressed={matchCase}
		onclick={() => (matchCase = !matchCase)}>Aa</button
	>
	<button
		class="toggle"
		class:on={useRegex}
		title="Regular expression"
		aria-label="Regular expression"
		aria-pressed={useRegex}
		onclick={() => (useRegex = !useRegex)}>.*</button
	>
	<button
		class="toggle"
		class:on={hideNonMatching}
		title={hideNonMatching ? "Filtering: hide lines that do not match" : "Finding: highlight matches"}
		aria-label="Hide non-matching lines"
		aria-pressed={hideNonMatching}
		onclick={() => (hideNonMatching = !hideNonMatching)}>Filter</button
	>
	<span class="count" aria-live="polite">
		{#if badRegex}<span class="badtxt">invalid</span>{:else if query}{#if matchCount}{curOrdinal
					? `${curOrdinal}/`
					: ""}{matchCount.toLocaleString()}{searching ? "+" : ""}{:else}{searching ? "…" : "0"}{/if}{/if}
	</span>
	<button
		class="icon"
		title={`Previous match (${kbd("shift-enter")})`}
		aria-label="Previous match"
		onclick={() => onstep(-1)}><ChevronUp size={14} /></button
	>
	<button class="icon" title="Next match (↵)" aria-label="Next match" onclick={() => onstep(1)}
		><ChevronDown size={14} /></button
	>
</div>

<style>
	.search {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.search input {
		font: inherit;
		font-size: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		background: var(--ca-surface-raised, var(--ca-bg-default));
		color: var(--ca-text-primary);
		width: 220px;
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.search.bad input {
		border-color: var(--ca-semantic-error);
		color: var(--ca-semantic-error);
	}
	.toggle {
		font: inherit;
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.toggle.on {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.count {
		min-width: 44px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
	.badtxt {
		color: var(--ca-semantic-error);
	}
	.icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 1px;
		min-width: 26px;
		height: 26px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.icon:hover {
		background: var(--ca-action-hover);
	}
</style>
