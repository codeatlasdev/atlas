<script lang="ts">
	import { ChevronRight, ChevronDown } from "@lucide/svelte";
	import { fileIcon } from "#lib/icons.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import type { SearchState } from "#lib/search/search.svelte.ts";
	import { dirOf, flatRows, nameOf, navKey, previewRuns, type Row } from "#lib/search/rows.ts";
	import VirtualList from "#lib/components/VirtualList.svelte";
	import { m } from "#lib/i18n/m.ts";
	import { kbd } from "#lib/keys.ts";

	/** Result tree (files with their matches) and its keyboard navigation. */
	let {
		worktreeId,
		s,
		list = $bindable(),
		onleaveup,
	}: {
		worktreeId: string;
		s: SearchState;
		list?: HTMLDivElement;
		/** ArrowUp from the first row: hand focus back to the query field. */
		onleaveup: () => void;
	} = $props();

	let collapsed = $state<Record<string, boolean>>({});
	let cursor = $state(0);
	let vl: ReturnType<typeof VirtualList<Row>> | undefined = $state();

	let rows = $derived(flatRows(s.files, collapsed));

	/** Move from the query field into the list (ArrowDown). */
	export function enter() {
		if (!rows.length) return false;
		move(0);
		list?.focus();
		return true;
	}

	// A new result set restarts keyboard navigation at the top.
	$effect(() => {
		void s.files;
		cursor = 0;
		collapsed = {};
	});

	function open(i: number) {
		const r = rows[i];
		if (!r) return;
		if (r.kind === "file") collapsed[r.path] = !collapsed[r.path];
		else openFile(worktreeId, r.path, r.match.line, r.match.start + 1);
	}

	function move(to: number) {
		cursor = Math.min(rows.length - 1, Math.max(0, to));
		// The cursor's row may not be in the DOM (the list renders a window): scroll by index, not by element.
		vl?.reveal(cursor);
	}

	function onkeydown(e: KeyboardEvent) {
		const act = navKey(e.key, rows, cursor);
		if (!act) return;
		if (act.type === "move") (e.preventDefault(), move(act.to));
		else if (act.type === "leave") (e.preventDefault(), onleaveup());
		else if (act.type === "open") (e.preventDefault(), open(cursor));
		else if (act.type === "fold") collapsed[act.path] = act.collapsed;
		// "swallow": a handled key with nothing to do (→ on a match row) still must not bubble.
		e.stopPropagation();
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
	class="list"
	bind:this={list}
	{onkeydown}
	tabindex="0"
	role="tree"
	aria-label={m.search_results_search_results()}
>
	<VirtualList bind:this={vl} class="vlist" items={rows} key={(r) => r.key} estimate={22}>
		{#snippet header()}
			{#if s.phase === "idle"}
				<div class="hint">
					<p>{m.search_results_search_every_file_in_this()}</p>
					<p class="sub">
						{m.search_results_type_to_search_use_to({
							up: kbd("up"),
							down: kbd("down"),
							enter: kbd("enter"),
						})}
					</p>
				</div>
			{:else if s.phase === "done" && !s.files.length}
				<div class="hint">
					<p>{m.search_results_nothing_matches_query({ query: s.query })}</p>
					<p class="sub">{m.search_results_try_fewer_words_turn_off()}</p>
				</div>
			{/if}
		{/snippet}
		{#snippet item(r, i)}
			{#if r.kind === "file"}
				<div class="box head">
					<div
						class="file"
						class:cur={cursor === i}
						data-row={i}
						role="treeitem"
						aria-selected={cursor === i}
						aria-expanded={!collapsed[r.path]}
						tabindex="-1"
						onclick={() => ((cursor = i), open(i))}
						onkeydown={(e) => e.key === "Enter" && open(i)}
					>
						{#if collapsed[r.path]}<ChevronRight size={11} />{:else}<ChevronDown size={11} />{/if}
						<span class="ico" data-contrast-decorative>{@html fileIcon(nameOf(r.path))}</span>
						<span class="fname">{nameOf(r.path)}</span>
						<span class="fdir">{dirOf(r.path)}</span>
						<span class="count">{r.count}</span>
					</div>
				</div>
			{:else}
				<div class="box">
					<div
						class="match"
						class:cur={cursor === i}
						data-row={i}
						role="treeitem"
						aria-selected={cursor === i}
						tabindex="-1"
						onclick={() => ((cursor = i), open(i))}
						onkeydown={(e) => e.key === "Enter" && open(i)}
					>
						<span class="ln">{r.match.line}</span>
						<span class="txt">
							{#each previewRuns(r.match) as run, k (k)}{#if run.hit}<mark>
										{run.text}
									</mark>{:else}{run.text}{/if}{/each}
						</span>
					</div>
				</div>
			{/if}
		{/snippet}
		{#snippet footer()}
			{#if s.truncated}
				<p class="more">{m.search_results_showing_the_first_results_narrow()}</p>
			{/if}
			<div class="end"></div>
		{/snippet}
	</VirtualList>
</div>

<style>
	.list {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		outline: none;
	}
	.list :global(.vlist) {
		flex: 1;
	}
	/* Padding, not margin: the list measures each row's box, and a margin would collapse out of it. */
	.box {
		padding: 0 4px;
	}
	.box.head {
		padding-top: 2px;
	}
	.end {
		height: 10px;
	}
	.hint {
		padding: 20px 16px;
		color: var(--ca-text-secondary);
	}
	.hint p {
		margin: 0 0 4px;
	}
	.hint .sub {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.file,
	.match {
		display: flex;
		align-items: center;
		gap: 5px;
		margin: 0;
		border-radius: 5px;
		cursor: default;
	}
	.file {
		height: 24px;
		padding: 0 6px;
	}
	.match {
		min-height: 22px;
		padding: 1px 6px 1px 24px;
		align-items: baseline;
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.file:hover,
	.match:hover {
		background: var(--ca-action-hover);
	}
	.list:focus-within .cur,
	.cur {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 16%, transparent);
	}
	.ico {
		display: inline-flex;
	}
	.ico :global(svg) {
		width: 15px;
		height: 15px;
	}
	.fname {
		font-weight: 550;
		white-space: nowrap;
	}
	.fdir {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.count {
		padding: 0 6px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 9%, transparent);
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}
	.ln {
		min-width: 28px;
		text-align: right;
		color: var(--ca-text-disabled);
		flex-shrink: 0;
	}
	.txt {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: pre;
		color: var(--ca-text-secondary);
	}
	mark {
		border-radius: 3px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 38%, transparent);
		color: var(--ca-text-primary);
	}
	.more {
		margin: 8px 12px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
</style>
