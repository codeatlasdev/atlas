<script lang="ts">
	import {
		Layers,
		ExternalLink,
		TriangleAlert,
		Terminal,
		FileInput,
		Variable,
		Network,
		FolderOpen,
		User,
		Play,
		HeartPulse,
		Tag,
		ArrowDownToLine,
		CircleDot,
	} from "@lucide/svelte";
	import type { Component } from "svelte";
	import { api } from "#lib/api.ts";
	import { analyzeDockerfile, parseImage } from "#lib/viewers/rich/dockerfile.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Code from "#lib/viewers/rich/Code.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let doc = $derived(analyzeDockerfile(ctx.text));
	let active = $state(0);
	let stage = $derived(doc.stages[Math.min(active, doc.stages.length - 1)]);
	let warnLines = $derived(new Map(doc.warnings.map((w) => [w.line, w.message])));

	const ICONS: Record<string, Component> = {
		RUN: Terminal,
		CMD: Play,
		ENTRYPOINT: Play,
		COPY: FileInput,
		ADD: FileInput,
		ENV: Variable,
		ARG: Variable,
		EXPOSE: Network,
		WORKDIR: FolderOpen,
		USER: User,
		HEALTHCHECK: HeartPulse,
		LABEL: Tag,
		VOLUME: ArrowDownToLine,
	};
	const icon = (op: string): Component => ICONS[op] ?? CircleDot;
	const img = (s: string) => parseImage(s);
	const base = $derived(parseImage(stage?.image ?? ""));
	const envs = $derived(stage?.instructions.filter((i) => i.op === "ENV" || i.op === "ARG") ?? []);
	const shellLike = (op: string) =>
		op === "RUN" || op === "CMD" || op === "ENTRYPOINT" || op === "HEALTHCHECK";
	const body = (args: string) => (args.startsWith("[") ? args : args);
</script>

<div class="rv">
	<div class="rv-inner">
		<header class="rv-head">
			<div class="rv-title">
				<Layers size={18} />
				<h1>{ctx.path.split("/").pop()}</h1>
				<span class="rv-ver"
					>{m.viewers_devops_dockerfile_view_stages_count({ count: doc.stages.length })}</span
				>
			</div>
			<div class="rv-facts">
				{#each [...new Set(doc.ports.map((p) => p.port))] as p (p)}<span class="rv-chip info mono"
						>{`EXPOSE ${p}`}</span
					>{/each}
				{#if doc.warnings.length}<span class="rv-chip warn"
						>{m.viewers_devops_dockerfile_view_warnings_count({ count: doc.warnings.length })}</span
					>{/if}
			</div>
		</header>

		{#if !doc.stages.length}
			<div class="rv-banner warn">
				<TriangleAlert size={14} />
				{m.viewers_devops_dockerfile_view_no_from()}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource()}>{m.viewers_devops_open_as_source()}</button>
			</div>
		{:else}
			{#if doc.globalArgs.length}
				<div class="rv-facts">
					{#each doc.globalArgs as a (a.line)}<span class="rv-chip mono">{`ARG ${a.args}`}</span>{/each}
				</div>
			{/if}

			<section class="rv-sec">
				<h2>{m.viewers_devops_dockerfile_view_build_stages()}</h2>
				<div class="timeline" role="tablist" aria-label={m.viewers_devops_dockerfile_view_build_stages()}>
					{#each doc.stages as s (s.index)}
						{@const ref = img(s.image)}
						<button
							class="st"
							class:on={s.index === stage.index}
							role="tab"
							aria-selected={s.index === stage.index}
							onclick={() => (active = s.index)}
						>
							<span class="dot"></span>
							<span class="nm"
								>{s.name ?? m.viewers_devops_dockerfile_view_stage_default({ index: s.index })}</span
							>
							<span class="im">{s.image}</span>
							<span class="rv-note"
								>{m.viewers_devops_dockerfile_view_steps_count({ count: s.instructions.length })}</span
							>
							{#if s.index === doc.stages.length - 1}<span class="rv-chip acc"
									>{m.viewers_devops_dockerfile_view_final()}</span
								>{/if}
							{#if ref.tag === "latest" || (!ref.tag && !ref.digest && s.image !== "scratch" && !doc.stages.some((o) => o.name === s.image))}<span
									class="rv-chip warn">{m.viewers_devops_dockerfile_view_unpinned()}</span
								>{/if}
						</button>
					{/each}
				</div>
			</section>

			<section class="rv-sec">
				<h2>
					{m.viewers_devops_dockerfile_view_base_image()}
					<span class="sp"></span>
				</h2>
				<div class="rv-list">
					<div class="rv-row">
						<span class="rv-k">{base.repo || stage.image}</span>
						{#if base.tag}<span class="rv-chip mono" class:warn={base.tag === "latest"}>{base.tag}</span>{/if}
						{#if base.digest}<span class="rv-chip mono">{base.digest.slice(0, 19)}…</span>{/if}
						{#if base.registry}<span class="rv-note">{base.registry}</span>{/if}
						{#if stage.platform}<span class="rv-chip">{stage.platform}</span>{/if}
						<span class="rv-v"></span>
						{#if base.url}
							<button class="rv-btn" onclick={() => api.openUrl(base.url!)}
								><ExternalLink size={11} /> {m.viewers_devops_dockerfile_view_registry()}</button
							>
						{/if}
					</div>
				</div>
			</section>
			{#if envs.length}
				<section class="rv-sec">
					<h2>{m.viewers_devops_environment()} <span class="n">{envs.length}</span></h2>
					<div class="rv-facts">
						{#each envs as e (e.line)}<span class="rv-chip mono">{e.op} {e.args}</span>{/each}
					</div>
				</section>
			{/if}

			<section class="rv-sec">
				<h2>
					{m.viewers_devops_dockerfile_view_instructions()} <span class="n">{stage.instructions.length}</span>
				</h2>
				<div class="rv-list">
					{#each stage.instructions as ins (ins.line)}
						{@const Icon = icon(ins.op)}
						{@const warn = warnLines.get(ins.line)}
						<div class="ins" class:warn={!!warn}>
							<button
								class="gutter"
								title={m.viewers_devops_go_to_line({ line: ins.line })}
								aria-label={m.viewers_devops_go_to_line({ line: ins.line })}
								onclick={() => ctx.toSource(ins.line)}
							>
								<Icon size={13} />
							</button>
							<span class="op">{ins.op}</span>
							<div class="args">
								{#if ins.comment}<div class="rv-note">{ins.comment}</div>{/if}
								{#if shellLike(ins.op) || ins.args.includes("\n")}
									<Code
										code={ins.args}
										lang={ins.op === "RUN" || ins.op === "HEALTHCHECK" ? "shell" : null}
										inline
									/>
								{:else}
									<span class="plain">{body(ins.args)}</span>
								{/if}
								{#if warn}<div class="w"><TriangleAlert size={11} /> {warn}</div>{/if}
							</div>
						</div>
					{/each}
				</div>
			</section>

			{#if doc.warnings.length}
				<section class="rv-sec">
					<h2>
						<TriangleAlert size={13} />
						{m.viewers_devops_dockerfile_view_warnings()} <span class="n">{doc.warnings.length}</span>
					</h2>
					<div class="rv-list">
						{#each doc.warnings as w (w.line + w.message)}
							<div class="rv-row warn">
								<span class="rv-v wrap">{w.message}</span>
								<button class="rv-btn" onclick={() => ctx.toSource(w.line)}
									>{m.viewers_devops_line({ line: w.line })}</button
								>
							</div>
						{/each}
					</div>
				</section>
			{/if}
		{/if}
	</div>
</div>

<style>
	.timeline {
		display: flex;
		flex-direction: column;
		position: relative;
		padding-left: 4px;
	}
	.timeline::before {
		content: "";
		position: absolute;
		left: 10px;
		top: 14px;
		bottom: 14px;
		width: 1px;
		background: var(--rv-line);
	}
	.st {
		position: relative;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 7px 10px 7px 6px;
		border: 0;
		border-radius: 8px;
		background: none;
		font: inherit;
		color: var(--ca-text-primary);
		text-align: left;
		cursor: pointer;
		min-width: 0;
	}
	.st:hover {
		background: var(--rv-tint);
	}
	.st.on {
		background: color-mix(in oklab, var(--rv-acc) 10%, transparent);
	}
	.dot {
		width: 9px;
		height: 9px;
		flex: none;
		border-radius: 50%;
		border: 1.5px solid var(--ca-text-disabled);
		background: var(--ca-surface-raised, var(--ca-bg-default));
		z-index: 1;
	}
	.st.on .dot {
		border-color: var(--rv-acc);
		background: var(--rv-acc);
	}
	.nm {
		font-weight: 600;
		font-size: 12.5px;
		flex: none;
	}
	.im {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		min-width: 0;
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ins {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 7px 10px 7px 4px;
		border-bottom: 0.5px solid var(--rv-line);
	}
	.ins:last-child {
		border-bottom: 0;
	}
	.ins.warn {
		background: color-mix(in oklab, var(--rv-warn) 8%, transparent);
	}
	.gutter {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 20px;
		flex: none;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-disabled);
		cursor: pointer;
	}
	.gutter:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.op {
		flex: none;
		width: 84px;
		font-family: var(--mono);
		font-size: 11px;
		font-weight: 650;
		line-height: 20px;
		color: var(--rv-info);
	}
	.args {
		flex: 1;
		min-width: 0;
		line-height: 20px;
		display: flex;
		flex-direction: column;
	}
	.args :global(code),
	.plain {
		font-family: var(--mono);
		font-size: 11.5px;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--cm-fg, var(--ca-text-primary));
	}
	.w {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		color: color-mix(in oklab, var(--rv-warn) 85%, var(--ca-text-primary));
	}
</style>
