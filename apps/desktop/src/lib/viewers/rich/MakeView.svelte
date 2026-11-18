<script lang="ts">
	import { Play, ChevronRight, Search, Hammer, Star } from "@lucide/svelte";
	import { parseJustfile, parseMakefile, type Target } from "#lib/viewers/rich/makefile.ts";
	import { runInTerminal } from "#lib/viewers/rich/run.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Code from "#lib/viewers/rich/Code.svelte";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx, just = false }: { ctx: RichCtx; just?: boolean } = $props();

	let doc = $derived(just ? parseJustfile(ctx.text) : parseMakefile(ctx.text));
	let open = $state<Record<string, boolean>>({});
	let q = $state("");
	let shown = $derived(
		doc.targets.filter((t) => {
			const s = q.trim().toLowerCase();
			return !s || t.name.toLowerCase().includes(s) || t.doc.toLowerCase().includes(s);
		}),
	);
	const tool = $derived(just ? "just" : "make");
	const argv = (t: Target) => [tool, t.name];
	const key = (t: Target) => `${t.name}:${t.line}`;
</script>

<div class="rv">
	<div class="rv-inner">
		<header class="rv-head">
			<div class="rv-title">
				<Hammer size={18} />
				<h1>{ctx.path.split("/").pop()}</h1>
				<span class="rv-ver"
					>{just
						? m.viewers_devops_make_view_recipes_count({ count: doc.targets.length })
						: m.viewers_devops_make_view_targets_count({ count: doc.targets.length })}</span
				>
			</div>
			<div class="rv-facts">
				{#if doc.defaultTarget}<span class="rv-chip acc"
						><Star size={10} />
						{m.viewers_devops_make_view_default_target({ defaultTarget: doc.defaultTarget })}</span
					>{/if}
				{#each doc.includes as i (i)}<span class="rv-chip mono">{`include ${i}`}</span>{/each}
			</div>
		</header>

		{#if doc.variables.length}
			<section class="rv-sec">
				<h2>{m.viewers_devops_variables()} <span class="n">{doc.variables.length}</span></h2>
				<div class="rv-list">
					{#each doc.variables as v (v.line)}
						<div class="rv-row">
							<span class="rv-k">{v.name}</span>
							<span class="rv-v" title={v.value}>{v.value}</span>
							<button class="rv-btn" onclick={() => ctx.toSource(v.line)}
								>{m.viewers_devops_line({ line: v.line })}</button
							>
						</div>
					{/each}
				</div>
			</section>
		{/if}

		<section class="rv-sec">
			<h2>
				{just ? m.viewers_devops_make_view_recipes() : m.viewers_devops_make_view_targets()}
				<span class="sp"></span>
				<label class="rv-search">
					<Search size={12} />
					<input
						placeholder={m.viewers_devops_filter()}
						bind:value={q}
						aria-label={m.viewers_devops_make_view_filter_targets()}
					/>
				</label>
			</h2>
			<div class="rv-list">
				{#each shown as t (key(t))}
					<div class="tg">
						<div class="rv-row head">
							<button
								class="tw"
								aria-expanded={!!open[key(t)]}
								aria-label={m.viewers_devops_make_view_toggle_recipe({ name: t.name })}
								disabled={!t.recipe.length}
								onclick={() => (open[key(t)] = !open[key(t)])}
							>
								<ChevronRight size={13} class={open[key(t)] ? "rot" : ""} />
							</button>
							<span class="rv-k">{t.name}</span>
							{#each t.params as p (p)}<span class="rv-chip mono">{p}</span>{/each}
							{#if t.phony}<span class="rv-chip info">{m.viewers_devops_make_view_phony()}</span>{/if}
							{#if t.name === doc.defaultTarget}<span class="rv-chip acc"
									>{m.viewers_devops_make_view_default()}</span
								>{/if}
							{#if t.doc}<span class="rv-note doc" title={t.doc}>{t.doc}</span>{/if}
							<span class="rv-v">{t.deps.length ? `← ${t.deps.join(" ")}` : ""}</span>
							<button
								class="rv-btn icon"
								title={m.viewers_devops_go_to_line({ line: t.line })}
								aria-label={m.viewers_devops_go_to_line({ line: t.line })}
								onclick={() => ctx.toSource(t.line)}
							>
								<span class="ln">{t.line}</span>
							</button>
							{#if t.runnable && !t.params.length}
								<button
									class="rv-btn run"
									title={m.viewers_devops_make_view_run_in_terminal({ tool, name: t.name })}
									onclick={() => runInTerminal(ctx.worktreeId, ctx.path, argv(t), t.name)}
								>
									<Play size={11} />
									{m.viewers_devops_make_view_run()}
								</button>
							{/if}
							<Copy text={argv(t).join(" ")} label={m.viewers_devops_copy_command()} />
						</div>
						{#if open[key(t)] && t.recipe.length}
							<div class="rec"><Code code={t.recipe.join("\n")} lang="shell" /></div>
						{/if}
					</div>
				{:else}
					<div class="rv-empty">
						{q ? m.viewers_devops_make_view_no_match() : m.viewers_devops_make_view_no_targets()}
					</div>
				{/each}
			</div>
		</section>
	</div>
</div>

<style>
	.tg {
		border-bottom: 0.5px solid var(--rv-line);
	}
	.tg:last-child {
		border-bottom: 0;
	}
	.head {
		border-bottom: 0;
	}
	.tw {
		display: inline-flex;
		border: 0;
		background: none;
		padding: 2px;
		color: var(--ca-text-secondary);
		cursor: pointer;
		border-radius: 4px;
	}
	.tw:disabled {
		color: var(--ca-text-disabled);
		cursor: default;
	}
	.tw :global(.rot) {
		transform: rotate(90deg);
	}
	.doc {
		max-width: 38%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.rec {
		padding: 0 12px 10px 36px;
	}
	.ln {
		font-family: var(--mono);
		font-size: 10px;
		color: var(--ca-text-disabled);
	}
	.sp {
		flex: 1;
	}
</style>
