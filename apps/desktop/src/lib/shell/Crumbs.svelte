<script lang="ts">
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { ChevronLeft, ChevronRight, GitBranch } from "@lucide/svelte";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { openBranchPicker } from "#lib/stores/branch.svelte.ts";
	import Tip from "#lib/components/Tip.svelte";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";

	/**
	 * Where you are: `Space › worktree › tab`, each segment clickable, plus back/forward over
	 * the (worktree, tab) visit history. The space name is always visible here.
	 */
	const space = useSpace();
	const w = $derived(space.worktree);
	const tab = $derived(w ? ws.focusedTab(w.id) : undefined);
	const tabLabel = $derived(tab ? (tab.pty && ws.titles[tab.pty]) || tab.label : "");
	const spaceName = $derived(shell.active?.name ?? "");
	const wtName = $derived(w ? (w.isMain ? "main" : w.name) : "");

	/** Segment 1: the space's main checkout. 2: the worktree (re-shows its last tab). 3: the tab (re-activates it). */
	function toSpace() {
		if (shell.active) shell.select(shell.active.id);
	}
	/** "Back to Logs" / "Forward to main › Terminal": says where the arrow goes. */
	function where(delta: 1 | -1): string {
		const v = shell.target(delta);
		const verb = delta < 0 ? "Back" : "Forward";
		if (!v) return verb;
		const t = v.tabId ? ws.tab(v.tabId) : undefined;
		const tl = t ? (t.pty && ws.titles[t.pty]) || t.label : "";
		const vw = ws.worktree(v.worktreeId);
		const wn = vw ? (vw.isMain ? "main" : vw.name) : "";
		const label = v.worktreeId === w?.id ? tl || wn : [wn, tl].filter(Boolean).join(" › ");
		return label ? `${verb} to ${label}` : verb;
	}
	const hasHistory = $derived(shell.canGo(-1) || shell.canGo(1));
	function toWorktree() {
		if (w) shell.select(w.id);
	}
</script>

<nav class="crumbs" aria-label="Location" data-tour="breadcrumb">
	{#if hasHistory}<span class="nav">
			<Tip label={where(-1)} command="shell::Back">
				{#snippet child(props)}
					<button {...props} aria-label={where(-1)} disabled={!shell.canGo(-1)} onclick={() => shell.go(-1)}>
						<ChevronLeft size={14} />
					</button>
				{/snippet}
			</Tip>
			<Tip label={where(1)} command="shell::Forward">
				{#snippet child(props)}
					<button {...props} aria-label={where(1)} disabled={!shell.canGo(1)} onclick={() => shell.go(1)}>
						<ChevronRight size={14} />
					</button>
				{/snippet}
			</Tip>
		</span>{/if}
	{#if shell.active}
		<button class="seg space" onclick={toSpace} style:--c={shell.accent(shell.active.id)}>
			<ProjectIcon id={shell.active.id} size={16} /><span class="label">{spaceName}</span>
		</button>
	{/if}
	{#if w && store.current}
		<ChevronRight size={11} class="sep" />
		<button class="seg wt" class:last={!tab} onclick={toWorktree}><span class="label">{wtName}</span></button>
		{#if w.branch}
			<Tip label="Switch branch" command="git::Branch">
				{#snippet child(props)}
					<button
						{...props}
						class="seg branch"
						aria-label="Branch {w.branch}, switch"
						onclick={openBranchPicker}
					>
						<GitBranch size={12} /><span class="label mono">{w.branch}</span>
					</button>
				{/snippet}
			</Tip>
		{/if}
	{/if}
	{#if tab}
		<ChevronRight size={11} class="sep" />
		<span class="seg last" aria-current="page"><span class="label">{tabLabel}</span></span>
	{/if}
</nav>

<style>
	.crumbs {
		display: flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.nav {
		display: inline-flex;
		flex: none;
		margin-right: 4px;
	}
	.nav button {
		display: inline-grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: inherit;
		transition: background var(--dur-fast) var(--ease);
	}
	.nav button:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.nav button:disabled {
		color: var(--ca-text-disabled);
	}
	.crumbs :global(.sep) {
		flex: none;
		color: var(--ca-text-disabled);
	}
	.seg {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		height: 24px;
		padding: 0 6px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		color: inherit;
		white-space: nowrap;
	}
	.seg .label {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	/* Truncation order when the window narrows: the tab first, then the worktree, the space last. */
	.seg.space {
		flex: 0 1 auto;
		max-width: 180px;
	}
	.seg.wt {
		flex: 0 2 auto;
		max-width: 200px;
	}
	.seg.branch {
		flex: 0 3 auto;
		max-width: 200px;
		font-size: 11.5px;
	}
	.seg.last {
		flex: 0 4 auto;
		max-width: 260px;
	}
	button.seg {
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	button.seg:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.seg.last {
		color: var(--ca-text-primary);
		font-weight: 600;
	}
	.space {
		font-weight: 650;
		color: var(--ca-text-primary);
	}
</style>
