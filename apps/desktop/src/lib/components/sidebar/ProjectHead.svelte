<script lang="ts">
	/**
	 * A project: its icon, name, live status. The active one also acts as its own `main` row
	 * when it has no other worktrees (a lone "main" child would be redundant).
	 */
	import { ChevronRight, Ellipsis, Pin, Plus } from "@lucide/svelte";
	import RowMore from "#lib/components/RowMore.svelte";
	import { fromMenu } from "#lib/components/rowEvent.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { health, store } from "#lib/stores/projects.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import RowMenu from "./RowMenu.svelte";
	import { sidebar } from "./state.svelte.ts";
	import type { ProjectView } from "#lib/api.ts";

	let { r, active, onAgent }: { r: ProjectView; active: boolean; onAgent?: (worktreeId: string) => void } =
		$props();

	const live = $derived(store.live[r.id]);
	const kids = $derived(sidebar.kids(r.id));
	const solo = $derived(kids.length === 0);
	const w = $derived(ws.worktree(r.id));
	const up = $derived(live?.services.filter((s) => s.state === "running").length ?? 0);
	const open = $derived(!!sidebar.openRepos[r.id]);

	/** The active project with worktrees only selects; everything else switches to it. */
	function activate() {
		if (active && !solo) shell.select(r.id);
		else shell.switchTo(r.id);
	}
</script>

<div
	class="project"
	class:current={active}
	class:missing={!!r.missing}
	title={r.missing ? `Not found: ${r.root}` : undefined}
	class:selected={active && solo && ws.view === "worktree" && store.selected === r.id}
	class:unread={w?.unread}
	style:--c={shell.accent(r.id)}
	role="button"
	tabindex="0"
	aria-label="{r.name}{active ? '' : ' — open project'}"
	oncontextmenu={(e) => {
		e.preventDefault();
		identities.editing = r.id;
	}}
	onmouseenter={() => (sidebar.hovered = r.id)}
	onmouseleave={() => sidebar.hovered === r.id && (sidebar.hovered = null)}
	onclick={(e) => fromMenu(e) || activate()}
	onkeydown={(e) => e.key === "Enter" && !fromMenu(e) && e.target === e.currentTarget && activate()}
>
	{#if !active && kids.length}
		<button
			class="twist"
			aria-label="{open ? 'Hide' : 'Show'} worktrees of {r.name}"
			aria-expanded={open}
			onclick={(e) => {
				e.stopPropagation();
				sidebar.openRepos[r.id] = !open;
			}}
		>
			<ChevronRight size={11} class="chev {open ? 'open' : ''}" />
		</button>
	{:else}
		<span class="twist" aria-hidden="true"></span>
	{/if}
	<span class="ico">
		<ProjectIcon id={r.id} size={20} ring={active} />
		<i
			class="dot {r.missing ? 'off' : health(r, live)}"
			class:pulse={r.running}
			title={r.running ? "Running" : "Stopped"}
		></i>
	</span>
	<span class="pname">{r.name}</span>
	{#if r.missing}<span class="gone">Not found</span>{/if}
	{#if r.host}<span class="chip">{r.host.replace(/^ssh:/, "")}</span>{/if}
	{#if r.pinned}<Pin size={10} class="pin" />{/if}
	<span class="meta">
		{#if r.running}<span class="count">{up}/{live?.services.length || r.services_total}</span>{/if}
		{#if !active && kids.length}<span class="count">{kids.length + 1} wt</span>{/if}
	</span>
	{#if !r.missing}
		<button
			class="hover-btn"
			aria-label="New worktree"
			title="New worktree"
			onclick={(e) => {
				e.stopPropagation();
				worktrees.newDialog = { open: true, repoId: r.id };
			}}
		>
			<Plus size={13} />
		</button>
	{/if}
	<RowMore label="Project menu" class="hover-btn">
		{#snippet icon()}<Ellipsis size={13} />{/snippet}
		<RowMenu p={r} {w} {onAgent} />
	</RowMore>
</div>

<style>
	/* A project row: icon tile (with its live status dot), name, services count. */
	.project {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 32px;
		margin-top: 2px;
		padding: 0 4px 0 2px;
		border-radius: 8px;
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		outline: none;
		cursor: default;
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	.project:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.project.current {
		color: var(--ca-text-primary);
		margin-top: 6px;
	}
	.project.selected {
		background: color-mix(in oklab, var(--c) 16%, transparent);
		box-shadow: inset 2px 0 0 var(--c);
	}
	.project:focus-visible {
		box-shadow: 0 0 0 2px var(--ca-action-focus);
	}
	.project.unread .pname {
		font-weight: 750;
	}
	/* Gone from disk: dimmed, still selectable so its recovery view can open. */
	.project.missing .pname,
	.project.missing .ico {
		opacity: 0.5;
	}
	.gone {
		flex: none;
		font-size: 10px;
		padding: 0 5px;
		border-radius: 4px;
		white-space: nowrap;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--dot-warn) 14%, transparent);
	}
	.twist {
		display: inline-grid;
		place-items: center;
		flex: none;
		width: 14px;
		height: 20px;
		padding: 0;
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
	}
	.ico {
		position: relative;
		display: inline-flex;
		flex: none;
	}
	/* Status sits on the tile's corner, ringed with the surface so it reads on any icon. */
	.ico .dot {
		position: absolute;
		right: -3px;
		bottom: -3px;
		width: 9px;
		height: 9px;
		box-shadow: 0 0 0 2px var(--ca-surface-base);
	}
	.ico .dot.pulse.ok {
		animation: alive 2.4s ease-in-out infinite;
	}
	@keyframes alive {
		50% {
			box-shadow:
				0 0 0 2px var(--ca-surface-base),
				0 0 0 5px color-mix(in oklab, var(--dot-ok) 28%, transparent);
		}
	}
	.pname {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		display: inline-flex;
		gap: 6px;
	}
	.project:hover .meta,
	.project:focus-within .meta {
		display: none;
	}
	.project:hover :global(.hover-btn),
	.project:focus-within :global(.hover-btn) {
		display: inline-flex;
	}
	.project :global(.chev) {
		transition: transform var(--dur-fast) var(--ease);
	}
	.project :global(.chev.open) {
		transform: rotate(90deg);
	}
</style>
