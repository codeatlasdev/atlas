<script lang="ts">
	/**
	 * CrewBoard — Kanban view of all crewmates' tasks across every reachable host.
	 * Four columns: Queued · Working · Needs you · Done (today only).
	 * Replaces the right panel (CrewmateView) when crewView === 'board'.
	 */
	import { onMount } from "svelte";
	import { flip } from "svelte/animate";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { crewView } from "#lib/crew/crewView.svelte.ts";
	import { boardColumns } from "#lib/crew/board.ts";
	import { reducedMotion } from "#lib/crew/motion.ts";
	import { BoardFilters } from "#lib/crew/board/boardFilters.svelte.ts";
	import BoardToolbar from "#lib/crew/board/BoardToolbar.svelte";
	import BoardCard from "#lib/crew/board/BoardCard.svelte";
	import type { BoardCard as Card } from "#lib/crew/board.ts";

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
		crew.boardOpen = true;
		if (crew.boardEntries.length === 0) void crew.loadBoardTasks();
		return () => {
			crew.boardOpen = false;
		};
	});

	const filters = new BoardFilters(() => boardColumns(crew.boardEntries, now));
	$effect(() => filters.save());

	const allCols = $derived(boardColumns(crew.boardEntries, now, filters.options));
	const columns = $derived(filters.showDone ? allCols : allCols.filter((c) => c.id !== "done"));
	const showGlobalEmpty = $derived(columns.every((c) => c.cards.length === 0) && filters.hasActive);

	/** Open the chat at this task, not just the crewmate: the conversation scrolls to it and flashes it. */
	function openCard(card: Card) {
		crew.focusTask = card.task.id;
		void crew.select(card.hostKey, card.crewmate.id);
		crewView.value = "chat";
	}

	const FLIP_MS = reducedMotion ? 0 : 200;
</script>

<!-- Full window now (no crewmate list beside it): a drag strip on top keeps the window movable and clear of the traffic
     lights, which sat over the first column. -->
<div class="board-wrap">
	<div class="board-drag" data-tauri-drag-region aria-hidden="true"></div>

	<BoardToolbar {filters} />

	<!-- The columns always stay (each says "Nothing here"); replacing the whole board with this line looked broken,
	     e.g. after turning off Show done. -->
	{#if showGlobalEmpty}
		<div class="board-empty">
			No tasks match these filters. <button class="empty-clear" onclick={() => filters.clear()}
				>Clear filters</button
			>
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
						<div animate:flip={{ duration: FLIP_MS }}>
							<BoardCard {card} column={col.id} {now} onopen={() => openCard(card)} />
						</div>
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
</style>
