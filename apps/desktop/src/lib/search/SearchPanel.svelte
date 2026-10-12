<script lang="ts">
	import { onMount, tick } from "svelte";
	import { Spinner } from "@codeatlasdev/ui";
	import {
		Search,
		CaseSensitive,
		Regex,
		WholeWord,
		ChevronRight,
		ChevronDown,
		SlidersHorizontal,
		X,
	} from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { fileIcon } from "#lib/icons.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { registerSearchInput, scheduleSearch, searchState } from "#lib/search/search.svelte.ts";
	import { flatRows, previewRuns } from "#lib/search/rows.ts";

	let { worktreeId }: { worktreeId: string } = $props();

	let s = $derived(searchState(worktreeId));
	let collapsed = $state<Record<string, boolean>>({});
	let cursor = $state(0);
	let input: HTMLInputElement | undefined = $state();
	let list: HTMLDivElement | undefined = $state();

	let rows = $derived(flatRows(s.files, collapsed));

	onMount(() => {
		registerSearchInput(input);
		input?.focus();
		input?.select();
		return () => registerSearchInput(undefined);
	});

	// A new result set restarts keyboard navigation at the top.
	$effect(() => {
		void s.files;
		cursor = 0;
		collapsed = {};
	});

	const change = () => scheduleSearch(worktreeId);

	function open(i: number) {
		const r = rows[i];
		if (!r) return;
		if (r.kind === "file") collapsed[r.path] = !collapsed[r.path];
		else openFile(worktreeId, r.path, r.match.line, r.match.start + 1);
	}

	async function move(to: number) {
		cursor = Math.min(rows.length - 1, Math.max(0, to));
		await tick();
		list?.querySelector(`[data-row="${cursor}"]`)?.scrollIntoView({ block: "nearest" });
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === "ArrowDown") (e.preventDefault(), void move(cursor + 1));
		else if (e.key === "ArrowUp") {
			e.preventDefault();
			if (cursor === 0) input?.focus();
			else void move(cursor - 1);
		} else if (e.key === "Enter") (e.preventDefault(), open(cursor));
		else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
			const r = rows[cursor];
			if (r?.kind === "file") collapsed[r.path] = e.key === "ArrowLeft";
			else if (r && e.key === "ArrowLeft") {
				const i = rows.findLastIndex((x, j) => j < cursor && x.kind === "file");
				if (i >= 0) void move(i);
			}
		} else return;
		e.stopPropagation();
	}

	function onInputKey(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			scheduleSearch(worktreeId, true);
		} else if (e.key === "ArrowDown" && rows.length) {
			e.preventDefault();
			void move(0);
			list?.focus();
		} else if (e.key === "Escape" && s.query) {
			s.query = "";
			change();
		}
	}

	const dirOf = (p: string) => p.split("/").slice(0, -1).join("/");
	const nameOf = (p: string) => p.split("/").pop() ?? p;
</script>

<aside class="search">
	<div class="head">
		<span>Search</span>
	</div>
	<div class="form">
		<div class="field">
			<Search size={13} />
			<input
				bind:this={input}
				bind:value={s.query}
				oninput={change}
				onkeydown={onInputKey}
				placeholder="Find in project"
				spellcheck="false"
				autocomplete="off"
				aria-label="Search query"
			/>
			{#if s.query}
				<button
					class="tog"
					aria-label="Clear"
					onclick={() => {
						s.query = "";
						change();
						input?.focus();
					}}><X size={12} /></button
				>
			{/if}
			<Tip label="Match case" side="bottom">
				{#snippet child(props)}
					<button
						{...props}
						class="tog"
						class:on={s.caseSensitive}
						aria-pressed={s.caseSensitive}
						aria-label="Match case"
						onclick={() => ((s.caseSensitive = !s.caseSensitive), change())}
						><CaseSensitive size={14} /></button
					>
				{/snippet}
			</Tip>
			<Tip label="Whole word" side="bottom">
				{#snippet child(props)}
					<button
						{...props}
						class="tog"
						class:on={s.wholeWord}
						aria-pressed={s.wholeWord}
						aria-label="Whole word"
						onclick={() => ((s.wholeWord = !s.wholeWord), change())}><WholeWord size={14} /></button
					>
				{/snippet}
			</Tip>
			<Tip label="Regular expression" side="bottom">
				{#snippet child(props)}
					<button
						{...props}
						class="tog"
						class:on={s.regex}
						aria-pressed={s.regex}
						aria-label="Regular expression"
						onclick={() => ((s.regex = !s.regex), change())}><Regex size={14} /></button
					>
				{/snippet}
			</Tip>
		</div>
		<button class="filters" onclick={() => (s.showFilters = !s.showFilters)} aria-expanded={s.showFilters}>
			<SlidersHorizontal size={11} />
			Files to include or skip
			{#if s.include || s.exclude}<span class="dot"></span>{/if}
		</button>
		{#if s.showFilters}
			<div class="globs">
				<input
					bind:value={s.include}
					oninput={change}
					placeholder="Include: src/**, *.ts"
					spellcheck="false"
					aria-label="Files to include"
				/>
				<input
					bind:value={s.exclude}
					oninput={change}
					placeholder="Skip: dist, *.min.js"
					spellcheck="false"
					aria-label="Files to skip"
				/>
			</div>
		{/if}
	</div>

	<div class="status" aria-live="polite">
		{#if s.phase === "running"}
			<Spinner size="small" /> Searching…
		{:else if s.phase === "done"}
			{#if s.matches}
				{s.matches}{s.truncated ? "+" : ""} result{s.matches === 1 ? "" : "s"} in {s.files.length}{s.truncated
					? "+"
					: ""} file{s.files.length === 1 ? "" : "s"}
				<span class="took">{s.tookMs} ms · {s.tool}</span>
			{:else}
				No results
			{/if}
		{:else if s.phase === "error"}
			<span class="err">{s.error}</span>
		{/if}
	</div>

	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="list" bind:this={list} {onkeydown} tabindex="0" role="tree" aria-label="Search results">
		{#if s.phase === "idle"}
			<div class="hint">
				<p>Search every file in this worktree.</p>
				<p class="sub">Type to search. Use ↑ ↓ to move, Enter to open at the line.</p>
			</div>
		{:else if s.phase === "done" && !s.files.length}
			<div class="hint">
				<p>Nothing matches “{s.query}”.</p>
				<p class="sub">Try fewer words, turn off whole word, or clear the file filters.</p>
			</div>
		{/if}
		{#each rows as r, i (r.key)}
			{#if r.kind === "file"}
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
					<span class="ico">{@html fileIcon(nameOf(r.path))}</span>
					<span class="fname">{nameOf(r.path)}</span>
					<span class="fdir">{dirOf(r.path)}</span>
					<span class="count">{r.count}</span>
				</div>
			{:else}
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
					<span class="txt"
						>{#each previewRuns(r.match) as run, k (k)}{#if run.hit}<mark>{run.text}</mark
								>{:else}{run.text}{/if}{/each}</span
					>
				</div>
			{/if}
		{/each}
		{#if s.truncated}
			<p class="more">Showing the first results. Narrow the search to see the rest.</p>
		{/if}
	</div>
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
		border: 0.5px solid var(--ca-outline-subtle);
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
		color: var(--space-accent, var(--ca-brand-main));
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
		align-items: center;
		gap: 6px;
		min-height: 20px;
		padding: 0 12px 4px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.took {
		margin-left: auto;
		color: var(--ca-text-disabled);
	}
	.err {
		color: var(--ca-semantic-error, #d33);
	}
	.list {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 2px 0 10px;
		outline: none;
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
		color: var(--ca-text-disabled);
	}
	.file,
	.match {
		display: flex;
		align-items: center;
		gap: 5px;
		margin: 0 4px;
		border-radius: 5px;
		cursor: default;
	}
	.file {
		height: 24px;
		padding: 0 6px;
		margin-top: 2px;
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
		color: var(--ca-text-disabled);
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
		color: var(--ca-text-disabled);
	}
</style>
