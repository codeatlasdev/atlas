<script lang="ts">
	import {
		Button,
		DropdownMenu,
		DropdownMenuItem,
		DropdownMenuLabel,
		EmptyState,
		IconButton,
		Spinner,
		Skeleton,
		Dialog,
		Alert,
		NotificationBell,
	} from "@codeatlasdev/ui";
	import {
		Play,
		Square,
		ChevronDown,
		FolderPlus,
		Code,
		Pin,
		PinOff,
		Trash2,
		Ellipsis,
		PanelLeft,
		PanelBottom,
	} from "@lucide/svelte";
	import { open as pickFolder } from "@tauri-apps/plugin-dialog";
	import { api } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { pending, runAction } from "#lib/actions.svelte.ts";
	import PaneTree from "#lib/components/panes/PaneTree.svelte";
	import TabGroupView from "#lib/components/panes/TabGroupView.svelte";
	import { leaves, ws } from "#lib/stores/workspace.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import AgentsView from "#lib/components/AgentsView.svelte";
	import Explorer from "#lib/components/Explorer.svelte";
	import SourceControl from "#lib/components/SourceControl.svelte";
	import { docks, toggleDock } from "#lib/stores/docks.svelte.ts";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import DockResizer from "#lib/components/DockResizer.svelte";
	import RightDock from "#lib/shell/RightDock.svelte";
	import Crumbs from "#lib/shell/Crumbs.svelte";
	import RightToggle from "#lib/shell/RightToggle.svelte";
	import SearchPanel from "#lib/search/SearchPanel.svelte";
	import PanelOps from "#lib/panel/PanelOps.svelte";
	import ActivityDrawer from "#lib/panel/ActivityDrawer.svelte";
	import { summarize } from "#lib/panel/metrics.ts";
	import Welcome from "#lib/components/Welcome.svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { untrack } from "svelte";

	function openInbox(id: string) {
		const n = agents.inbox.find((x) => x.id === id);
		if (!n) return;
		n.read = true;
		ws.view = "worktree";
		store.selected = n.worktreeId;
		ws.select(n.worktreeId);
		if (ws.tab(n.tabId)) ws.activate(n.tabId);
	}
	let p = $derived(store.current);
	let live = $derived(p ? store.live[p.id] : undefined);
	let busy = $derived(p ? !!store.busy[`stack:${p.id}`] : false);
	let removeOpen = $state(false);
	let cfg = $derived(p ? store.configs[p.id] : undefined);
	let health = $derived(
		summarize(
			(p?.running && live?.services.length ? live.services : (cfg?.services ?? [])).map((s) =>
				"state" in s ? s.state : "stopped",
			),
			!!p?.running,
		),
	);
	let wt = $derived(ws.selected);
	let layout = $derived(wt ? ws.doc.layouts[wt.id] : undefined);
	let bottomId = $derived(wt ? ws.doc.bottomGroup[wt.id] : undefined);
	// The bottom dock's group exists whenever the dock is shown.
	$effect(() => {
		const id = wt?.id;
		if (docks.bottom && id) untrack(() => ws.bottomGroup(id));
	});

	/** No tab in the main pane tree (the bottom dock has its own group). */
	let empty = $derived(!!layout && leaves(layout).every((id) => !ws.group(id)?.order.length));
	const DOCKS = [
		{ side: "left", label: "Left dock", command: "workspace::ToggleLeftDock", icon: PanelLeft },
		{ side: "bottom", label: "Bottom dock", command: "workspace::ToggleBottomDock", icon: PanelBottom },
	] as const;

	async function add() {
		const dir = await pickFolder({ directory: true, title: "Add atlas project" });
		if (typeof dir !== "string") return;
		const v = await store.run("add", () => api.add(dir));
		if (v) {
			await store.refresh();
			store.selected = v.id;
		}
	}
</script>

{#snippet dockPanel(id: string, edge: "left" | "right")}
	{#if edge === "right"}<DockResizer side="left" {edge} />{/if}
	<div class="dock" style:width="{docks.leftSize}px">
		{#if docks.leftPanel === "explorer"}<Explorer
				worktreeId={id}
			/>{:else if docks.leftPanel === "search"}<SearchPanel worktreeId={id} />{:else}<SourceControl
				worktreeId={id}
			/>{/if}
	</div>
	{#if edge === "left"}<DockResizer side="left" {edge} />{/if}
{/snippet}

{#if ws.view !== "worktree"}
	<AgentsView mode={ws.view} />
{:else if !p}
	<div class="drag-strip" data-tauri-drag-region></div>
	<div class="empty">
		{#if !store.loaded}
			<div class="loading" aria-busy="true">
				<Skeleton variant="rounded" width={220} height={22} />
				<Skeleton variant="text" rows={2} width={320} />
			</div>
		{:else if store.projects.length === 0}
			<EmptyState
				variant="hero"
				icon={FolderPlus}
				title="Welcome to Atlas"
				description="Add a project with an atlas.yaml. Each git worktree gets its own stack, terminals and agents."
			>
				{#snippet actions()}
					<Button onclick={add}>Add project…</Button>
					<Button variant="ghost" onclick={() => runCommand("atlas::DiscoverProjects")}>Find projects</Button>
				{/snippet}
			</EmptyState>
		{:else}
			<EmptyState
				icon={FolderPlus}
				title="No project selected"
				description="Pick a worktree in the sidebar, or press {shortcutFor('atlas::Jump') ?? '⌘K'} to jump."
			>
				{#snippet actions()}
					<Button variant="glass" onclick={add}>Add project…</Button>
				{/snippet}
			</EmptyState>
		{/if}
	</div>
{:else}
	<header data-tauri-drag-region class:inset={!docks.sidebar}>
		<Crumbs />
		<span class="pill t-{health.tone}" title={health.label}>
			<span class="dot" class:pulse={health.tone === "ok"}></span>{p.running ? health.label : "Stopped"}
		</span>
		<div class="grow" data-tauri-drag-region></div>
		<div class="actions">
			<NotificationBell
				notifications={agents.inbox.map((n) => ({
					id: n.id,
					title: n.title,
					body: n.body,
					read: n.read,
					time: new Date(n.ts).toLocaleTimeString(),
					variant: n.body === "Needs you" ? "warning" : "success",
				}))}
				onRead={openInbox}
				onReadAll={() => agents.inbox.forEach((n) => (n.read = true))}
				onClear={() => (agents.inbox = [])}
			/>
			<div class="docks" role="group" aria-label="Docks">
				{#each DOCKS as d (d.side)}
					<Tip label="Toggle {d.label.toLowerCase()}" command={d.command}>
						{#snippet child(props)}
							<button
								{...props}
								class="dock-btn"
								class:on={docks[d.side]}
								aria-label="Toggle {d.label.toLowerCase()}"
								aria-pressed={docks[d.side]}
								onclick={() => toggleDock(d.side)}
							>
								<d.icon size={15} />
							</button>
						{/snippet}
					</Tip>
				{/each}
				<RightToggle />
			</div>
			<div class="ops" role="group" aria-label="Activity and actions">
				<PanelOps project={p} />
			</div>
			<DropdownMenu align="end">
				{#snippet trigger()}
					<span class="more" aria-label="Project menu"><Ellipsis size={16} /></span>
				{/snippet}
				<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}>
					<Code size={14} /> Open in external editor
				</DropdownMenuItem>
				<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
					{#if p.pinned}<PinOff size={14} /> Unpin{:else}<Pin size={14} /> Pin to top{/if}
				</DropdownMenuItem>
				<DropdownMenuItem variant="destructive" onclick={() => (removeOpen = true)}>
					<Trash2 size={14} /> Remove from Atlas
				</DropdownMenuItem>
			</DropdownMenu>
			{#if p.running}
				<Button
					variant="glass"
					size="sm"
					disabled={busy}
					onclick={() => store.run(`stack:${p.id}`, () => api.stop(p.id))}
				>
					{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<Square
								size={12}
								fill="currentColor"
							/>{/if}{/snippet}
					Stop
				</Button>
			{:else}
				<div class="split" class:has-menu={p.profiles.length > 0}>
					<Button
						size="sm"
						disabled={busy || !!p.error}
						onclick={() => store.run(`stack:${p.id}`, () => api.start(p.id))}
					>
						{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<Play
									size={12}
									fill="currentColor"
								/>{/if}{/snippet}
						Start
					</Button>
					{#if p.profiles.length}
						<DropdownMenu align="end">
							{#snippet trigger()}
								<span class="split-arrow" aria-label="Start with profile"><ChevronDown size={13} /></span>
							{/snippet}
							<DropdownMenuLabel>Start with profile</DropdownMenuLabel>
							{#each p.profiles as prof (prof)}
								<DropdownMenuItem onclick={() => store.run(`stack:${p.id}`, () => api.start(p.id, [prof]))}>
									{prof}
								</DropdownMenuItem>
							{/each}
						</DropdownMenu>
					{/if}
				</div>
			{/if}
		</div>
	</header>
	<ActivityDrawer projectId={p.id} />

	{#if agents.ompIntegration === "missing"}
		<div class="banner">
			<Alert severity="info" title="Enable omp integration">
				Atlas shows omp's state (working, needs you, done) through a small omp extension.
				<Button size="sm" onclick={() => agents.enableOmp()}>Enable</Button>
			</Alert>
		</div>
	{/if}
	{#if p.error}
		<div class="banner">
			<Alert severity="error" title="atlas.yaml has problems"
				><pre class="mono selectable">{p.error}</pre></Alert
			>
		</div>
	{/if}

	<div class="rule"></div>

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

	<Dialog
		bind:open={removeOpen}
		title="Remove {p.name}?"
		description="Only removes it from the sidebar. Files and running stacks are untouched."
		size="sm"
	>
		{#snippet actions()}
			<Button variant="ghost" onclick={() => (removeOpen = false)}>Cancel</Button>
			<Button
				variant="destructive"
				onclick={() => {
					removeOpen = false;
					store.run("rm", () => api.remove(p.id));
				}}>Remove</Button
			>
		{/snippet}
	</Dialog>
{/if}

<Dialog
	open={!!pending.value}
	onOpenChange={(o) => !o && (pending.value = null)}
	title={pending.value?.label}
	description={pending.value?.text}
	size="sm"
>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (pending.value = null)}>Cancel</Button>
		<Button
			variant="destructive"
			onclick={() => {
				const v = pending.value;
				pending.value = null;
				if (v) runAction(v.id, v.target, true);
			}}>Confirm</Button
		>
	{/snippet}
</Dialog>

<style>
	.drag-strip {
		height: 40px;
	}
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
	}
	/* One row, 40px: the same band as the macOS traffic lights (window y ≈ 8 + 20). The crumb
	   truncates first; the actions never wrap. */
	header {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 40px;
		flex: none;
		padding: 0 10px 0 10px;
		transition: padding-left var(--dur-slow) var(--ease);
	}
	/* Sidebar hidden: the content reaches the window's left edge, so keep clear of the traffic lights. */
	header.inset {
		padding-left: 70px;
	}
	header > :global(.crumbs) {
		flex: 0 1 auto;
		min-width: 0;
	}
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		align-self: center;
		padding: 0 8px 0 7px;
		height: 22px;
		border-radius: 10px;
		font-size: 11.5px;
		font-weight: 550;
		color: var(--c);
		background: color-mix(in oklab, var(--c) 13%, transparent);
		--c: var(--ca-text-secondary);
		transition:
			background 200ms ease-out,
			color 200ms ease-out;
	}
	.pill.t-ok {
		--c: var(--ca-semantic-success);
	}
	.pill.t-warn {
		--c: var(--ca-semantic-warning);
	}
	.pill.t-fail {
		--c: var(--ca-semantic-error);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--c);
	}
	.dot.pulse {
		animation: live 2.4s ease-out infinite;
	}
	@keyframes live {
		0% {
			box-shadow: 0 0 0 0 color-mix(in oklab, var(--c) 55%, transparent);
		}
		70%,
		100% {
			box-shadow: 0 0 0 5px transparent;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot.pulse {
			animation: none;
		}
	}
	.grow {
		flex: 1 1 24px;
		align-self: stretch;
		min-width: 24px;
	}
	.actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: 4px;
		white-space: nowrap;
	}
	.more,
	.split-arrow {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 7px;
		color: var(--ca-text-secondary);
	}
	.more {
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.more:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.split {
		display: flex;
		align-items: stretch;
	}
	.split.has-menu :global(.ca-btn) {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
	}
	.split-arrow {
		height: 100%;
		min-height: 28px;
		width: 24px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		border-radius: 0 8px 8px 0;
		box-shadow: inset 1px 0 0 color-mix(in oklab, var(--ca-brand-contrast) 25%, transparent);
	}
	.split-arrow:hover {
		background: var(--ca-brand-fill-hover);
	}
	.banner {
		padding: 0 20px 10px;
	}
	.banner pre {
		margin: 4px 0 0;
		font-size: 11.5px;
		white-space: pre-wrap;
	}
	.rule {
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
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
	.loading {
		display: flex;
		flex-direction: column;
		gap: 10px;
		align-items: center;
	}
	.docks,
	.ops {
		display: inline-flex;
		align-items: center;
		gap: 1px;
		padding: 2px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
	}
	.dock-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 24px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.dock-btn:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.dock-btn.on {
		color: var(--space-accent, var(--ca-brand-main));
		background: var(--ca-surface-raised);
		box-shadow:
			var(--ca-shadow-xs),
			0 0 0 0.5px var(--ca-outline-subtle);
	}
</style>
