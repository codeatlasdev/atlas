<script lang="ts">
	import Contain from "#lib/errors/Contain.svelte";
	import DockResizer from "#lib/components/DockResizer.svelte";
	import Explorer from "#lib/components/Explorer.svelte";
	import PaneTree from "#lib/components/panes/PaneTree.svelte";
	import TabGroupView from "#lib/components/panes/TabGroupView.svelte";
	import SourceControl from "#lib/components/SourceControl.svelte";
	import Welcome from "#lib/components/Welcome.svelte";
	import SearchPanel from "#lib/search/SearchPanel.svelte";
	import RightDock from "#lib/shell/RightDock.svelte";
	import { docks, showExplorerOnce } from "#lib/stores/docks.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { leaves, ws } from "#lib/stores/workspace.svelte.ts";
	import { untrack } from "svelte";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { m } from "#lib/i18n/m.ts";

	const space = useSpace();
	let wt = $derived(space.worktree);
	let layout = $derived(wt ? ws.doc.layouts[wt.id] : undefined);
	let bottomId = $derived(wt ? ws.doc.bottomGroup[wt.id] : undefined);
	// The bottom dock's group exists whenever the dock is shown.
	$effect(() => {
		const id = wt?.id;
		if (docks.bottom && id) untrack(() => ws.bottomGroup(id));
	});
	// A plain folder is its files: show the explorer the first time it is the space on screen (not a mounted neighbour).
	$effect(() => {
		const id = wt?.id;
		if (id && wt?.plain && space.current) untrack(() => showExplorerOnce(id));
	});

	/** No tab in the main pane tree (the bottom dock has its own group). */
	let empty = $derived(!!layout && leaves(layout).every((id) => !ws.group(id)?.order.length));
	/** A plain folder: the Changes panel has nothing to show, so it says why instead of failing on `git status`. */
	let folder = $derived(space.project?.kind === "folder");
</script>

{#snippet dockPanel(id: string, edge: "left" | "right")}
	{#if edge === "right"}<DockResizer side="left" {edge} />{/if}
	<div class="dock" style:width="{docks.leftSize}px">
		<Contain
			where={docks.leftPanel === "explorer"
				? "Explorer"
				: docks.leftPanel === "search"
					? "Search"
					: "Source control"}
			title={m.shell_workbench_this_panel_hit_an_error()}
			hint={m.shell_contain_default_hint()}
		>
			{#if docks.leftPanel === "explorer"}<Explorer
					worktreeId={id}
				/>{:else if docks.leftPanel === "search"}<SearchPanel worktreeId={id} />{:else if folder}
				<div class="notgit" role="status">
					<b>{m.shell_workbench_not_a_git_repository()}</b>
					<span>{m.shell_workbench_this_folder_has_no_version()}</span>
				</div>
			{:else}<SourceControl worktreeId={id} />{/if}
		</Contain>
	</div>
	{#if edge === "left"}<DockResizer side="left" {edge} />{/if}
{/snippet}

<section class="body">
	{#if wt && layout}
		{#key wt.id}
			{#if prefs.value.project_panel.dock === "left" && docks.left && !docks.zoomed}
				{@render dockPanel(wt.id, "left")}
			{/if}
			<div class="center">
				<div class="tree" class:centered={docks.centered && !docks.zoomed}>
					{#if empty}
						<Welcome worktreeId={wt.id} />
					{:else if docks.zoomed}
						<TabGroupView groupId={ws.focusedGroup(wt.id).id} />
					{:else}
						<PaneTree {layout} worktreeId={wt.id} />
					{/if}
				</div>
				{#if docks.bottom && !docks.zoomed && bottomId}
					<DockResizer side="bottom" />
					<div class="bottom" style:height="{docks.bottomSize}px">
						<TabGroupView groupId={bottomId} />
					</div>
				{/if}
			</div>
			{#if prefs.value.project_panel.dock === "right" && docks.left && !docks.zoomed}
				{@render dockPanel(wt.id, "right")}
			{/if}
			<RightDock zoomed={docks.zoomed} />
		{/key}
	{/if}
</section>

<style>
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
	}
	.notgit {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 16px 14px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.notgit b {
		color: var(--ca-text-primary);
		font-weight: 600;
	}
	.center {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.tree {
		flex: 1;
		min-height: 0;
		display: flex;
	}
	/* Zed's centered layout: the panes keep a readable width on wide windows. */
	.tree.centered {
		width: 100%;
		max-width: 1200px;
		margin: 0 auto;
	}
	.bottom {
		flex: none;
		display: flex;
		min-height: 0;
	}
	.dock {
		flex: none;
		display: flex;
		min-width: 0;
		min-height: 0;
	}
</style>
