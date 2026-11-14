<script lang="ts">
	/** BoardToolbar — the board's filters: crewmate chips, source, search, and the done column's range. */
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { crewmateKey } from "#lib/crew/crewmateKey.ts";
	import type { BoardFilters, BoardSource } from "#lib/crew/board/boardFilters.svelte.ts";

	let { filters }: { filters: BoardFilters } = $props();

	const SOURCES: [BoardSource, string][] = [
		["all", "All"],
		["chat", "Chats"],
		["auto", "Automations"],
	];
</script>

<div class="board-toolbar">
	{#if filters.availableCrewmates.length > 1}
		<div class="toolbar-crewmates" role="group" aria-label="Filter by crewmate">
			{#each filters.availableCrewmates as { hostKey, crewmate } (crewmateKey(hostKey, crewmate.id))}
				{@const key = crewmateKey(hostKey, crewmate.id)}
				<button
					class="crewmate-chip"
					class:crewmate-chip--on={filters.selectedCrewmates.has(key)}
					onclick={() => filters.toggleCrewmate(key)}
					aria-pressed={filters.selectedCrewmates.has(key)}
				>
					<CrewmateFace seed={crewmate.id} state={crewmate.state} size={16} look={crewmate.look} />
					<span class="crewmate-chip-name">{crewmate.name}</span>
				</button>
			{/each}
		</div>
	{/if}

	<div class="toolbar-right">
		<div class="seg" role="group" aria-label="Source filter">
			{#each SOURCES as [val, label] (val)}
				<button
					class="seg-btn"
					class:seg-btn--on={filters.source === val}
					onclick={() => (filters.source = val)}
					aria-pressed={filters.source === val}>{label}</button
				>
			{/each}
		</div>

		<input
			class="toolbar-search"
			type="search"
			placeholder="Search…"
			bind:value={filters.query}
			aria-label="Search tasks"
		/>

		<label class="show-done">
			<input type="checkbox" bind:checked={filters.showDone} />
			Show done
		</label>

		{#if filters.showDone}
			<div class="seg" role="group" aria-label="Done range">
				<button
					class="seg-btn"
					class:seg-btn--on={filters.doneDays === 1}
					onclick={() => (filters.doneDays = 1)}
					aria-pressed={filters.doneDays === 1}>Today</button
				>
				<button
					class="seg-btn"
					class:seg-btn--on={filters.doneDays === 7}
					onclick={() => (filters.doneDays = 7)}
					aria-pressed={filters.doneDays === 7}>7 days</button
				>
			</div>
		{/if}
	</div>
</div>

<style>
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

	.toolbar-crewmates {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		flex: 1;
		min-width: 0;
	}

	.crewmate-chip {
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

	.crewmate-chip:hover {
		background: var(--ca-vibrancy-fill);
	}

	.crewmate-chip--on {
		background: color-mix(in oklab, var(--ca-accent) 15%, transparent);
		border-color: color-mix(in oklab, var(--ca-accent) 40%, transparent);
		color: var(--ca-text-primary);
	}

	.crewmate-chip-name {
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
</style>
