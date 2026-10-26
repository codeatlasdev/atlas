<script lang="ts">
	import { useNow } from "#lib/clock.svelte.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { EmptyState, ToggleGroup } from "@codeatlasdev/ui";
	import { Activity } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { activity } from "./activity.svelte.ts";
	import DrawerRow from "./DrawerRow.svelte";
	import DrawerShell from "./DrawerShell.svelte";
	import { clock, counts, filterRows, fold, group, span, type Filter, type Row } from "./activity.ts";

	let { projectId }: { projectId: string } = $props();
	const space = useSpace();

	const t = useNow(15_000);
	let rows = $derived(fold(store.live[projectId]?.events ?? []));
	let tally = $derived(counts(rows));
	let groups = $derived(group(filterRows(rows, activity.filter), t.now));

	let items = $derived(
		(
			[
				["all", "All"],
				["errors", "Errors"],
				["restarts", "Restarts"],
				["actions", "Actions"],
			] as const
		).map(([value, label]) => ({ value, label: tally[value] ? `${label} ${tally[value]}` : label })),
	);

	/** Rows of a service open its logs at that moment; action/tunnel rows have no service. */
	function open(r: Row) {
		if (r.type === "action" || !r.service) return;
		const w = space.worktree;
		if (!w) return;
		store.focusedService = r.service;
		activity.seek = { project: projectId, service: r.service, ts: r.from };
		ws.reveal(w.id, (t) => t.kind === "logs", { kind: "logs", label: "Logs" });
		activity.open = false;
	}

	// Opening marks everything seen; keep the relative buckets fresh while it is open.
	$effect(() => {
		if (!activity.open) return;
		activity.markSeen(projectId);
		return () => activity.markSeen(projectId);
	});

	onMount(() => () => (activity.open = false));
</script>

<DrawerShell bind:open={activity.open} title="Activity" width="400px">
	<ToggleGroup
		type="single"
		size="sm"
		{items}
		value={activity.filter}
		aria-label="Filter activity"
		onchange={(v) => (activity.filter = (v || "all") as Filter)}
	/>
	{#if groups.length === 0}
		<div class="empty">
			<EmptyState
				icon={Activity}
				title={rows.length ? "Nothing matches" : "No activity yet"}
				description={rows.length
					? "Try another filter."
					: "Service state changes, alerts and actions show up here."}
			/>
		</div>
	{:else}
		<div class="list">
			{#each groups as g (g.label)}
				<div class="gl">{g.label}</div>
				{#each g.rows as r (r.id)}
					{@const go = r.type !== "action" && !!r.service}
					<DrawerRow
						tone={r.tone}
						disabled={!go}
						onclick={() => open(r)}
						title={go ? `Open ${r.service} logs at ${clock(r.from)}` : undefined}
					>
						<span class="title">{r.title}</span>
						{#if r.count > 1}
							<span class="count" title="{r.count} events over {span(r.from, r.ts)}">×{r.count}</span>
						{/if}
						{#if r.detail}<span class="detail">{r.detail}</span>{/if}
						<span class="time mono">{clock(r.ts)}</span>
					</DrawerRow>
				{/each}
			{/each}
		</div>
	{/if}
</DrawerShell>

<style>
	.list {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		margin: 0 -8px;
		padding: 0 8px 8px;
	}
	.gl {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 8px 6px 3px;
		font-size: 10.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
		background: var(--ca-surface-overlay);
	}
	.title {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.count,
	.detail {
		flex: none;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.detail {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.time {
		margin-left: auto;
		flex: none;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
	}
</style>
