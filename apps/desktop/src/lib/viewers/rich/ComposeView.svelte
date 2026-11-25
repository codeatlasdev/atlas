<script lang="ts">
	import {
		Container,
		Hammer,
		Network,
		HardDrive,
		Variable,
		ArrowRight,
		TriangleAlert,
		HeartPulse,
		Play,
	} from "@lucide/svelte";
	import { parseData } from "#lib/viewers/rich/data.ts";
	import { composeModel } from "#lib/viewers/rich/compose.ts";
	import { runInTerminal } from "#lib/viewers/rich/run.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let parsed = $derived(parseData(ctx.text, "yaml"));
	let model = $derived(parsed.ok ? composeModel(parsed.value) : null);
	let focus = $state<string | null>(null);
	/** Services the focused one waits for, and the ones waiting for it. */
	let related = $derived.by(() => {
		if (!model || !focus) return new Set<string>();
		const s = model.services.find((x) => x.name === focus);
		const up = s?.dependsOn.map((d) => d.name) ?? [];
		const down = model.services.filter((x) => x.dependsOn.some((d) => d.name === focus)).map((x) => x.name);
		return new Set([focus, ...up, ...down]);
	});
	const dim = (n: string) => !!focus && !related.has(n);
	const run = (args: string[], label: string) =>
		runInTerminal(
			ctx.worktreeId,
			ctx.path,
			["docker", "compose", "-f", ctx.path.split("/").pop()!, ...args],
			label,
		);
</script>

<div class="rv">
	<div class="rv-inner">
		{#if !parsed.ok}
			<div class="rv-banner bad">
				<TriangleAlert size={14} />
				{m.viewers_devops_cannot_parse({ error: parsed.error })}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource(parsed.line)}>
					{m.viewers_devops_open_as_source()}
				</button>
			</div>
		{:else if model}
			<header class="rv-head">
				<div class="rv-title">
					<Container size={18} />
					<h1>{model.name ?? ctx.path.split("/").pop()}</h1>
					<span class="rv-ver">{m.viewers_devops_services_count({ count: model.services.length })}</span>
				</div>
				<div class="rv-facts">
					{#if model.volumes.length}<span class="rv-chip">
							<HardDrive size={10} />
							{m.viewers_devops_compose_view_volumes_count({ count: model.volumes.length })}
						</span>{/if}
					{#if model.networks.length}<span class="rv-chip">
							<Network size={10} />
							{model.networks.join(", ")}
						</span>{/if}
					<span class="sp"></span>
					<button class="rv-btn run" onclick={() => run(["up"], "compose up")}>
						<Play size={11} />
						{m.viewers_devops_compose_view_up()}
					</button>
					<button class="rv-btn" onclick={() => run(["ps"], "compose ps")}>
						{m.viewers_devops_compose_view_status()}
					</button>
				</div>
			</header>

			{#each model.unknownDeps as u (u.service + u.dep)}
				<div class="rv-banner warn">
					<TriangleAlert size={14} />
					{m.viewers_devops_compose_view_unknown_dep({ service: u.service, dep: u.dep })}
				</div>
			{/each}

			{#if model.services.some((s) => s.dependsOn.length)}
				<section class="rv-sec">
					<h2>{m.viewers_devops_start_order()}</h2>
					<div class="order">
						{#each model.order as n, i (n)}
							<button
								class="node"
								class:on={focus === n}
								class:dim={dim(n)}
								onclick={() => (focus = focus === n ? null : n)}
							>
								{n}
							</button>
							{#if i < model.order.length - 1}<ArrowRight size={11} class="arrow" />{/if}
						{/each}
					</div>
				</section>
			{/if}

			<section class="rv-sec">
				<h2>{m.viewers_devops_services()}</h2>
				<div class="grid">
					{#each model.services as s (s.name)}
						<article class="card" class:dim={dim(s.name)} class:on={focus === s.name}>
							<header>
								<button class="nm" onclick={() => (focus = focus === s.name ? null : s.name)}>
									{s.name}
								</button>
								{#if s.healthcheck}<span
										class="rv-chip ok"
										title={m.viewers_devops_compose_view_has_healthcheck()}
									>
										<HeartPulse size={10} />
									</span>{/if}
								{#if s.restart}<span class="rv-chip">{s.restart}</span>{/if}
								{#each s.profiles as p (p)}<span class="rv-chip info">{p}</span>{/each}
							</header>
							<div class="src">
								{#if s.image}<span class="rv-chip mono"><Container size={10} /> {s.image}</span>{/if}
								{#if s.build}<span class="rv-chip mono">
										<Hammer size={10} />
										{m.viewers_devops_compose_view_build({ build: s.build })}
									</span>{/if}
							</div>
							{#if s.command}<code class="cmd" title={s.command}>{s.command}</code>{/if}
							{#if s.ports.length}
								<div class="grp">
									<span class="lbl">
										{m.viewers_devops_compose_view_ports()}
									</span>{#each s.ports as p (p)}<span class="rv-chip info mono">{p}</span>{/each}
								</div>
							{/if}
							{#if s.volumes.length}
								<div class="grp">
									<span class="lbl">
										{m.viewers_devops_compose_view_volumes()}
									</span>{#each s.volumes as v (v)}<span class="rv-chip mono">{v}</span>{/each}
								</div>
							{/if}
							{#if s.env.length || s.envFiles.length}
								<div class="grp">
									<span class="lbl"><Variable size={10} /> {m.viewers_devops_compose_view_env()}</span>
									{#each s.envFiles as f (f)}<span class="rv-chip acc mono">{f}</span>{/each}
									{#each s.env.slice(0, 8) as e (e)}<span class="rv-chip mono" title={e}>
											{e.split("=")[0]}
										</span>{/each}
									{#if s.env.length > 8}<span class="rv-note">+{s.env.length - 8}</span>{/if}
								</div>
							{/if}
							{#if s.dependsOn.length}
								<div class="grp">
									<span class="lbl">{m.viewers_devops_compose_view_waits_for()}</span>
									{#each s.dependsOn as d (d.name)}
										<span class="rv-chip" class:warn={model.unknownDeps.some((u) => u.dep === d.name)}>
											{d.name}{d.condition ? ` · ${d.condition.replace("service_", "")}` : ""}
										</span>
									{/each}
								</div>
							{/if}
						</article>
					{/each}
				</div>
			</section>

			{#if model.volumes.length}
				<section class="rv-sec">
					<h2>
						{m.viewers_devops_compose_view_named_volumes()} <span class="n">{model.volumes.length}</span>
					</h2>
					<div class="rv-facts">
						{#each model.volumes as v (v)}<span class="rv-chip mono">{v}</span>{/each}
					</div>
				</section>
			{/if}
		{/if}
	</div>
</div>

<style>
	.order {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}
	.order :global(.arrow) {
		color: var(--ca-text-disabled);
	}
	.node {
		font: inherit;
		font-family: var(--mono);
		font-size: 11.5px;
		padding: 3px 9px;
		border-radius: 7px;
		border: 0.5px solid var(--rv-line);
		background: none;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.node.on {
		border-color: var(--rv-acc);
		color: var(--rv-acc);
		background: color-mix(in oklab, var(--rv-acc) 10%, transparent);
	}
	.dim {
		color: var(--ca-text-secondary);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
		gap: 10px;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 11px 12px;
		border: 0.5px solid var(--rv-line);
		border-radius: 10px;
		min-width: 0;
		transition: opacity var(--dur-fast, 120ms) var(--ease, ease);
	}
	.card.on {
		border-color: color-mix(in oklab, var(--rv-acc) 60%, transparent);
	}
	.card header {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}
	.nm {
		border: 0;
		background: none;
		padding: 0;
		margin-right: auto;
		font: inherit;
		font-size: 13.5px;
		font-weight: 650;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.src,
	.grp {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 5px;
	}
	.lbl {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		width: 52px;
		flex: none;
	}
	.cmd {
		font-family: var(--mono);
		font-size: 11px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.sp {
		flex: 1;
	}
</style>
