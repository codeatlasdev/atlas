<script lang="ts">
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { Network } from "@lucide/svelte";
	import { Spinner } from "@codeatlasdev/ui";
	import { api, type ProjectConfig, type ProjectView } from "#lib/api.ts";
	import { tilde } from "#lib/format.ts";
	import { revealLabel } from "#lib/keys.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import SetupRow from "#lib/components/SetupRow.svelte";
	import { store, type Live } from "#lib/stores/projects.svelte.ts";
	import { fmtUptime } from "./metrics.ts";
	import { portsLabel } from "./overview.ts";

	/** Title, facts line, setup row and the in-flight operation/tunnel strip of the panel. */
	let {
		project,
		cfg,
		live,
		uptime,
	}: {
		project: ProjectView;
		cfg: ProjectConfig | undefined;
		live: Live | undefined;
		/** Milliseconds since the stack came up, or null. */
		uptime: number | null;
	} = $props();

	let ports = $derived(portsLabel(cfg?.services ?? []));
	let modeLabel = $derived(live?.mode ?? project.mode ?? "running");
	const space = useSpace();
</script>

<div class="ident">
	<div class="title">
		<ProjectIcon id={project.parent ?? project.id} size={30} />
		<h1>{project.parent ? (space.worktree?.name ?? project.name) : project.name}</h1>
	</div>
	<div class="facts">
		{#if project.running}<span title="Time since the stack came up">Up {fmtUptime(uptime)}</span>{/if}
		{#if project.running && live?.mode}<span>{modeLabel}{live.version ? ` · v${live.version}` : ""}</span
			>{/if}
		{#if project.branch}<span class="mono"
				>{project.branch}{project.port_offset ? ` · ports +${project.port_offset}` : ""}</span
			>{/if}
		{#if ports}
			<span class="mono" title="Ports of the services">{ports}</span>
		{/if}
		<button
			class="root mono"
			title={revealLabel()}
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

<style>
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
		color: var(--space-accent-text, var(--ca-text-primary));
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
</style>
