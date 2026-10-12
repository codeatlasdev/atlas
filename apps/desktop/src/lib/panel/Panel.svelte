<script lang="ts">
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import { Button, Kbd, Spinner, EmptyState } from "@codeatlasdev/ui";
	import {
		Network,
		Server,
		ExternalLink,
		Play,
		FileCog,
		Cpu,
		MemoryStick,
		RotateCw,
		TriangleAlert,
		Layers,
	} from "@lucide/svelte";
	import { flip } from "svelte/animate";
	import { onMount, tick } from "svelte";
	import { api, type ProjectView, type ServiceDef } from "#lib/api.ts";
	import { store, type Service } from "#lib/stores/projects.svelte.ts";
	import { openService } from "#lib/actions.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { fmtMem } from "#lib/format.ts";
	import SetupRow from "#lib/components/SetupRow.svelte";
	import ServiceCard from "./ServiceCard.svelte";
	import Charts from "./Charts.svelte";
	import StateTimeline from "./StateTimeline.svelte";
	import Kpi from "./Kpi.svelte";
	import { hex } from "./color.ts";
	import { metrics } from "./metrics.svelte.ts";
	import { fmtUptime } from "./metrics.ts";
	import { tilde } from "#lib/format.ts";
	import { byAttention, gridStep, shellIn } from "./panel.ts";

	let { project }: { project: ProjectView } = $props();

	let live = $derived(store.live[project.id]);
	let cfg = $derived(store.configs[project.id]);
	let root = $state<HTMLElement | null>(null);
	let grid = $state<HTMLElement | null>(null);
	let cols = $state(1);
	let now = $state(Date.now());

	let quick = $derived((cfg?.services ?? []).filter((s) => s.target?.how === "browser"));

	let services = $derived.by(() => {
		const defs: Record<string, ServiceDef> = Object.fromEntries(
			(cfg?.services ?? []).map((s) => [s.name, s]),
		);
		const rows =
			project.running && live?.services.length
				? live.services
				: (cfg?.services ?? []).map((s): Service => ({
						name: s.name,
						state: s.enabled ? "stopped" : "disabled",
						exit_code: null,
						restarts: 0,
						note: null,
					}));
		return byAttention(rows).map((s) => ({ ...s, def: defs[s.name] as ServiceDef | undefined }));
	});
	let names = $derived(services.map((s) => s.name));

	// ── KPIs: recomputed once per published metrics revision (≤1/s) ──
	let kpi = $derived.by(() => {
		void metrics.rev;
		const t = metrics.totals(project.id);
		return {
			mem: t.mem,
			cpu: t.cpu,
			restarts: metrics.restartsLastHour(project.id, now),
			errors: metrics.errorsLast(project.id, 5 * 60_000),
		};
	});
	let upCount = $derived(services.filter((s) => s.state === "running").length);
	let failedCount = $derived(services.filter((s) => s.state === "failed").length);

	// Last log line per service, read once per log revision (the ring is append-only).
	let tickers = $derived.by(() => {
		void live?.logRev;
		const ring = store.logs(project.id);
		const want = new Set(names);
		const out: Record<string, string> = {};
		for (let i = ring.items.length - 1; i >= 0 && want.size; i--) {
			const l = ring.items[i];
			if (!l.mark && want.delete(l.name)) out[l.name] = l.line;
		}
		return out;
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

	function svc(cmd: "start_service" | "stop_service" | "restart_service", name: string) {
		store.run(`svc:${project.id}:${name}`, () => api.service(project.id, cmd, name));
	}

	/** Restarting a running service interrupts it: ask first, the way R does from the keyboard. */
	async function act(
		cmd: "start_service" | "stop_service" | "restart_service",
		name: string,
		viaKey: boolean,
	) {
		if (cmd === "restart_service" && viaKey) {
			const ok = await confirmDialog(`Restart ${name}?`, {
				description: "The service stops and starts again. Connections in flight are dropped.",
				confirmLabel: "Restart",
			});
			if (!ok) return;
		}
		svc(cmd, name);
	}

	function showLogs(name: string) {
		store.focusedService = name;
		const w = ws.selected;
		if (w) ws.reveal(w.id, (t) => t.kind === "logs", { kind: "logs", label: "Logs" });
	}

	function terminal(def: ServiceDef | undefined) {
		const w = ws.selected;
		if (w) void ws.newTerminal(w.id, { argv: shellIn(def?.dir ?? null), label: def?.name ?? "Terminal" });
	}

	function openConfig() {
		const w = ws.selected;
		if (w) ws.reveal(w.id, (t) => t.kind === "config", { kind: "config", label: "Config" });
	}

	function startStack() {
		store.run(`stack:${project.id}`, () => api.start(project.id));
	}

	async function keynav(e: KeyboardEvent, i: number) {
		const to = gridStep(e.key, i, services.length, cols);
		if (to === i || to < 0) return;
		e.preventDefault();
		store.focusedService = services[to].name;
		await tick();
		grid?.querySelector<HTMLElement>(`[data-service="${CSS.escape(services[to].name)}"]`)?.focus();
	}

	onMount(() => {
		const clock = setInterval(() => (now = Date.now()), 15_000);
		return () => clearInterval(clock);
	});

	// The grid only exists once there are services: follow the element, not the mount.
	$effect(() => {
		const el = grid;
		if (!el) return;
		const ro = new ResizeObserver(() => {
			// The computed template lists one track per column: the browser resolved auto-fill for us.
			cols = Math.max(1, getComputedStyle(el).gridTemplateColumns.split(" ").length);
		});
		ro.observe(el);
		return () => ro.disconnect();
	});

	let stackBusy = $derived(!!store.busy[`stack:${project.id}`]);
	let uptime = $derived.by(() => {
		const since = metrics.upSince(project.id);
		return since ? Math.max(0, now - since) : null;
	});
	let ports = $derived((cfg?.services ?? []).flatMap((s) => (s.enabled && s.port ? [s.port] : [])));
	let modeLabel = $derived(live?.mode ?? project.mode ?? "running");
</script>

<div class="scroll" bind:this={root}>
	<div class="ident">
		<div class="title">
			<ProjectIcon id={project.parent ?? project.id} size={30} />
			<h1>{project.parent ? (ws.selected?.name ?? project.name) : project.name}</h1>
		</div>
		<div class="facts">
			{#if project.running}<span title="Time since the stack came up">Up {fmtUptime(uptime)}</span>{/if}
			{#if project.running && live?.mode}<span>{modeLabel}{live.version ? ` · v${live.version}` : ""}</span
				>{/if}
			{#if project.branch}<span class="mono"
					>{project.branch}{project.port_offset ? ` · ports +${project.port_offset}` : ""}</span
				>{/if}
			{#if ports.length}
				<span class="mono" title="Ports of the services">
					{ports
						.slice(0, 5)
						.map((x) => `:${x}`)
						.join(" ")}{ports.length > 5 ? ` +${ports.length - 5}` : ""}
				</span>
			{/if}
			<button
				class="root mono"
				title="Reveal in Finder"
				onclick={() => store.run("reveal", () => api.reveal(project.id))}
			>
				{tilde(project.root)}
			</button>
		</div>
	</div>
	{#if cfg?.setup.command}
		<SetupRow id={project.id} setup={cfg.setup} />
	{/if}

	{#if live?.tunnel || live?.operation}
		<div class="strip">
			{#if live?.operation}
				<div class="op">
					<Spinner size={13} />
					<b>{live.operation.label}</b>
					<span class="mono detail">{live.operation.detail}</span>
				</div>
			{/if}
			{#if live?.tunnel}
				<div class="op">
					<Network size={13} />
					Tunnel {live.tunnel.state}
					<span class="mono detail"
						>:{live.tunnel.local_port} → {live.tunnel.ssh_host}{live.tunnel.remote
							? `:${live.tunnel.remote}`
							: ""}</span
					>
				</div>
			{/if}
		</div>
	{/if}

	{#if services.length === 0}
		<div class="hero">
			<EmptyState
				icon={FileCog}
				title="No services yet"
				description="Declare services in atlas.yaml and Atlas starts, watches and charts them here."
			>
				{#snippet actions()}
					<Button variant="glass" onclick={openConfig}>Open atlas.yaml</Button>
				{/snippet}
			</EmptyState>
		</div>
	{:else}
		<div class="kpis" data-tour="panel-kpis">
			<Kpi
				icon={Layers}
				label="Services"
				value="{upCount}/{services.length}"
				hint={project.running ? "running" : "stopped"}
				tone={failedCount ? "fail" : project.running && upCount === services.length ? "ok" : "off"}
			/>
			<Kpi
				icon={RotateCw}
				label="Restarts"
				value={String(kpi.restarts)}
				hint="last hour"
				tone={kpi.restarts ? "warn" : "off"}
			/>
			<Kpi
				icon={TriangleAlert}
				label="Errors"
				value={String(kpi.errors)}
				hint="last 5 min"
				tone={kpi.errors ? "fail" : "off"}
			/>
			<Kpi
				icon={MemoryStick}
				label="Memory"
				value={project.running ? fmtMem(Math.round(kpi.mem)) : "—"}
				hint="all services"
				tone="off"
			/>
			<Kpi
				icon={Cpu}
				label="CPU"
				value={project.running ? `${Math.round(kpi.cpu)}%` : "—"}
				hint="of one core"
				tone="off"
			/>
		</div>

		{#if !project.running}
			<div class="invite">
				<div class="invite-text">
					<b>Start the stack</b>
					<span>
						Starts {services.filter((s) => s.state !== "disabled").length} service{services.length === 1
							? ""
							: "s"}:
						<span class="mono"
							>{names.slice(0, 4).join(", ")}{names.length > 4 ? `, +${names.length - 4}` : ""}</span
						>
					</span>
				</div>
				<Button disabled={stackBusy || !!project.error} onclick={startStack}>
					{#snippet leading()}{#if stackBusy}<Spinner size={13} />{:else}<Play
								size={12}
								fill="currentColor"
							/>{/if}{/snippet}
					Start
				</Button>
			</div>
		{/if}

		{#if quick.length}
			<div class="quick">
				{#each quick as q (q.name)}
					{@const up = live?.services.find((x) => x.name === q.name)?.state === "running"}
					<button
						class="qbtn"
						class:up
						disabled={!project.running}
						onclick={() => openService(q)}
						title={q.target?.url}
					>
						<span class="qname">{q.name}</span>
						<span class="qurl mono">{q.target?.url.replace(/^https?:\/\/(localhost)?/, "")}</span>
						{#if q.open_key}<Kbd>{q.open_key}</Kbd>{:else}<ExternalLink size={12} />{/if}
					</button>
				{/each}
			</div>
		{/if}

		<section>
			<h2>Services <span class="n">{services.length}</span></h2>
			<div
				class="grid"
				bind:this={grid}
				role="group"
				aria-label="Services"
				style:grid-template-columns="repeat(auto-fill, minmax(min(300px, 100%), 1fr))"
				data-tour="service-card"
			>
				{#each services as s, i (s.name)}
					<div animate:flip={{ duration: 220 }} class="cell">
						<ServiceCard
							projectId={project.id}
							service={s}
							def={s.def}
							running={project.running}
							busy={!!store.busy[`svc:${project.id}:${s.name}`]}
							focused={store.focusedService === s.name}
							ticker={tickers[s.name] ?? null}
							{colors}
							onfocus={() => (store.focusedService = s.name)}
							onlogs={() => showLogs(s.name)}
							onterminal={() => terminal(s.def)}
							onaction={(cmd) => act(cmd, s.name, cmd === "restart_service" && s.state === "running")}
							onopen={() => s.def && openService(s.def)}
							onkeynav={(e) => keynav(e, i)}
						/>
					</div>
				{/each}
			</div>
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
		color: var(--ca-text-disabled);
	}
	.hero {
		flex: 1;
		display: grid;
		place-items: center;
		min-height: 260px;
	}
	.strip {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.op {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		border-radius: 8px;
		font-size: 12px;
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 10%, transparent);
		box-shadow: inset 0 0 0 0.5px
			color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 30%, transparent);
	}
	.detail {
		color: var(--ca-text-secondary);
		max-width: 420px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.kpis {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 10px;
	}
	.invite {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 14px 16px;
		border-radius: 12px;
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 9%, var(--ca-surface-raised));
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 35%, transparent);
		animation: arrive 260ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.invite-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.invite-text b {
		font-size: 14px;
		color: var(--ca-text-primary);
	}
	.invite-text .mono {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ident {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		margin-bottom: -4px;
	}
	.title {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 20px;
		font-weight: 700;
		letter-spacing: -0.02em;
		line-height: 1.2;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--space-accent, var(--ca-text-primary));
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 12px;
		row-gap: 2px;
		min-width: 0;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.root {
		border: 0;
		background: transparent;
		color: var(--ca-text-secondary);
		font-size: 11.5px;
		padding: 1px 0;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.root:hover {
		color: var(--ca-text-primary);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
	}
	.quick {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.qbtn {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		padding: 0 10px;
		border-radius: 9px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
		color: var(--ca-text-primary);
		font: inherit;
		transition:
			border-color 120ms ease-out,
			background 120ms ease-out;
	}
	.qbtn:hover:not(:disabled) {
		border-color: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 60%, transparent);
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 6%, var(--ca-surface-raised));
	}
	.qbtn:disabled {
		opacity: 0.45;
	}
	.qname {
		font-weight: 600;
		font-size: 12.5px;
	}
	.qurl {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.grid {
		display: grid;
		gap: 12px;
	}
	.cell {
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.cell > :global(.card) {
		flex: 1;
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
	@media (prefers-reduced-motion: reduce) {
		.invite {
			animation: none;
		}
	}
</style>
