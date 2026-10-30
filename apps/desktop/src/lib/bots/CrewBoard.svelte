<script lang="ts">
	/**
	 * CrewBoard — Kanban view of all bots' tasks across every reachable host.
	 * Four columns: Queued · Working · Needs you · Done (today only).
	 * Replaces the right panel (BotView) when crewView === 'board'.
	 */
	import { onMount } from "svelte";
	import { flip } from "svelte/animate";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { crewView } from "#lib/bots/crewView.svelte.ts";
	import { boardColumns } from "#lib/bots/board.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { workingLabel } from "#lib/bots/stepPhrase.ts";
	import type { BoardCard, BoardFilterOptions, ColumnId } from "#lib/bots/board.ts";
	import type { Bot } from "#lib/bots.ts";

	// ─── clock ────────────────────────────────────────────────────────────────

	let now = $state(Date.now());
	$effect(() => {
		const id = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(id);
	});

	// ─── board data ───────────────────────────────────────────────────────────

	onMount(() => {
		bots.boardOpen = true;
		if (bots.boardEntries.length === 0) void bots.loadBoardTasks();
		return () => {
			bots.boardOpen = false;
		};
	});

	// ─── filter state ─────────────────────────────────────────────────────────

	const FILTERS_KEY = "crew-board-filters";

	interface PersistedFilters {
		selectedBots: string[];
		source: "all" | "chat" | "auto";
		query: string;
		showDone: boolean;
		doneDays: 1 | 7;
	}

	function loadFilters(): PersistedFilters {
		try {
			const raw = localStorage.getItem(FILTERS_KEY);
			if (raw) {
				const p = JSON.parse(raw) as Partial<PersistedFilters>;
				return {
					selectedBots: Array.isArray(p.selectedBots) ? p.selectedBots : [],
					source: p.source === "chat" || p.source === "auto" ? p.source : "all",
					query: typeof p.query === "string" ? p.query : "",
					showDone: p.showDone !== false,
					doneDays: p.doneDays === 7 ? 7 : 1,
				};
			}
		} catch {}
		return { selectedBots: [], source: "all", query: "", showDone: true, doneDays: 1 };
	}

	const saved = loadFilters();
	let selectedBots = $state(new Set<string>(saved.selectedBots));
	let source = $state<"all" | "chat" | "auto">(saved.source);
	let query = $state(saved.query);
	let showDone = $state(saved.showDone);
	let doneDays = $state<1 | 7>(saved.doneDays);

	$effect(() => {
		try {
			localStorage.setItem(
				FILTERS_KEY,
				JSON.stringify({ selectedBots: [...selectedBots], source, query, showDone, doneDays }),
			);
		} catch {}
	});


	// bots that actually have at least one card (used to build chips)
	const unfilteredCols = $derived(boardColumns(bots.boardEntries, now));
	const availableBots = $derived.by(() => {
		const seen = new Map<string, { hostKey: string; bot: Bot }>();
		for (const col of unfilteredCols) {
			for (const card of col.cards) {
				const k = `${card.hostKey}\u0000${card.bot.id}`;
				if (!seen.has(k)) seen.set(k, { hostKey: card.hostKey, bot: card.bot });
			}
		}
		return [...seen.values()];
	});

	// Only the bots that have a chip on screen count. A saved pick whose bot lost its cards (or was deleted) had no
	// chip to unselect yet still filtered everything out, which left an empty board with no visible filter.
	const activeBots = $derived(
		new Set([...selectedBots].filter((k) => availableBots.some((b) => `${b.hostKey}\u0000${b.bot.id}` === k))),
	);
	const filterOpts = $derived<BoardFilterOptions>({
		...(activeBots.size > 0 ? { bots: activeBots } : {}),
		...(source !== "all" ? { source } : {}),
		...(query.trim() ? { query: query.trim() } : {}),
		doneDays: showDone ? doneDays : 1,
	});
	const allCols = $derived(boardColumns(bots.boardEntries, now, filterOpts));
	const columns = $derived(showDone ? allCols : allCols.filter((c) => c.id !== "done"));

	const hasActiveFilters = $derived(activeBots.size > 0 || source !== "all" || query.trim().length > 0);
	const showGlobalEmpty = $derived(columns.every((c) => c.cards.length === 0) && hasActiveFilters);

	function toggleBot(key: string) {
		const next = new Set(selectedBots);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		selectedBots = next;
	}

	function clearFilters() {
		selectedBots = new Set();
		source = "all";
		query = "";
	}

	// ─── card interaction ─────────────────────────────────────────────────────

	/** Open the chat at this task, not just the bot: the conversation scrolls to it and flashes it. */
	function openCard(card: BoardCard) {
		bots.focusTask = card.task.id;
		void bots.select(card.hostKey, card.bot.id);
		crewView.value = "chat";
	}

	// ─── helpers ──────────────────────────────────────────────────────────────

	/** Elapsed ms → "1m 23s" style label. */
	function elapsed(startedAt: number | undefined, n: number): string {
		const ms = n - (startedAt ?? n);
		if (ms < 0) return "";
		const s = Math.floor(ms / 1000);
		if (s < 60) return `${s}s`;
		const m = Math.floor(s / 60);
		const rem = s % 60;
		return rem === 0 ? `${m}m` : `${m}m ${rem}s`;
	}

	/** Format a finish timestamp as a short time string. */
	function finishedLabel(ts: number | undefined): string {
		if (!ts) return "";
		return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
	}

	/** Action kind → human readable short label. */
	function actionLabel(card: BoardCard): string {
		const a = card.task.action;
		if (!a) return card.bot.pending_approval ? "Wants approval" : "Needs input";
		switch (a.kind) {
			case "input":
				return a.label;
			case "choose":
				return a.question;
			case "connect_github":
				return "Connect GitHub";
			case "provide_secret":
				return a.label;
			case "sign_in_engine":
				return `Sign in to ${a.engine}`;
			case "add_trigger":
				return "Add trigger";
			default:
				return "Needs input";
		}
	}

	const FLIP_MS = reducedMotion ? 0 : 200;
</script>

<!-- Full window now (no bot list beside it): a drag strip on top keeps the window movable and clear of the traffic
     lights, which sat over the first column. -->
<div class="board-wrap">
	<div class="board-drag" data-tauri-drag-region aria-hidden="true"></div>

	<!-- ─── filter toolbar ──────────────────────────────────────────────── -->
	<div class="board-toolbar">
		{#if availableBots.length > 1}
			<div class="toolbar-bots" role="group" aria-label="Filter by bot">
				{#each availableBots as { hostKey, bot } (`${hostKey}\u0000${bot.id}`)}
					{@const key = `${hostKey}\u0000${bot.id}`}
					<button
						class="bot-chip"
						class:bot-chip--on={selectedBots.has(key)}
						onclick={() => toggleBot(key)}
						aria-pressed={selectedBots.has(key)}
					>
						<BotFace seed={bot.id} state={bot.state} size={16} look={bot.look} />
						<span class="bot-chip-name">{bot.name}</span>
					</button>
				{/each}
			</div>
		{/if}

		<div class="toolbar-right">
			<div class="seg" role="group" aria-label="Source filter">
				{#each [["all", "All"], ["chat", "Chats"], ["auto", "Automations"]] as [val, label] (val)}
					<button
						class="seg-btn"
						class:seg-btn--on={source === val}
						onclick={() => (source = val as "all" | "chat" | "auto")}
						aria-pressed={source === val}>{label}</button
					>
				{/each}
			</div>

			<input
				class="toolbar-search"
				type="search"
				placeholder="Search…"
				bind:value={query}
				aria-label="Search tasks"
			/>

			<label class="show-done">
				<input type="checkbox" bind:checked={showDone} />
				Show done
			</label>

			{#if showDone}
				<div class="seg" role="group" aria-label="Done range">
					<button
						class="seg-btn"
						class:seg-btn--on={doneDays === 1}
						onclick={() => (doneDays = 1)}
						aria-pressed={doneDays === 1}>Today</button
					>
					<button
						class="seg-btn"
						class:seg-btn--on={doneDays === 7}
						onclick={() => (doneDays = 7)}
						aria-pressed={doneDays === 7}>7 days</button
					>
				</div>
			{/if}
		</div>
	</div>

	<!-- The columns always stay (each says "Nothing here"); replacing the whole board with this line looked broken,
	     e.g. after turning off Show done. -->
	{#if showGlobalEmpty}
		<div class="board-empty">
			No tasks match these filters. <button class="empty-clear" onclick={clearFilters}>Clear filters</button>
		</div>
	{/if}
	<div class="board" aria-label="Crew board">
		{#each columns as col (col.id)}
			<div class="column" data-col={col.id}>
				<div class="col-header">
					<span class="col-label">{col.label}</span>
					{#if col.cards.length > 0}
						<span class="col-count" aria-label="{col.cards.length} tasks">{col.cards.length}</span>
					{/if}
				</div>

				<div class="col-cards" role="list" aria-label="{col.label} tasks">
					{#if col.cards.length === 0}
						<p class="col-empty" aria-live="polite">Nothing here</p>
					{/if}

					{#each col.cards as card (card.task.id)}
						<button
							class="card"
							class:card--failed={card.failed}
							class:card--cancelled={card.cancelled}
							aria-label="{card.bot.name}: {card.title ?? card.task.prompt}"
							onclick={() => openCard(card)}
							animate:flip={{ duration: FLIP_MS }}
						>
							<div class="card-head">
								<BotFace seed={card.bot.id} state={card.bot.state} size={22} look={card.bot.look} />
								<span class="card-name">{card.bot.name}</span>

								{#if col.id === "working"}
									<span class="card-elapsed">{elapsed(card.task.started_at, now)}</span>
								{/if}

								{#if col.id === "done"}
									<span class="card-time">{finishedLabel(card.task.finished_at)}</span>
									{#if card.failed}
										<span class="badge badge--error">Failed</span>
									{:else if card.cancelled}
										<span class="badge badge--muted">Cancelled</span>
									{/if}
								{/if}
							</div>

							<!-- The work as the agent named it; the message only until it has a name. -->
							<p class="card-prompt">{card.title ?? card.task.prompt}</p>

							{#if col.id === "working"}
								<p class="card-step">
									{workingLabel(card.task.steps?.at(-1)?.text, card.task.steps?.at(-1)?.ts, now)}
								</p>
							{/if}

							{#if col.id === "needs_you"}
								<p class="card-action">{actionLabel(card)}</p>
							{/if}
						</button>
					{/each}
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.board-wrap {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		background: var(--ca-surface-base);
	}
	.board-drag {
		flex: none;
		height: 38px;
	}
	.board {
		display: flex;
		flex: 1;
		min-height: 0;
		gap: 0;
		overflow: hidden;
	}

	/* ─── Toolbar ─────────────────────────────────────────────── */
	.board-toolbar {
		flex: none;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		min-height: 0;
	}

	.toolbar-bots {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		flex: 1;
		min-width: 0;
	}

	.bot-chip {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 3px 8px 3px 5px;
		border-radius: 20px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		cursor: default;
		font-size: 11.5px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		transition:
			background 100ms ease,
			color 100ms ease;
		white-space: nowrap;
	}

	.bot-chip:hover {
		background: var(--ca-vibrancy-fill);
	}

	.bot-chip--on {
		background: color-mix(in oklab, var(--ca-accent) 15%, transparent);
		border-color: color-mix(in oklab, var(--ca-accent) 40%, transparent);
		color: var(--ca-text-primary);
	}

	.bot-chip-name {
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 80px;
	}

	.toolbar-right {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.seg {
		display: flex;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 6px;
		overflow: hidden;
	}

	.seg-btn {
		padding: 3px 8px;
		font-size: 11.5px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		background: transparent;
		cursor: default;
		border: none;
		border-left: 0.5px solid var(--ca-outline-subtle);
		white-space: nowrap;
		transition:
			background 100ms ease,
			color 100ms ease;
	}

	.seg-btn:first-child {
		border-left: none;
	}

	.seg-btn:hover {
		background: var(--ca-vibrancy-fill);
	}

	.seg-btn--on {
		background: color-mix(in oklab, var(--ca-accent) 15%, transparent);
		color: var(--ca-text-primary);
	}

	.toolbar-search {
		height: 24px;
		padding: 0 7px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		font-size: 11.5px;
		color: var(--ca-text-primary);
		width: 130px;
		outline: none;
	}

	.toolbar-search:focus {
		border-color: var(--ca-accent, var(--ca-outline-subtle));
	}

	.toolbar-search::placeholder {
		color: var(--ca-text-disabled);
	}

	.show-done {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11.5px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		cursor: default;
		white-space: nowrap;
		user-select: none;
	}

	.show-done input[type="checkbox"] {
		cursor: default;
	}

	/* ─── Global empty state ──────────────────────────────────── */
	.board-empty {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 8px;
		font-size: 12px;
		color: var(--ca-text-disabled);
		gap: 6px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}

	.empty-clear {
		font-size: 12px;
		color: var(--ca-accent, var(--ca-text-secondary));
		background: none;
		border: none;
		padding: 0;
		cursor: default;
		text-decoration: underline;
	}

	/* ─── Column ──────────────────────────────────────────────── */
	.column {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		border-right: 0.5px solid var(--ca-outline-subtle);
	}

	.column:last-child {
		border-right: none;
	}

	.col-header {
		flex: none;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 14px 12px 8px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}

	.col-label {
		font-size: 11.5px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}

	.col-count {
		font-size: 11px;
		font-weight: 600;
		min-width: 18px;
		height: 18px;
		border-radius: 9px;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-secondary);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 4px;
	}

	/* Needs you column: accent the count */
	.column[data-col="needs_you"] .col-count {
		background: color-mix(in oklab, var(--ca-semantic-error) 15%, transparent);
		color: var(--ca-semantic-error);
	}

	.col-cards {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.col-empty {
		font-size: 12px;
		color: var(--ca-text-disabled);
		text-align: center;
		padding: 24px 8px;
		margin: 0;
	}

	/* ─── Card ────────────────────────────────────────────────── */
	.card {
		display: flex;
		flex-direction: column;
		gap: 5px;
		padding: 9px 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		background: var(--ca-surface-raised, var(--ca-surface-sunken));
		text-align: left;
		cursor: default;
		width: 100%;
		transition:
			background 120ms ease,
			border-color 120ms ease;
	}

	.card:hover {
		background: var(--ca-vibrancy-fill);
		border-color: var(--ca-outline-subtle);
	}

	.card--failed {
		border-color: color-mix(in oklab, var(--ca-semantic-error) 40%, transparent);
	}

	.card--cancelled {
		opacity: 0.55;
	}

	.card-head {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}

	.card-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
		min-width: 0;
	}

	.card-elapsed {
		font-size: 10.5px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		flex: none;
		font-variant-numeric: tabular-nums;
	}

	.card-time {
		font-size: 10.5px;
		color: var(--ca-text-disabled);
		flex: none;
		font-variant-numeric: tabular-nums;
	}

	.badge {
		font-size: 10px;
		font-weight: 600;
		padding: 1px 5px;
		border-radius: 4px;
		flex: none;
	}

	.badge--error {
		background: color-mix(in oklab, var(--ca-semantic-error) 15%, transparent);
		color: var(--ca-semantic-error);
	}

	.badge--muted {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-disabled);
	}

	.card-prompt {
		font-size: 12px;
		color: var(--ca-text-primary);
		margin: 0;
		/* 2-line clamp */
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		overflow: hidden;
		line-height: 1.45;
	}

	.card-step {
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card-action {
		font-size: 11px;
		color: var(--ca-semantic-warning, var(--ca-text-secondary));
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Needs you column: give cards a subtle amber tint */
	.column[data-col="needs_you"] .card:not(.card--failed) {
		border-color: color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
	}
</style>
