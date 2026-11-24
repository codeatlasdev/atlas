<script lang="ts">
	import {
		Play,
		RotateCw,
		Square,
		TriangleAlert,
		CircleCheck,
		Cable,
		Zap,
		ArrowRight,
		Loader,
	} from "@lucide/svelte";
	import { api, errText } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { parseData } from "#lib/viewers/rich/data.ts";
	import { atlasModel, lineOfKey } from "#lib/viewers/rich/atlas.ts";
	import type { Problem } from "#lib/api.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import StatusDot, { type DotTone } from "#lib/panel/StatusDot.svelte";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let parsed = $derived(parseData(ctx.text, "yaml"));
	let model = $derived(parsed.ok ? atlasModel(parsed.value) : null);
	let isLocal = $derived(/atlas\.local\./.test(ctx.path));

	// The project this worktree belongs to (a linked checkout is its own project, rooted at the worktree).
	let project = $derived.by(() => {
		const w = ws.worktree(ctx.worktreeId);
		return (
			store.projects.find((p) => p.root === w?.path) ?? store.projects.find((p) => p.id === w?.repoId) ?? null
		);
	});
	let live = $derived(project ? store.live[project.id] : undefined);
	let running = $derived(!!project?.running);
	const svc = (name: string) => live?.services.find((s) => s.name === name);

	let problems = $state<Problem[]>([]);
	let validating = $state(false);
	// Live validation through the same loader the CLI uses (300 ms debounce). The base file is validated
	// as shown; atlas.local.yaml is validated through the project (it merges onto the on-disk atlas.yaml).
	$effect(() => {
		const t = ctx.text;
		const p = project;
		if (!p || isLocal || !parsed.ok) {
			problems = [];
			return;
		}
		validating = true;
		let live = true;
		const timer = setTimeout(async () => {
			const r = await api
				.configValidate(p.id, t)
				.catch((e): Problem[] => [{ path: "", message: errText(e) }]);
			if (live) {
				problems = r;
				validating = false;
			}
		}, 300);
		return () => {
			live = false;
			clearTimeout(timer);
		};
	});

	const tone = (s: string | undefined): DotTone =>
		!s
			? "off"
			: s === "running"
				? "ok"
				: s === "starting"
					? "run"
					: s === "failed" || s === "crashed"
						? "fail"
						: s === "stopped"
							? "off"
							: "warn";
	const problemsFor = (name: string) =>
		problems.filter((p) => p.path.includes(`services.${name}`) || p.path.split(".").includes(name));
	const general = $derived(
		problems.filter((p) => !model?.services.some((s) => problemsFor(s.name).includes(p))),
	);
	const act = (cmd: "start_service" | "stop_service" | "restart_service", name: string) =>
		project && store.run(`svc:${project.id}:${name}`, () => api.service(project.id, cmd, name));
	const busy = (name: string) => !!(project && store.busy[`svc:${project.id}:${name}`]);
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
					<Zap size={18} />
					<h1>{model.name ?? (isLocal ? m.viewers_devops_atlas_view_local_overrides() : "atlas.yaml")}</h1>
					<span class="rv-ver">
						{m.viewers_devops_services_count({ count: model.services.length })} · {m.viewers_devops_atlas_view_actions_count(
							{
								count: model.actions.length,
							},
						)}
					</span>
				</div>
				<div class="rv-facts">
					{#if isLocal}<span class="rv-chip info">
							{m.viewers_devops_atlas_view_local_not_committed()}
						</span>{/if}
					{#if project}
						<span class="rv-chip" class:ok={running}>
							<StatusDot tone={running ? "ok" : "off"} />
							{running
								? m.viewers_devops_atlas_view_stack_running()
								: m.viewers_devops_atlas_view_stack_stopped()}
						</span>
					{/if}
					{#if validating}
						<span class="rv-chip">
							<Loader size={10} class="spin" />
							{m.viewers_devops_atlas_view_validating()}
						</span>
					{:else if !isLocal && project}
						{#if problems.length}
							<span class="rv-chip bad">
								<TriangleAlert size={10} />
								{m.viewers_devops_atlas_view_problems_count({ count: problems.length })}
							</span>
						{:else}
							<span class="rv-chip ok"><CircleCheck size={10} /> {m.viewers_devops_atlas_view_valid()}</span>
						{/if}
					{/if}
					{#if model.preCommand}
						<!-- i18n-ignore: the atlas.yaml key name, shown as a tooltip -->
						<span class="rv-chip mono" title="pre_command">
							{m.viewers_devops_atlas_view_pre_command({ preCommand: model.preCommand })}
						</span>
					{/if}
				</div>
			</header>

			{#if general.length}
				<section class="rv-sec">
					<h2>
						<TriangleAlert size={13} />
						{m.viewers_devops_atlas_view_validation()} <span class="n">{general.length}</span>
					</h2>
					<div class="rv-list">
						{#each general as p (p.path + p.message)}
							{@const line = lineOfKey(ctx.text, p.path)}
							<div class="rv-row bad">
								{#if p.path}<span class="rv-k">{p.path}</span>{/if}
								<span class="rv-v wrap">{p.message}</span>
								{#if line}<button class="rv-btn" onclick={() => ctx.toSource(line)}>
										{m.viewers_devops_line({ line })}
									</button>{/if}
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#each model.unknownDeps as u (u.service + u.dep)}
				<div class="rv-banner warn">
					<TriangleAlert size={14} />
					{isLocal
						? m.viewers_devops_atlas_view_unknown_dep_local({ name: u.service, dep: u.dep })
						: m.viewers_devops_atlas_view_unknown_dep({ name: u.service, dep: u.dep })}
				</div>
			{/each}

			{#if model.order.length > 1 && model.services.some((s) => s.dependsOn.length)}
				<section class="rv-sec">
					<h2>{m.viewers_devops_start_order()}</h2>
					<div class="order">
						{#each model.order as n, i (n)}
							<span class="node">{n}</span>
							{#if i < model.order.length - 1}<ArrowRight size={11} class="arrow" />{/if}
						{/each}
					</div>
				</section>
			{/if}

			{#if model.services.length}
				<section class="rv-sec">
					<h2>{m.viewers_devops_services()} <span class="n">{model.services.length}</span></h2>
					<div class="rv-list">
						{#each model.services as s (s.name)}
							{@const st = svc(s.name)}
							{@const probs = problemsFor(s.name)}
							<div class="svc" class:off={!s.enabled}>
								<div class="rv-row head" class:bad={probs.length > 0}>
									<StatusDot tone={running ? tone(st?.state) : "off"} pulse={st?.state === "starting"} />
									<span class="rv-k">{s.name}</span>
									{#if s.port}<span class="rv-chip info mono">:{s.port}</span>{/if}
									{#if s.critical}<span class="rv-chip warn">
											{m.viewers_devops_atlas_view_critical()}
										</span>{/if}
									{#if !s.enabled}<span class="rv-chip">{m.viewers_devops_atlas_view_disabled()}</span>{/if}
									{#if s.hideLogs}<span class="rv-chip">{m.viewers_devops_atlas_view_no_log_tab()}</span>{/if}
									{#if st && running}<span class="rv-note">
											{st.state}{st.restarts
												? ` · ${m.viewers_devops_atlas_view_restarts({ count: st.restarts })}`
												: ""}
										</span>{/if}
									<span class="rv-v" title={s.command ?? ""}>
										{s.command ?? m.viewers_devops_atlas_view_container_external()}
									</span>
									{#if running}
										<button
											class="rv-btn run"
											disabled={busy(s.name) || st?.state === "running"}
											title={m.viewers_devops_atlas_view_start_name({ name: s.name })}
											onclick={() => act("start_service", s.name)}
										>
											<Play size={11} />
											{m.viewers_devops_atlas_view_start()}
										</button>
										<button
											class="rv-btn"
											disabled={busy(s.name)}
											title={m.viewers_devops_atlas_view_restart_name({ name: s.name })}
											onclick={() => act("restart_service", s.name)}
										>
											<RotateCw size={11} />
											{m.viewers_devops_atlas_view_restart()}
										</button>
										<button
											class="rv-btn icon"
											disabled={busy(s.name) || st?.state === "stopped"}
											title={m.viewers_devops_atlas_view_stop_name({ name: s.name })}
											aria-label={m.viewers_devops_atlas_view_stop_name({ name: s.name })}
											onclick={() => act("stop_service", s.name)}
										>
											<Square size={11} />
										</button>
									{/if}
								</div>
								<div class="meta">
									{#if s.dir}<span class="rv-chip mono">
											{m.viewers_devops_atlas_view_dir({ dir: s.dir })}
										</span>{/if}
									{#if s.health}<span
											class="rv-chip ok mono"
											title={m.viewers_devops_atlas_view_health_url()}
										>
											{m.viewers_devops_atlas_view_health({ health: s.health })}
										</span>{/if}
									{#each s.dependsOn as d (d.name)}
										<span class="rv-chip" class:warn={model.unknownDeps.some((u) => u.dep === d.name)}>
											{m.viewers_devops_atlas_view_waits_for({ name: d.name })}{d.condition
												? ` · ${d.condition}`
												: ""}
										</span>
									{/each}
									{#if s.env.length}<span class="rv-chip mono" title={s.env.join(", ")}>
											{m.viewers_devops_atlas_view_env_count({ count: s.env.length })}
										</span>{/if}
								</div>
								{#each probs as p (p.path + p.message)}
									{@const line = lineOfKey(ctx.text, p.path)}
									<div class="prob">
										<TriangleAlert size={11} />
										<span class="pk">{p.path.split(".").slice(2).join(".") || p.path}</span>
										{p.message}
										{#if line}<button class="rv-btn" onclick={() => ctx.toSource(line)}>
												{m.viewers_devops_line({ line })}
											</button>{/if}
									</div>
								{/each}
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if model.actions.length}
				<section class="rv-sec">
					<h2>{m.viewers_devops_atlas_view_actions()} <span class="n">{model.actions.length}</span></h2>
					<div class="rv-list">
						{#each model.actions as a (a.key + a.label)}
							<div class="rv-row">
								<kbd>{a.key}</kbd>
								<span class="rv-k">{a.label}</span>
								{#if a.group}<span class="rv-chip">{a.group}</span>{/if}
								{#if a.confirm}<span class="rv-chip warn" title={a.confirm}>
										{m.viewers_devops_atlas_view_asks_first()}
									</span>{/if}
								{#if a.session}<span class="rv-chip info">{m.viewers_devops_atlas_view_session()}</span>{/if}
								<span class="rv-v" title={a.command}>{a.command}</span>
								<Copy text={a.command} label={m.viewers_devops_copy_command()} />
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if model.tunnel}
				<section class="rv-sec">
					<h2><Cable size={13} /> {m.viewers_devops_atlas_view_tunnel()}</h2>
					<div class="rv-list">
						<div class="rv-row">
							<span class="rv-chip" class:ok={model.tunnel.enabled}>
								{model.tunnel.enabled
									? m.viewers_devops_atlas_view_enabled()
									: m.viewers_devops_atlas_view_disabled()}
							</span>
							<span class="rv-k">{`localhost:${model.tunnel.local}`}</span>
							<ArrowRight size={11} />
							<span class="rv-v">
								{m.viewers_devops_atlas_view_tunnel_route({
									remote: model.tunnel.remote,
									ssh: model.tunnel.ssh,
								})}
							</span>
							{#if live?.tunnel}<span class="rv-chip" class:ok={live.tunnel.state === "up"}>
									{live.tunnel.state}
								</span>{/if}
						</div>
					</div>
				</section>
			{/if}

			{#if model.infra.length || model.envFiles.length || model.deployHosts.length}
				<section class="rv-sec">
					<h2>{m.viewers_devops_atlas_view_also_configured()}</h2>
					<div class="rv-facts">
						{#each model.infra as name (name)}<span class="rv-chip mono">
								{m.viewers_devops_atlas_view_infra({ name })}
							</span>{/each}
						{#each model.envFiles as file (file)}<span class="rv-chip acc mono">
								{m.viewers_devops_atlas_view_env_file({ file })}
							</span>{/each}
						{#each model.deployHosts as host (host)}<span class="rv-chip info mono">
								{m.viewers_devops_atlas_view_deploy_host({ host })}
							</span>{/each}
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
		font-family: var(--mono);
		font-size: 11.5px;
		padding: 3px 9px;
		border-radius: 7px;
		border: 0.5px solid var(--rv-line);
	}
	.svc {
		border-bottom: 0.5px solid var(--rv-line);
	}
	.svc:last-child {
		border-bottom: 0;
	}
	.svc.off {
		color: var(--ca-text-secondary);
	}
	.head {
		border-bottom: 0;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 5px;
		padding: 0 12px 8px 29px;
	}
	.meta:empty {
		display: none;
	}
	.pk {
		font-family: var(--mono);
		font-weight: 600;
	}
	.prob {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 0 12px 8px 29px;
		font-size: 11.5px;
		color: var(--rv-bad);
	}
	kbd {
		font-family: var(--mono);
		font-size: 11px;
		min-width: 20px;
		text-align: center;
		padding: 1px 5px;
		border-radius: 5px;
		border: 0.5px solid var(--rv-line);
		color: var(--ca-text-secondary);
	}
	.sp {
		flex: 1;
	}
	:global(.spin) {
		animation: spin 1s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
