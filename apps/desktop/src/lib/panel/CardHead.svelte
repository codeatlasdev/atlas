<script lang="ts">
	import {
		Braces,
		Cog,
		Copy,
		Database,
		ExternalLink,
		Globe,
		ListChecks,
		MemoryStick,
		Server,
	} from "@lucide/svelte";
	import type { ServiceDef, ServiceKind } from "#lib/api.ts";
	import type { Service } from "#lib/stores/projects.svelte.ts";
	import StatusRing from "#lib/components/StatusRing.svelte";

	const KIND_ICON = {
		web: Globe,
		api: Braces,
		database: Database,
		cache: MemoryStick,
		worker: Cog,
		task: ListChecks,
	} satisfies Record<ServiceKind, unknown>;

	/** Identity row of a service card: status ring, name, state line and the port/target button. */
	let {
		service,
		def,
		busy,
		onopen,
	}: { service: Service; def: ServiceDef | undefined; busy: boolean; onopen: () => void } = $props();

	let K = $derived(def ? KIND_ICON[def.kind] : Server);
</script>

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

<style>
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
	.state-failed,
	.state-unhealthy {
		color: var(--ca-semantic-error);
	}
	.state-starting,
	.state-degraded {
		color: var(--ca-semantic-warning);
	}
	.dim {
		color: var(--ca-text-secondary);
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
		color: var(--space-accent-text, var(--ca-brand-main));
		transition: background 120ms ease-out;
	}
	.port:hover {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 14%, transparent);
	}
	.port.plain {
		color: var(--ca-text-secondary);
	}
	@media (prefers-reduced-motion: reduce) {
		.ring.pulse::after {
			animation: none;
		}
	}
</style>
