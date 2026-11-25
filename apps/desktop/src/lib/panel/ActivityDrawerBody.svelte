<script lang="ts">
	import { useNow, usePoll } from "#lib/clock.svelte.ts";
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
	import { active, describe, summary } from "./heavy.ts";
	import { heavy } from "./heavy.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { projectId }: { projectId: string } = $props();
	const space = useSpace();

	const t = useNow(15_000);
	// The machine-wide heavy-job queue (atlas run --heavy): polled only while the drawer is open.
	usePoll(
		3000,
		() => activity.open,
		() => void heavy.refresh(),
	);
	let rows = $derived(fold(store.live[projectId]?.events ?? []));
	let tally = $derived(counts(rows));
	let groups = $derived(group(filterRows(rows, activity.filter), t.now));

	let items = $derived(
		(
			[
				["all", m.panel_activity_filter_all()],
				["errors", m.panel_kpis_errors()],
				["restarts", m.panel_kpis_restarts()],
				["actions", m.panel_ops_actions()],
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
		ws.reveal(w.id, (t) => t.kind === "logs", { kind: "logs", label: m.panel_tab_logs() });
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

<DrawerShell bind:open={activity.open} title={m.panel_activity_drawer_activity()} width="400px">
	{#if active(heavy.status)}
		<section class="heavy" aria-label={m.panel_activity_drawer_heavy_jobs()}>
			<div class="gl">{m.panel_activity_heavy_jobs_summary({ summary: summary(heavy.status) })}</div>
			{#each [...heavy.status.running, ...heavy.status.queued] as j (j.id)}
				{@const d = describe(j)}
				<DrawerRow
					tone={j.state === "running" ? "run" : "off"}
					pulse={j.state === "running"}
					disabled
					title={j.command}
				>
					<span class="title">{d.title}</span>
					<span class="detail">{d.detail}</span>
				</DrawerRow>
			{/each}
		</section>
	{/if}
	<ToggleGroup
		type="single"
		size="sm"
		{items}
		value={activity.filter}
		aria-label={m.panel_activity_drawer_filter_activity()}
		onchange={(v) => (activity.filter = (v || "all") as Filter)}
	/>
	{#if groups.length === 0}
		<div class="empty">
			<EmptyState
				icon={Activity}
				title={rows.length ? m.panel_activity_nothing_matches() : m.panel_activity_no_activity_yet()}
				description={rows.length ? m.panel_activity_try_another_filter() : m.panel_activity_empty_hint()}
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
						title={go
							? m.panel_activity_open_service_logs({ service: r.service ?? "", time: clock(r.from) })
							: undefined}
					>
						<span class="title">{r.title}</span>
						{#if r.count > 1}
							<span
								class="count"
								title={m.panel_activity_events_over({ count: r.count, span: span(r.from, r.ts) })}
							>
								×{r.count}
							</span>
						{/if}
						{#if r.detail}<span class="detail" title={r.detail}>{r.detail}</span>{/if}
						<span class="time mono">{clock(r.ts)}</span>
					</DrawerRow>
				{/each}
			{/each}
		</div>
	{/if}
</DrawerShell>

<style>
	.heavy {
		margin: 0 -8px;
		padding: 0 8px;
	}
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
		/* The detail gives way first: the title names the service and what happened. */
		flex: 0 8 auto;
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
