<script lang="ts">
	/** BoardCard — one task on the board; clicking it opens the chat at that task. */
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { workingLabel } from "#lib/crew/stepPhrase.ts";
	import { actionLabel, elapsed, finishedLabel } from "#lib/crew/board/boardText.ts";
	import type { BoardCard, ColumnId } from "#lib/crew/board.ts";

	let { card, column, now, onopen }: { card: BoardCard; column: ColumnId; now: number; onopen: () => void } =
		$props();
</script>

<button
	class="card"
	class:card--failed={card.failed}
	class:card--cancelled={card.cancelled}
	class:card--needs-you={column === "needs_you"}
	aria-label="{card.crewmate.name}: {card.title ?? card.task.prompt}"
	onclick={onopen}
>
	<div class="card-head">
		<CrewmateFace seed={card.crewmate.id} state={card.crewmate.state} size={22} look={card.crewmate.look} />
		<span class="card-name">{card.crewmate.name}</span>

		{#if column === "working"}
			<span class="card-elapsed">{elapsed(card.task.started_at, now)}</span>
		{/if}

		{#if column === "done"}
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

	{#if column === "working"}
		<p class="card-step">
			{workingLabel(card.task.steps?.at(-1)?.text, card.task.steps?.at(-1)?.ts, now)}
		</p>
	{/if}

	{#if column === "needs_you"}
		<p class="card-action">{actionLabel(card)}</p>
	{/if}
</button>

<style>
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

	/* Needs you column: give cards a subtle amber tint */
	.card--needs-you:not(.card--failed) {
		border-color: color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
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
</style>
