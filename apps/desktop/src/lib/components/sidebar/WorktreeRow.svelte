<script lang="ts">
	/** One worktree of a project: agent glyph, name/branch, linked PR, services count, menu. */
	import { primaryKey } from "#lib/keys.ts";
	import { Ellipsis, GitBranch, Pin } from "@lucide/svelte";
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import RowMore from "#lib/components/RowMore.svelte";
	import { fromMenu } from "#lib/components/rowEvent.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { pr, rollup } from "#lib/stores/pr.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import RowMenu from "./RowMenu.svelte";
	import { strongest } from "./model.ts";
	import { sidebar } from "./state.svelte.ts";
	import type { ProjectView } from "#lib/api.ts";

	let { p, onAgent }: { p: ProjectView; onAgent?: (worktreeId: string) => void } = $props();

	const w = $derived(ws.worktree(p.id));
	const live = $derived(store.live[p.id]);
	const up = $derived(live?.services.filter((s) => s.state === "running").length ?? 0);
	/** Strongest agent state among the worktree's terminal tabs. */
	const glyph = $derived(
		strongest(
			ws
				.tabsOf(p.id)
				.filter((t) => t.agent)
				.map((t) => agents.status[t.id]?.state ?? "idle"),
		),
	);

	function select(e: MouseEvent) {
		if (primaryKey(e)) {
			sidebar.multi[p.id] = !sidebar.multi[p.id];
			return;
		}
		sidebar.multi = {};
		shell.select(p.id);
	}
</script>

{#if sidebar.matches(p, w)}
	<div
		class="item nested"
		class:active={ws.view === "worktree" && store.selected === p.id}
		class:multi={sidebar.multi[p.id]}
		class:unread={w?.unread}
		class:main={!p.parent}
		class:missing={!!p.missing}
		title={p.missing ? `Not found: ${p.root}` : undefined}
		role="button"
		tabindex="0"
		onmouseenter={() => (sidebar.hovered = p.id)}
		onmouseleave={() => sidebar.hovered === p.id && (sidebar.hovered = null)}
		onclick={(e) => fromMenu(e) || select(e)}
		onkeydown={(e) =>
			e.key === "Enter" && !fromMenu(e) && e.target === e.currentTarget && (store.selected = p.id)}
	>
		{#if glyph}<AgentGlyph state={glyph} />{:else}<span class="glyph"><GitBranch size={11} /></span>{/if}
		<span class="name">
			{p.parent ? (w?.name ?? p.root.split("/").pop()) : "main"}
			{#if p.branch}<small class="branch">{p.branch}</small>{/if}
		</span>
		{#if w?.linkedPr}
			{@const v = pr.views[p.id]}
			<span class="chip" class:bad={v && rollup(v) === "failed"}>
				PR #{w.linkedPr}{v ? ` · ${v.state.toLowerCase()}` : ""}
			</span>
		{/if}
		{#if p.missing}<span class="chip warn">Not found</span>{/if}
		{#if p.port_offset}<span class="chip">+{p.port_offset}</span>{/if}
		{#if p.parent && p.pinned}<Pin size={11} class="pin" />{/if}
		<span class="meta">
			{#if p.running}<span class="count">{up}/{live?.services.length || p.services_total}</span>{/if}
		</span>
		<RowMore label="Worktree menu" class="hover-btn">
			{#snippet icon()}<Ellipsis size={13} />{/snippet}
			<RowMenu {p} {w} {onAgent} />
		</RowMore>
	</div>
{/if}

<style>
	.item.nested {
		margin-left: 22px;
		height: 28px;
	}
	.item.active {
		background: color-mix(in oklab, var(--space-accent) 16%, transparent);
		box-shadow: inset 2px 0 0 var(--space-accent);
	}
	.item:focus-visible {
		box-shadow: 0 0 0 2px var(--atlas-focus-ring, var(--ca-action-focus));
	}
	.item.unread .name {
		font-weight: 750;
	}
	.item.missing .name,
	.item.missing .glyph {
		opacity: 0.5;
	}
	.chip.warn {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--dot-warn) 14%, transparent);
	}
	.item.multi {
		box-shadow: inset 0 0 0 1px var(--ca-brand-main);
	}
	.item :global(.pin) {
		color: var(--ca-text-secondary);
	}
	.item:hover :global(.hover-btn),
	.item:focus-within :global(.hover-btn) {
		display: inline-flex;
	}
	.item:hover .meta,
	.item:focus-within .meta {
		display: none;
	}
	.branch {
		margin-left: 6px;
		font-size: 10.5px;
		font-weight: 400;
		color: var(--ca-text-secondary);
	}
	.chip.bad {
		background: color-mix(in oklab, var(--ca-semantic-error) 20%, transparent);
		color: var(--ca-semantic-error);
	}
</style>
