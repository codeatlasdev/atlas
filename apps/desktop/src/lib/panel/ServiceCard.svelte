<script lang="ts">
	import { CornerDownRight, TriangleAlert } from "@lucide/svelte";
	import type { ServiceDef } from "#lib/api.ts";
	import type { Service } from "#lib/stores/projects.svelte.ts";
	import CardHead from "./CardHead.svelte";
	import CardActions from "./CardActions.svelte";
	import CardSpark from "./CardSpark.svelte";
	import { isAlive, problemText } from "./serviceState.ts";

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

	let up = $derived(isAlive(service));
	let failed = $derived(service.state === "failed" || service.state === "unhealthy");
	let degraded = $derived(service.state === "degraded");
	let showSpark = $derived(up && service.mem_mb != null);
	let reason = $derived(problemText(service));

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
	class:degraded
	data-service={service.name}
	role="button"
	tabindex="0"
	aria-label="{service.name}, {service.state}"
	onclick={onfocus}
	ondblclick={onlogs}
	onkeydown={press}
	{onfocus}
>
	<CardHead {service} {def} {busy} {onopen} />

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
		{#if up && service.memory_leak}
			<div class="memnote" title={service.memory_leak}>{service.memory_leak}</div>
		{/if}
	{:else if showSpark}
		<CardSpark {projectId} {service} {colors} />
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

	<CardActions {running} {up} {busy} dir={def?.dir} {onaction} {onlogs} {onterminal} />
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
	.card.degraded {
		background: color-mix(in oklab, var(--ca-semantic-warning) 6%, var(--ca-surface-raised));
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-warning) 45%, transparent);
	}
	.card.degraded .reason {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, transparent);
	}
	/* A stopped service reads as quieter through its surface, not through `opacity`: fading the card multiplied
	   every text colour inside it (measured 2.1-4.4:1); the tokens keep their guaranteed contrast. */
	.s-stopped,
	.s-disabled {
		background: color-mix(in oklab, var(--ca-surface-sunken) 55%, var(--ca-surface-raised));
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
	.memnote {
		font-size: 11px;
		color: var(--ca-semantic-warning);
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
	.cmd,
	.ticker {
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.cmd {
		color: var(--ca-text-secondary);
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
		color: var(--ca-text-secondary);
	}
	.chip {
		font-size: 10.5px;
		padding: 0 6px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
	}
	@media (prefers-reduced-motion: reduce) {
		.card {
			transition: none;
		}
	}
</style>
