<script lang="ts">
	import { GitBranch, TriangleAlert, ChevronRight, Timer, Cpu, Rocket, Terminal, Zap } from "@lucide/svelte";
	import { parseData } from "#lib/viewers/rich/data.ts";
	import { workflowModel, type Job } from "#lib/viewers/rich/workflow.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Code from "#lib/viewers/rich/Code.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let parsed = $derived(parseData(ctx.text, "yaml"));
	let w = $derived(parsed.ok ? workflowModel(parsed.value) : null);
	let open = $state<Record<string, boolean>>({});
	let levels = $derived.by(() => {
		const out: Job[][] = [];
		for (const j of w?.jobs ?? []) (out[j.level] ??= []).push(j);
		return out.filter(Boolean);
	});
	const unpinned = (job: string, uses: string | null) =>
		!!uses && !!w?.unpinned.some((u) => u.job === job && u.uses === uses);
	const short = (u: string) => u.replace(/^actions\//, "");
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
		{:else if w}
			<header class="rv-head">
				<div class="rv-title">
					<Rocket size={18} />
					<h1>{w.name ?? ctx.path.split("/").pop()}</h1>
					<span class="rv-ver">{m.viewers_devops_workflow_view_jobs_count({ count: w.jobs.length })}</span>
				</div>
				<div class="rv-facts">
					{#each w.triggers as t (t.event)}
						<span class="rv-chip info" title={t.detail}>
							<Zap size={10} />
							{t.event}{t.detail ? ` · ${t.detail}` : ""}
						</span>
					{/each}
				</div>
			</header>

			{#each w.unknownNeeds as u (u.job + u.need)}
				<div class="rv-banner bad">
					<TriangleAlert size={14} />
					{m.viewers_devops_workflow_view_unknown_need({ job: u.job, need: u.need })}
				</div>
			{/each}
			{#if w.unpinned.length}
				<div class="rv-banner warn">
					<TriangleAlert size={14} />
					{m.viewers_devops_workflow_view_unpinned_actions({ count: w.unpinned.length })}
				</div>
			{/if}

			<section class="rv-sec">
				<h2>{m.viewers_devops_workflow_view_pipeline()}</h2>
				<div class="flow">
					{#each levels as col, ci (ci)}
						<div class="col">
							{#each col as j (j.id)}
								<div
									class="jn"
									title={j.needs.length
										? m.viewers_devops_workflow_view_needs({ needs: j.needs.join(", ") })
										: m.viewers_devops_workflow_view_starts_immediately()}
								>
									<b>{j.name}</b>
									{#if j.needs.length}<span class="rv-note">
											{m.viewers_devops_workflow_view_after({ needs: j.needs.join(", ") })}
										</span>{/if}
								</div>
							{/each}
						</div>
						{#if ci < levels.length - 1}<div class="link" aria-hidden="true"></div>{/if}
					{/each}
				</div>
			</section>

			<section class="rv-sec">
				<h2>{m.viewers_devops_workflow_view_jobs()}</h2>
				<div class="rv-list">
					{#each w.jobs as j (j.id)}
						<div class="job">
							<button
								class="rv-row jh"
								aria-expanded={!!open[j.id]}
								onclick={() => (open[j.id] = !open[j.id])}
							>
								<ChevronRight size={13} class={open[j.id] ? "rot" : ""} />
								<span class="rv-k">{j.name}</span>
								{#if j.name !== j.id}<span class="rv-note">{j.id}</span>{/if}
								<span class="rv-chip mono"><Cpu size={10} /> {j.runsOn || "?"}</span>
								{#each Object.entries(j.matrix) as [k, v] (k)}<span
										class="rv-chip info mono"
										title={v.join(", ")}
									>
										{k} × {v.length}
									</span>{/each}
								{#if j.timeout}<span class="rv-chip">
										<Timer size={10} />
										{m.viewers_devops_workflow_view_timeout({ timeout: j.timeout })}
									</span>{/if}
								{#if j.cond}<span class="rv-chip warn mono" title={j.cond}><!-- i18n-ignore -->if</span>{/if}
								<span class="rv-v"></span>
								<span class="rv-note">
									{m.viewers_devops_workflow_view_steps_count({ count: j.steps.length })}
								</span>
							</button>
							{#if open[j.id]}
								<ol class="steps">
									{#each j.steps as s, i (i)}
										<li>
											<div class="sh">
												{#if s.uses}<span class="rv-chip mono" class:warn={unpinned(j.id, s.uses)}>
														{short(s.uses)}
													</span>
												{:else}<Terminal size={12} />{/if}
												<span class="sn">{s.name}</span>
												{#if s.cond}<span class="rv-chip mono" title={s.cond}>{`if ${s.cond}`}</span>{/if}
											</div>
											{#if s.run && s.run.trim() !== s.name}<Code code={s.run.trim()} lang="shell" />{/if}
											{#if Object.keys(s.with).length}
												<div class="with">
													{#each Object.entries(s.with) as [k, v] (k)}<span class="rv-chip mono">
															<b>{k}</b>
															{v.length > 40 ? v.slice(0, 40) + "…" : v}
														</span>{/each}
												</div>
											{/if}
										</li>
									{/each}
								</ol>
							{/if}
						</div>
					{/each}
				</div>
			</section>

			{#if Object.keys(w.env).length}
				<section class="rv-sec">
					<h2>{m.viewers_devops_environment()} <span class="n">{Object.keys(w.env).length}</span></h2>
					<div class="rv-facts">
						{#each Object.entries(w.env) as [k, v] (k)}<span class="rv-chip mono"><b>{k}</b> {v}</span>{/each}
					</div>
				</section>
			{/if}
		{/if}
	</div>
</div>

<style>
	.flow {
		display: flex;
		align-items: stretch;
		overflow-x: auto;
		padding-bottom: 4px;
	}
	.col {
		display: flex;
		flex-direction: column;
		flex-wrap: wrap;
		justify-content: center;
		align-content: flex-start;
		gap: 8px;
		flex: none;
		max-height: 190px;
	}
	.jn {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 130px;
		max-width: 260px;
		padding: 8px 12px;
		border-radius: 9px;
		border: 0.5px solid var(--rv-line);
		background: var(--rv-tint);
	}
	.jn b {
		font-size: 12.5px;
		font-weight: 650;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.link {
		flex: none;
		align-self: center;
		width: 28px;
		height: 1px;
		background: var(--ca-text-disabled);
	}
	.job {
		border-bottom: 0.5px solid var(--rv-line);
	}
	.job:last-child {
		border-bottom: 0;
	}
	.jh {
		width: 100%;
		border: 0;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
		border-bottom: 0;
	}
	.jh :global(.rot) {
		transform: rotate(90deg);
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 2px 12px 12px 34px;
		display: flex;
		flex-direction: column;
		gap: 9px;
		counter-reset: s;
	}
	.steps li {
		display: flex;
		flex-direction: column;
		gap: 5px;
		padding-left: 12px;
		border-left: 1.5px solid var(--rv-line);
	}
	.sh {
		display: flex;
		align-items: center;
		gap: 7px;
		flex-wrap: wrap;
		color: var(--ca-text-secondary);
	}
	.sn {
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}
	.with {
		display: flex;
		flex-wrap: wrap;
		gap: 5px;
	}
	b {
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.sp {
		flex: 1;
	}
</style>
