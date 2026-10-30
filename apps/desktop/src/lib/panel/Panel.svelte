<script lang="ts">
	import { useNow } from "#lib/clock.svelte.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { Button, EmptyState } from "@codeatlasdev/ui";
	import { FileCog } from "@lucide/svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import Charts from "./Charts.svelte";
	import PanelHeader from "./PanelHeader.svelte";
	import PanelKpis from "./PanelKpis.svelte";
	import QuickLinks from "./QuickLinks.svelte";
	import ServiceGrid from "./ServiceGrid.svelte";
	import StartInvite from "./StartInvite.svelte";
	import StateTimeline from "./StateTimeline.svelte";
	import { hex } from "./color.ts";
	import { metrics } from "./metrics.svelte.ts";
	import { lastLines, serviceRows } from "./overview.ts";

	let { project }: { project: ProjectView } = $props();

	let live = $derived(store.live[project.id]);
	let cfg = $derived(store.configs[project.id]);
	let root = $state<HTMLElement | null>(null);
	const clk = useNow(15_000);

	let quick = $derived((cfg?.services ?? []).filter((s) => s.target?.how === "browser"));
	let services = $derived(serviceRows(cfg?.services ?? [], live?.services, project.running));
	let names = $derived(services.map((s) => s.name));
	let upNames = $derived(new Set(live?.services.filter((s) => s.state === "running").map((s) => s.name)));

	// ── KPIs: recomputed once per published metrics revision (≤1/s) ──
	let kpi = $derived.by(() => {
		void metrics.rev;
		const t = metrics.totals(project.id);
		return {
			mem: t.mem,
			cpu: t.cpu,
			restarts: metrics.restartsLastHour(project.id, clk.now),
			errors: metrics.errorsLast(project.id, 5 * 60_000),
		};
	});
	let upCount = $derived(services.filter((s) => s.state === "running").length);
	let failedCount = $derived(services.filter((s) => s.state === "failed").length);

	// Last log line per service, read once per log revision (the ring is append-only).
	let tickers = $derived.by(() => {
		void live?.logRev;
		return lastLines(store.logs(project.id).items, names);
	});

	// Segments mutate in place (no reactivity inside the history): copy per revision so the
	// timeline sees new objects, and use the sample time as its right edge so bands advance.
	let timelineData = $derived.by(() => {
		void metrics.rev;
		return {
			end: Date.now(),
			lanes: names.map((name) => ({
				name,
				segments: metrics.service(project.id, name).segments.map((s) => ({ ...s })),
			})),
		};
	});
	let bands = $derived(timelineData.lanes.some((l) => l.segments.some((s) => s.band !== "stopped")));

	// Canvas-safe colors for the sparklines, resolved once per theme/space.
	let colors = $derived.by(() => {
		const key = `${themes.current?.name}:${project.id}`;
		const scope = root ?? document.body;
		return {
			cpu: hex("var(--space-accent, var(--ca-brand-main))", key, scope),
			mem: hex("var(--ca-semantic-info)", key, scope),
		};
	});

	const space = useSpace();

	function showLogs(name: string) {
		store.focusedService = name;
		const w = space.worktree;
		if (w) ws.reveal(w.id, (t) => t.kind === "logs", { kind: "logs", label: "Logs" });
	}

	function openConfig() {
		const w = space.worktree;
		if (w) ws.reveal(w.id, (t) => t.kind === "config", { kind: "config", label: "Config" });
	}

	let stackBusy = $derived(!!store.busy[`stack:${project.id}`]);
	let uptime = $derived.by(() => {
		const since = metrics.upSince(project.id);
		return since ? Math.max(0, clk.now - since) : null;
	});
</script>

<div class="scroll" bind:this={root}>
	<PanelHeader {project} {cfg} {live} {uptime} />

	{#if services.length === 0}
		<div class="hero">
			<EmptyState
				icon={FileCog}
				title="No services yet"
				description="Atlas found nothing to run here. Declare services in atlas.yaml and it starts, watches and charts them."
			>
				{#snippet actions()}
					<Button variant="glass" onclick={openConfig}>Edit atlas.yaml</Button>
				{/snippet}
			</EmptyState>
		</div>
	{:else}
		<PanelKpis
			running={project.running}
			up={upCount}
			total={services.length}
			failed={failedCount}
			restarts={kpi.restarts}
			errors={kpi.errors}
			mem={kpi.mem}
			cpu={kpi.cpu}
		/>

		{#if !project.running}
			<StartInvite
				{names}
				startable={services.filter((s) => s.state !== "disabled").length}
				busy={stackBusy}
				disabled={!!project.error}
				onstart={() => store.run(`stack:${project.id}`, () => api.start(project.id))}
			/>
		{/if}

		{#if quick.length}
			<QuickLinks services={quick} running={project.running} {upNames} />
		{/if}

		<section>
			<h2>Services <span class="n">{services.length}</span></h2>
			<ServiceGrid
				projectId={project.id}
				{services}
				running={project.running}
				{tickers}
				{colors}
				onlogs={showLogs}
			/>
		</section>

		{#if project.running && bands}
			<div class="cols2">
				<section>
					<h2>Resources</h2>
					<div class="box"><Charts projectId={project.id} {names} /></div>
				</section>
				<section>
					<h2>Health over time</h2>
					<div class="box">
						<StateTimeline lanes={timelineData.lanes} end={timelineData.end} onpick={showLogs} />
					</div>
				</section>
			</div>
		{/if}
	{/if}
</div>

<style>
	.scroll {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		padding: 16px 20px 32px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}
	h2 {
		display: flex;
		align-items: baseline;
		gap: 6px;
		font-size: 13px;
		font-weight: 650;
		margin: 0 0 8px;
	}
	.n {
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-text-secondary);
	}
	.hero {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 260px;
	}
	.cols2 {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
		gap: 20px;
		align-items: start;
	}
	.box {
		border-radius: 12px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
		padding: 12px 14px;
		min-width: 0;
	}
</style>
