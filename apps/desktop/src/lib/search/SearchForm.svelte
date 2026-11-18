<script lang="ts">
	import { Spinner } from "@codeatlasdev/ui";
	import { Search, CaseSensitive, Regex, WholeWord, SlidersHorizontal, X } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { scheduleSearch, type SearchState } from "#lib/search/search.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
	import { formatNumber } from "#lib/i18n/fmt.ts";

	/** Query field with its toggles, the include/skip globs and the one-line status. */
	let {
		worktreeId,
		s,
		input = $bindable(),
		onkeydown,
	}: {
		worktreeId: string;
		s: SearchState;
		input?: HTMLInputElement;
		/** Keys of the query field (the results list drives navigation from it). */
		onkeydown: (e: KeyboardEvent) => void;
	} = $props();

	const change = () => scheduleSearch(worktreeId);
	// A search that stopped at its limit counts "100+": the number picks the plural, the label is what is shown.
	const more = $derived(s.truncated ? "+" : "");
</script>

<div class="form">
	<div class="field">
		<Search size={13} />
		<input
			bind:this={input}
			bind:value={s.query}
			oninput={change}
			{onkeydown}
			placeholder={m.search_form_find_in_project()}
			spellcheck="false"
			autocomplete="off"
			aria-label={m.search_form_search_query()}
		/>
		{#if s.query}
			<button
				class="tog"
				aria-label={m.search_form_clear()}
				onclick={() => {
					s.query = "";
					change();
					input?.focus();
				}}><X size={12} /></button
			>
		{/if}
		<Tip label={m.search_form_match_case()} side="bottom">
			{#snippet child(props)}
				<button
					{...props}
					class="tog"
					class:on={s.caseSensitive}
					aria-pressed={s.caseSensitive}
					aria-label={m.search_form_match_case()}
					onclick={() => ((s.caseSensitive = !s.caseSensitive), change())}><CaseSensitive size={14} /></button
				>
			{/snippet}
		</Tip>
		<Tip label={m.search_form_whole_word()} side="bottom">
			{#snippet child(props)}
				<button
					{...props}
					class="tog"
					class:on={s.wholeWord}
					aria-pressed={s.wholeWord}
					aria-label={m.search_form_whole_word()}
					onclick={() => ((s.wholeWord = !s.wholeWord), change())}><WholeWord size={14} /></button
				>
			{/snippet}
		</Tip>
		<Tip label={m.search_form_regular_expression()} side="bottom">
			{#snippet child(props)}
				<button
					{...props}
					class="tog"
					class:on={s.regex}
					aria-pressed={s.regex}
					aria-label={m.search_form_regular_expression()}
					onclick={() => ((s.regex = !s.regex), change())}><Regex size={14} /></button
				>
			{/snippet}
		</Tip>
	</div>
	<button class="filters" onclick={() => (s.showFilters = !s.showFilters)} aria-expanded={s.showFilters}>
		<SlidersHorizontal size={11} />
		{m.search_form_files_to_include_or_skip()}
		{#if s.include || s.exclude}<span class="dot"></span>{/if}
	</button>
	{#if s.showFilters}
		<div class="globs">
			<input
				bind:value={s.include}
				oninput={change}
				placeholder={m.search_form_include_src_ts()}
				spellcheck="false"
				aria-label={m.search_form_files_to_include()}
			/>
			<input
				bind:value={s.exclude}
				oninput={change}
				placeholder={m.search_form_skip_dist_min_js()}
				spellcheck="false"
				aria-label={m.search_form_files_to_skip()}
			/>
		</div>
	{/if}
</div>

<div class="status" aria-live="polite">
	{#if s.phase === "running"}
		<Spinner size="small" /> {m.search_form_searching()}
	{:else if s.phase === "done"}
		<span class="count"
			>{m.search_form_status({
				matches: s.matches,
				files: s.files.length,
				matchesLabel: formatNumber(s.matches) + more,
				filesLabel: formatNumber(s.files.length) + more,
			})}</span
		>
		{#if s.matches}<span class="took">{m.search_form_took({ ms: s.tookMs, tool: s.tool })}</span>{/if}
	{:else if s.phase === "error"}
		<span class="err">{s.error}</span>
	{/if}
</div>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px;
	}
	.field,
	.globs input {
		display: flex;
		align-items: center;
		gap: 2px;
		height: 28px;
		padding: 0 4px 0 8px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 4%, transparent);
		color: var(--ca-text-secondary);
		transition:
			border-color 140ms ease-out,
			background 140ms ease-out;
	}
	.field:focus-within,
	.globs input:focus {
		border-color: var(--space-accent, var(--ca-brand-main));
		background: transparent;
	}
	.field input {
		flex: 1;
		min-width: 0;
		height: 100%;
		border: 0;
		outline: 0;
		background: none;
		color: var(--ca-text-primary);
		font: inherit;
		padding: 0 4px;
	}
	.globs {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.globs input {
		padding: 0 8px;
		font: inherit;
		font-size: 12px;
		outline: 0;
		color: var(--ca-text-primary);
	}
	.tog {
		display: inline-grid;
		place-items: center;
		width: 22px;
		height: 22px;
		flex-shrink: 0;
		border: 0;
		border-radius: 5px;
		background: none;
		color: inherit;
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.tog:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.tog.on {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 22%, transparent);
		color: var(--space-accent-text, var(--ca-brand-main));
	}
	.filters {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		align-self: flex-start;
		padding: 2px 4px;
		border: 0;
		border-radius: 5px;
		background: none;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.filters:hover {
		color: var(--ca-text-primary);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--space-accent, var(--ca-brand-main));
	}
	.status {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 6px;
		row-gap: 0;
		min-height: 20px;
		padding: 0 12px 4px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.count {
		min-width: 0;
	}
	/* The timing never breaks in the middle ("26 ms · rg"): when a longer language leaves no room, it drops to its own line. */
	.took {
		margin-left: auto;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.err {
		color: var(--ca-semantic-error, #d33);
	}
</style>
