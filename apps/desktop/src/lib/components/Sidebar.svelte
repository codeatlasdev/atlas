<script lang="ts">
	/** Left sidebar: filter, fixed views, the active project and the collapsible list of the others. */
	import { IconButton, Kbd, Skeleton, Spinner } from "@codeatlasdev/ui";
	import {
		Bot,
		ChevronRight,
		LayoutDashboard,
		PanelLeftClose,
		RefreshCw,
		TriangleAlert,
	} from "@lucide/svelte";
	import { onMount } from "svelte";
	import { getCurrentWebview } from "@tauri-apps/api/webview";
	import IdentityDialog from "#lib/components/IdentityDialog.svelte";
	import SearchField from "#lib/components/SearchField.svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { register, shortcutFor } from "#lib/keymap.svelte.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { spaceSwipe } from "#lib/shell/gestures.ts";
	import { swipe } from "#lib/shell/swipe.ts";
	import { pushZones } from "#lib/shell/nativeSwipe.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { toggleSidebar } from "#lib/stores/docks.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import DeleteDialog from "./sidebar/DeleteDialog.svelte";
	import DiscoverDialog from "./sidebar/DiscoverDialog.svelte";
	import ProjectHead from "./sidebar/ProjectHead.svelte";
	import SidebarFooter from "./sidebar/SidebarFooter.svelte";
	import WorktreeRow from "./sidebar/WorktreeRow.svelte";
	import { droppedOn } from "./sidebar/addProject.ts";
	import {
		addFolder,
		addProject,
		refreshProjects,
		removeAllMissing,
		removeProject,
		showDiscover,
	} from "./sidebar/projects.ts";
	import { sidebar } from "./sidebar/state.svelte.ts";

	let { onJump, onAgent }: { onJump?: () => void; onAgent?: (worktreeId: string) => void } = $props();

	let repos = $derived(store.sorted.filter((p) => !p.parent));
	let activeRepo = $derived(repos.find((r) => r.id === shell.activeId) ?? null);
	let filtering = $derived(!!sidebar.filter.trim());
	let others = $derived(
		repos.filter((r) => r.id !== shell.activeId && (!filtering || sidebar.repoMatches(r))),
	);
	let activeKids = $derived(activeRepo ? sidebar.visibleKids(activeRepo.id) : []);
	/** "Other projects" is collapsed by default; a filter opens it so matches are never hidden. */
	let allOpen = $state(false);
	let listOpen = $derived(allOpen || filtering);
	let workingCount = $derived(
		ws.doc.tabs.filter((t) => t.agent && agents.status[t.id]?.state === "working").length,
	);
	let hasArchived = $derived(ws.doc.worktrees.some((w) => w.archived));
	/** Any row survives the filter (drives the "no match" hint). */
	let visible = $derived(repos.some(sidebar.repoMatches));

	let refreshing = $state(false);
	async function refresh() {
		refreshing = true;
		await refreshProjects();
		refreshing = false;
	}

	function onkeydown(e: KeyboardEvent) {
		if (!(e.metaKey && e.shiftKey && e.key === "Backspace")) return;
		const sel = Object.keys(sidebar.multi).filter((k) => sidebar.multi[k]);
		const ids = sel.length ? sel : sidebar.hovered ? [sidebar.hovered] : [];
		if (!ids.length) return;
		e.preventDefault();
		sidebar.askDelete(ids);
	}

	// Neighbour availability bounds the native swipe: re-push the zones when the spaces change.
	$effect(() => {
		void [shell.activeId, shell.spaceIds];
		pushZones();
	});

	let asideEl: HTMLElement | undefined = $state();
	let dropping = $state(false);

	/** A folder dragged from Finder onto the sidebar is added like a picked one (Tauri delivers native drops, not DOM ones). */
	onMount(() => {
		const off = getCurrentWebview().onDragDropEvent((e) => {
			const d = e.payload;
			if (d.type === "leave") dropping = false;
			if (d.type !== "over" && d.type !== "enter" && d.type !== "drop") return;
			const rect = asideEl?.getBoundingClientRect();
			const inside = !!rect && droppedOn(rect, d.position, window.devicePixelRatio);
			dropping = inside && d.type !== "drop";
			if (d.type === "drop" && inside) for (const path of new Set(d.paths)) void addFolder(path);
		});
		return () => void off.then((u) => u());
	});

	onMount(() =>
		register([
			{ id: "atlas::AddProject", label: "Add project…", run: () => void addProject() },
			{
				id: "atlas::RemoveProject",
				label: "Remove project from Atlas…",
				run: () => activeRepo && void removeProject(activeRepo.id),
				when: () => !!activeRepo,
			},
			{
				id: "atlas::DiscoverProjects",
				label: "Find projects on this machine",
				run: () => void showDiscover(),
			},
			{ id: "atlas::RefreshProjects", label: "Refresh projects", run: () => void refresh() },
			{
				id: "atlas::RemoveMissingProjects",
				label: "Remove missing projects",
				run: () => void removeAllMissing(),
				when: () => store.missing.length > 0,
			},
		]),
	);
</script>

<svelte:window {onkeydown} />

<aside class="sidebar" class:dropping data-tour="sidebar" bind:this={asideEl} use:swipe={spaceSwipe()}>
	<div class="drag" data-tauri-drag-region>
		<Tip label="Hide sidebar" command="shell::ToggleSidebar" side="bottom">
			{#snippet child(props)}
				<IconButton
					{...props}
					class="hide-btn"
					variant="text"
					size="sm"
					aria-label="Hide sidebar"
					onclick={toggleSidebar}
				>
					<PanelLeftClose size={15} />
				</IconButton>
			{/snippet}
		</Tip>
		<Tip label="Refresh projects" command="atlas::RefreshProjects" side="bottom">
			{#snippet child(props)}
				<IconButton
					{...props}
					class="refresh-btn"
					variant="text"
					size="sm"
					aria-label="Refresh projects"
					disabled={refreshing}
					onclick={refresh}
				>
					{#if refreshing}<Spinner size={13} />{:else}<RefreshCw size={14} />{/if}
				</IconButton>
			{/snippet}
		</Tip>
	</div>
	<div class="filter">
		<SearchField
			compact
			clearable
			label="Clear filter"
			placeholder="Filter worktrees"
			bind:value={sidebar.filter}
			onkeydown={(e) => e.key === "Escape" && ((sidebar.filter = ""), e.currentTarget.blur())}
		/>
		<Tip label="Jump to worktree or terminal" command="atlas::Jump" side="bottom">
			{#snippet child(props)}
				<button
					{...props}
					class="jump"
					data-tour="palette"
					aria-label="Jump to worktree or terminal"
					onclick={() => onJump?.()}
				>
					<Kbd>{shortcutFor("atlas::Jump") ?? "⌘K"}</Kbd>
				</button>
			{/snippet}
		</Tip>
	</div>
	{#if store.missing.length}
		<div class="missing-bar" role="status">
			<TriangleAlert size={13} />
			<span>{store.missing.length} not found</span>
			<button onclick={refresh}>Refresh</button>
			<button onclick={removeAllMissing}>Remove all</button>
		</div>
	{/if}
	<nav class="list">
		<button class="item fixed" class:active={ws.view === "agents"} onclick={() => (ws.view = "agents")}>
			<Bot size={13} /><span class="name">Agents</span>
			{#if workingCount}<span class="count">{workingCount}</span>{/if}
		</button>
		<button class="item fixed" class:active={ws.view === "dashboard"} onclick={() => (ws.view = "dashboard")}>
			<LayoutDashboard size={13} /><span class="name">Dashboard</span>
		</button>
		{#if activeRepo}
			<ProjectHead r={activeRepo} active {onAgent} />
			{#if activeKids.length}
				{#each [activeRepo, ...activeKids] as p (p.id)}<WorktreeRow {p} {onAgent} />{/each}
			{/if}
			{@render creating(activeRepo.id)}
		{/if}
		{#if others.length}
			<button class="section" aria-expanded={listOpen} onclick={() => (allOpen = !allOpen)}>
				<ChevronRight size={11} class="chev {listOpen ? 'open' : ''}" />
				<span>{activeRepo ? "Other projects" : "Projects"}</span>
				<span class="count">{others.length}</span>
			</button>
			{#if listOpen}
				{#each others as r (r.id)}
					<ProjectHead {r} active={false} {onAgent} />
					{#if sidebar.openRepos[r.id] || filtering}
						{#each sidebar.visibleKids(r.id) as p (p.id)}<WorktreeRow {p} {onAgent} />{/each}
					{/if}
					{@render creating(r.id)}
				{/each}
			{/if}
		{/if}
		{#if !store.loaded}
			<div class="skeleton" aria-busy="true">
				{#each [0, 1, 2, 3] as i (i)}<Skeleton
						variant="rounded"
						height={22}
						width={i === 0 ? "55%" : "85%"}
					/>{/each}
			</div>
		{:else if repos.length === 0}
			<p class="hint">No projects yet. Add a folder with an <code>atlas.yaml</code>.</p>
		{:else if filtering && !visible}
			<p class="hint">No worktree matches “{sidebar.filter.trim()}”.</p>
		{/if}
	</nav>
	{#if hasArchived}
		<label class="archived"><input type="checkbox" bind:checked={sidebar.showArchived} /> Show archived</label
		>
	{/if}
	<SidebarFooter onDiscover={showDiscover} />
</aside>

{#snippet creating(repoId: string)}
	{#each Object.values(worktrees.creations).filter((c) => c.repoId === repoId) as c (c.id)}
		<div class="item creating nested">
			<span class="glyph"><Spinner size={11} /></span>
			<span class="name">{c.name}<small class="branch">{c.step}…</small></span>
		</div>
	{/each}
{/snippet}

<DeleteDialog />
<IdentityDialog />
<DiscoverDialog />

<style>
	.sidebar {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		/* The theme's own ground, opaque (theme/ground.ts): text contrast is a property of the theme tokens, never of the
		   wallpaper or macOS appearance. */
		background: var(--ca-surface-sunken);
	}
	/* A folder is being dragged over: it will be added. */
	.sidebar.dropping {
		box-shadow: inset 0 0 0 2px var(--ca-brand-main);
	}
	.drag {
		height: 52px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		padding-right: 10px;
	}
	/* Hidden until the sidebar is hovered, like Arc's chrome. */
	.drag :global(.hide-btn),
	.drag :global(.refresh-btn) {
		opacity: 0;
		transition: opacity var(--dur) var(--ease);
	}
	.sidebar:hover .drag :global(.hide-btn),
	.sidebar:hover .drag :global(.refresh-btn),
	.drag :global(.hide-btn:focus-visible),
	.drag :global(.refresh-btn:focus-visible) {
		opacity: 1;
	}
	.missing-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0 8px 4px;
		padding: 5px 8px;
		border-radius: 8px;
		font-size: 11.5px;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--dot-warn) 12%, transparent);
	}
	.missing-bar span {
		flex: 1;
	}
	.missing-bar button {
		border: 0;
		padding: 0;
		background: none;
		font: inherit;
		color: var(--ca-text-secondary);
	}
	.missing-bar button:hover {
		color: var(--ca-text-primary);
		text-decoration: underline;
	}
	.filter {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0 12px 8px;
	}
	.jump {
		border: 0;
		background: none;
		padding: 0;
	}
	.list {
		flex: 1;
		overflow-y: auto;
		padding: 0 10px;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 8px;
	}
	.hint {
		margin: 0;
		padding: 12px 8px;
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.hint code {
		font-family: var(--mono);
		font-size: 11px;
	}
	.section {
		display: flex;
		align-items: center;
		gap: 4px;
		height: 24px;
		margin-top: 10px;
		padding: 0 6px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.02em;
		color: var(--ca-text-secondary);
		text-align: left;
	}
	.section:hover {
		color: var(--ca-text-primary);
	}
	.section .count {
		margin-left: auto;
	}
	.section :global(.chev) {
		transition: transform var(--dur-fast) var(--ease);
	}
	.section :global(.chev.open) {
		transform: rotate(90deg);
	}
	.archived {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 18px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}

	/* Row base shared by the fixed views, "creating" placeholders and the worktree rows. */
	.list :global(.item) {
		position: relative;
		display: flex;
		align-items: center;
		gap: 9px;
		height: 30px;
		padding: 0 8px;
		border-radius: 7px;
		font-size: 13px;
		color: var(--ca-text-primary);
		outline: none;
		transition: background 150ms ease-out;
	}
	.list :global(.item:hover) {
		background: var(--ca-vibrancy-fill);
	}
	.list :global(.item.nested) {
		margin-left: 22px;
		height: 28px;
	}
	.item.fixed {
		width: 100%;
		border: 0;
		background: none;
		font: inherit;
		text-align: left;
	}
	.item.fixed.active {
		background: color-mix(in oklab, var(--ca-text-primary) 11%, transparent);
	}
	.item.creating {
		color: var(--ca-text-secondary);
	}
	.list :global(.name) {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 500;
	}
	.list :global(.glyph) {
		width: 14px;
		display: inline-flex;
		justify-content: center;
		color: var(--ca-text-disabled);
		font-size: 11px;
		font-weight: 700;
	}
	.list :global(.branch) {
		margin-left: 6px;
		font-size: 10.5px;
		font-weight: 400;
		color: var(--ca-text-secondary);
	}
	.list :global(.count) {
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
	}
	.list :global(.chip) {
		font-size: 10px;
		padding: 0 5px;
		border-radius: 4px;
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
		color: var(--ca-text-secondary);
		white-space: nowrap;
	}
	.list :global(.hover-btn) {
		display: none;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.list :global(.hover-btn:hover) {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
</style>
