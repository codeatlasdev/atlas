<script lang="ts">
	import { Tooltip } from "@codeatlasdev/ui";
	import { Sparkline } from "@codeatlasdev/ui/charts";
	import {
		RotateCw,
		Square,
		Play,
		ScrollText,
		SquareTerminal,
		ExternalLink,
		Copy,
		Globe,
		Braces,
		Database,
		MemoryStick,
		Cog,
		ListChecks,
		Server,
		TriangleAlert,
		CornerDownRight,
	} from "@lucide/svelte";
	import type { ServiceDef, ServiceKind } from "#lib/api.ts";
	import type { Service } from "#lib/stores/projects.svelte.ts";
	import { fmtMem } from "#lib/format.ts";
	import StatusRing from "#lib/components/StatusRing.svelte";
	import { metrics } from "./metrics.svelte.ts";
	const KIND_ICON = {
		web: Globe,
		api: Braces,
		database: Database,
		cache: MemoryStick,
		worker: Cog,
		task: ListChecks,
	} satisfies Record<ServiceKind, unknown>;

	let {
		projectId,
		service,
		def,
		running,
		busy,
		focused,
		ticker,
		colors,
		onfocus,
		onlogs,
		onterminal,
		onaction,
		onopen,
		onkeynav,
	}: {
		projectId: string;
		service: Service;
		def: ServiceDef | undefined;
		/** The stack is up: lifecycle buttons are available. */
		running: boolean;
		busy: boolean;
		focused: boolean;
		/** Last log line of this service (already plain text), or null. */
		ticker: string | null;
		/** Canvas-safe `#rrggbb` colors for the sparklines (resolved once by the grid). */
		colors: { cpu: string; mem: string };
		onfocus: () => void;
		onlogs: () => void;
		onterminal: () => void;
		onaction: (cmd: "start_service" | "stop_service" | "restart_service") => void;
		onopen: () => void;
		/** Arrow/Home/End navigation, handled by the grid. */
		onkeynav: (e: KeyboardEvent) => void;
	} = $props();

	const SPARK_POINTS = 40;

	let up = $derived(service.state === "running" || service.state === "starting");
	let failed = $derived(service.state === "failed");
	let K = $derived(def ? KIND_ICON[def.kind] : Server);
	// Re-read the rings once per published revision; the arrays are small (40 numbers).
	let cpu = $derived.by(() => {
		void metrics.rev;
		const h = metrics.service(projectId, service.name);
		return h.cpu.last(SPARK_POINTS, metrics.end(projectId));
	});
	let mem = $derived.by(() => {
		void metrics.rev;
		const h = metrics.service(projectId, service.name);
		return h.mem.last(SPARK_POINTS, metrics.end(projectId));
	});
	let showSpark = $derived(up && service.mem_mb != null);
	let reason = $derived(
		failed
			? (service.note ??
					(service.exit_code != null ? `Exited with code ${service.exit_code}` : "Failed to start"))
			: null,
	);

	function press(e: KeyboardEvent) {
		if (e.target !== e.currentTarget) return;
		if (e.key === "Enter") {
			e.preventDefault();
			onlogs();
		} else if ((e.key === "r" || e.key === "R") && running && up && !e.metaKey && !e.ctrlKey) {
			e.preventDefault();
			onaction("restart_service");
		} else onkeynav(e);
	}
</script>

<div
	class="card s-{service.state}"
	class:focused
	class:failed
	data-service={service.name}
	role="button"
	tabindex="0"
	aria-label="{service.name}, {service.state}"
	onclick={onfocus}
	ondblclick={onlogs}
	onkeydown={press}
	{onfocus}
>
	<div class="top">
		<span class="ring" class:pulse={service.state === "running"}>
			<StatusRing state={busy ? "starting" : service.state} />
		</span>
		<div class="id">
			<div class="name">
				<K size={13} class="kind" />
				<span class="n">{service.name}</span>
				{#if service.restarts}
					<span class="badge" title="Restarts since the stack started">↻ {service.restarts}</span>
				{/if}
			</div>
			<div class="meta">
				<span class="state state-{service.state}">{service.state}{service.completed_ok ? " ✓" : ""}</span>
				{#if service.paused_by}<span class="dim">paused by {service.paused_by}</span>{/if}
				{#if service.deselected}<span class="dim">not selected</span>{/if}
			</div>
		</div>
		{#if def?.target}
			{@const t = def.target}
			<button
				class="port mono"
				title={t.how === "browser" ? `Open ${t.url}` : `Copy ${t.url}`}
				onclick={(e) => {
					e.stopPropagation();
					onopen();
				}}
			>
				{#if def.port}:{def.port}{/if}
				{#if t.how === "browser"}<ExternalLink size={11} />{:else}<Copy size={11} />{/if}
			</button>
		{:else if def?.port}
			<span class="port plain mono">:{def.port}</span>
		{/if}
	</div>

	{#if reason}
		<div class="reason" title={reason}>
			<TriangleAlert size={13} />
			<span>{reason}</span>
			<button
				class="link"
				onclick={(e) => {
					e.stopPropagation();
					onlogs();
				}}>View logs</button
			>
		</div>
	{:else if showSpark}
		<div class="spark">
			<div class="sp" title="CPU of the process tree, last {Math.round((SPARK_POINTS * 3) / 60)} min">
				<Sparkline data={cpu} height="30px" color={colors.cpu} />
				<span class="val mono">{service.cpu ?? 0}%</span>
			</div>
			<div class="sp" title="Memory of the process tree, last {Math.round((SPARK_POINTS * 3) / 60)} min">
				<Sparkline data={mem} height="30px" color={colors.mem} />
				<span class="val mono" class:heavy={(service.mem_mb ?? 0) >= 2048}>{fmtMem(service.mem_mb ?? 0)}</span
				>
			</div>
		</div>
	{:else if def?.command}
		<div class="cmd mono" title={def.command}>{def.dir ? `${def.dir} $ ` : "$ "}{def.command}</div>
	{/if}

	{#if ticker && up}
		<div class="ticker mono" title={ticker}>{ticker}</div>
	{/if}

	{#if def?.depends_on.length}
		<div class="deps" title="Waits for {def.depends_on.join(', ')}">
			<CornerDownRight size={11} />
			{#each def.depends_on as d (d)}<span class="chip">{d}</span>{/each}
		</div>
	{/if}

	<div class="btns">
		{#if running}
			{#if up}
				<Tooltip content="Restart (R)"
					><button
						class="b"
						disabled={busy}
						aria-label="Restart"
						onclick={(e) => {
							e.stopPropagation();
							onaction("restart_service");
						}}><RotateCw size={13} /></button
					></Tooltip
				>
				<Tooltip content="Stop"
					><button
						class="b"
						disabled={busy}
						aria-label="Stop"
						onclick={(e) => {
							e.stopPropagation();
							onaction("stop_service");
						}}><Square size={12} /></button
					></Tooltip
				>
			{:else}
				<Tooltip content="Start"
					><button
						class="b"
						disabled={busy}
						aria-label="Start"
						onclick={(e) => {
							e.stopPropagation();
							onaction("start_service");
						}}><Play size={13} /></button
					></Tooltip
				>
			{/if}
		{/if}
		<Tooltip content="Logs (Enter)"
			><button
				class="b"
				aria-label="Logs"
				onclick={(e) => {
					e.stopPropagation();
					onlogs();
				}}><ScrollText size={13} /></button
			></Tooltip
		>
		<Tooltip content="Terminal in {def?.dir ?? 'project root'}"
			><button
				class="b"
				aria-label="Terminal"
				onclick={(e) => {
					e.stopPropagation();
					onterminal();
				}}><SquareTerminal size={13} /></button
			></Tooltip
		>
	</div>
</div>

<style>
	.card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 12px 10px;
		border-radius: 12px;
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			var(--ca-shadow-xs);
		outline: none;
		min-width: 0;
		transition:
			box-shadow 200ms cubic-bezier(0.2, 0.8, 0.2, 1),
			transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1),
			background 200ms ease-out;
	}
	.card:hover {
		transform: translateY(-1px);
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 45%, var(--ca-outline-subtle)),
			var(--ca-shadow-sm, var(--ca-shadow-xs));
	}
	.card.focused {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 7%, var(--ca-surface-raised));
		box-shadow: 0 0 0 1px color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 60%, transparent);
	}
	:global([data-kbd]) .card:focus-visible {
		box-shadow: 0 0 0 2px var(--space-accent, var(--ca-brand-main));
	}
	.card.failed {
		background: color-mix(in oklab, var(--ca-semantic-error) 6%, var(--ca-surface-raised));
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-error) 45%, transparent);
	}
	.s-stopped,
	.s-disabled {
		opacity: 0.78;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}
	/* A soft halo behind the ring while the service is up: the one looping animation here. */
	.ring {
		position: relative;
		display: grid;
		flex: none;
	}
	.ring.pulse::after {
		content: "";
		position: absolute;
		inset: 4px;
		border-radius: 50%;
		background: var(--ca-semantic-success);
		opacity: 0;
		animation: halo 2.4s ease-out infinite;
		pointer-events: none;
	}
	@keyframes halo {
		0% {
			opacity: 0.35;
			transform: scale(0.7);
		}
		70%,
		100% {
			opacity: 0;
			transform: scale(1.5);
		}
	}
	.id {
		min-width: 0;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.name {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}
	.name :global(.kind) {
		color: var(--ca-text-secondary);
		flex: none;
	}
	.n {
		font-weight: 650;
		font-size: 13.5px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		flex: none;
		font-size: 10.5px;
		padding: 0 5px;
		border-radius: 8px;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 14%, transparent);
	}
	.meta {
		display: flex;
		gap: 8px;
		font-size: 11.5px;
	}
	.state {
		color: var(--ca-text-secondary);
	}
	.state-running {
		color: var(--ca-semantic-success);
	}
	.state-failed {
		color: var(--ca-semantic-error);
	}
	.state-starting {
		color: var(--ca-semantic-warning);
	}
	.dim {
		color: var(--ca-text-disabled);
	}
	.port {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 6px;
		border: 0;
		border-radius: 6px;
		background: none;
		font-size: 11.5px;
		color: var(--space-accent, var(--ca-brand-main));
		transition: background 120ms ease-out;
	}
	.port:hover {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 14%, transparent);
	}
	.port.plain {
		color: var(--ca-text-secondary);
	}
	.reason {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		border-radius: 8px;
		font-size: 12px;
		color: var(--ca-semantic-error);
		background: color-mix(in oklab, var(--ca-semantic-error) 10%, transparent);
	}
	.reason span {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.link {
		flex: none;
		border: 0;
		background: none;
		padding: 2px 4px;
		font: inherit;
		font-weight: 600;
		color: inherit;
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.spark {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
	}
	.sp {
		position: relative;
		min-width: 0;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		overflow: hidden;
	}
	.val {
		position: absolute;
		right: 6px;
		top: 3px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.val.heavy {
		color: var(--ca-semantic-warning);
	}
	.cmd,
	.ticker {
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.cmd {
		color: var(--ca-text-disabled);
	}
	.ticker {
		color: var(--ca-text-secondary);
		padding-left: 8px;
		border-left: 2px solid color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 40%, transparent);
	}
	.deps {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px;
		color: var(--ca-text-disabled);
	}
	.chip {
		font-size: 10.5px;
		padding: 0 6px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
	}
	.btns {
		position: absolute;
		top: 8px;
		right: 8px;
		display: flex;
		gap: 2px;
		padding: 2px;
		border-radius: 8px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		opacity: 0;
		transform: translateY(-2px);
		pointer-events: none;
		transition:
			opacity 160ms ease-out,
			transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.card:hover .btns,
	.card.focused .btns,
	.card:focus-within .btns {
		opacity: 1;
		transform: none;
		pointer-events: auto;
	}
	.b {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.b:hover:not(:disabled) {
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
	}
	.b:disabled {
		opacity: 0.4;
	}
	@media (prefers-reduced-motion: reduce) {
		.card,
		.btns {
			transition: none;
		}
		.ring.pulse::after {
			animation: none;
		}
	}
</style>
