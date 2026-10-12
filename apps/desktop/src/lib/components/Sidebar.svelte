<script lang="ts">
	import {
		IconButton,
		Kbd,
		Dialog,
		Button,
		Checkbox,
		toast,
		DropdownMenu,
		DropdownMenuItem,
		Skeleton,
		Spinner,
	} from "@codeatlasdev/ui";
	import {
		Plus,
		Pin,
		Settings,
		Search,
		Sparkles,
		GitBranch,
		ChevronRight,
		Ellipsis,
		Bot,
		LayoutDashboard,
		X,
		PanelLeftClose,
	} from "@lucide/svelte";
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import SpaceDots from "#lib/shell/SpaceDots.svelte";
	import IdentityDialog from "#lib/components/IdentityDialog.svelte";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { swipe } from "#lib/shell/swipe.ts";
	import { spaceSwipe } from "#lib/shell/gestures.ts";
	import { toggleSidebar } from "#lib/stores/docks.svelte.ts";
	import { pr, rollup } from "#lib/stores/pr.svelte.ts";
	import { open as pickFolder } from "@tauri-apps/plugin-dialog";
	import { onMount } from "svelte";
	import { register, shortcutFor } from "#lib/keymap.svelte.ts";
	import { promptDialog } from "#lib/stores/ask.svelte.ts";
	import Tip from "#lib/components/Tip.svelte";
	import { api } from "#lib/api.ts";
	import { store, health } from "#lib/stores/projects.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import { agents, type AgentState } from "#lib/stores/agents.svelte.ts";
	import type { ProjectView } from "#lib/api.ts";
	import type { Worktree } from "#lib/stores/workspace.svelte.ts";

	let { onJump, onAgent }: { onJump?: () => void; onAgent?: (worktreeId: string) => void } = $props();
	let filter = $state("");
	let showArchived = $state(false);
	let hovered = $state<string | null>(null);
	let multi = $state<Record<string, boolean>>({});
	let deleteIds = $state<string[]>([]);
	let deleteForce = $state(false);

	let repos = $derived(store.sorted.filter((p) => !p.parent));
	let activeRepo = $derived(repos.find((r) => r.id === shell.activeId) ?? null);
	let others = $derived(repos.filter((r) => r.id !== shell.activeId && (!filter.trim() || repoMatches(r))));
	/** Linked worktrees of a repo that pass the filter; a repo with none shows no children at all. */
	const childrenOf = (repoId: string) => children(repoId).filter((p) => matches(p, ws.worktree(p.id)));
	let activeKids = $derived(activeRepo ? childrenOf(activeRepo.id) : []);
	/** "Other projects" is collapsed by default; a filter opens it so matches are never hidden. */
	let allOpen = $state(false);
	let listOpen = $derived(allOpen || !!filter.trim());
	let openRepos = $state<Record<string, boolean>>({});
	let workingCount = $derived(
		ws.doc.tabs.filter((t) => t.agent && agents.status[t.id]?.state === "working").length,
	);
	const children = (repoId: string) =>
		store.sorted.filter((p) => p.parent === repoId).sort((a, b) => Number(b.pinned) - Number(a.pinned));

	let hasArchived = $derived(ws.doc.worktrees.some((w) => w.archived));
	/** Any row survives the filter (drives the "no match" hint). */
	let visible = $derived(repos.some(repoMatches));

	function repoMatches(r: ProjectView) {
		return [r, ...children(r.id)].some((p) => matches(p, ws.worktree(p.id)));
	}

	function matches(p: ProjectView, w: Worktree | undefined) {
		if (w?.archived && !showArchived) return false;
		const q = filter.trim().toLowerCase();
		return !q || [p.name, p.branch ?? "", w?.name ?? "", p.root].some((x) => x.toLowerCase().includes(q));
	}

	/** Strongest agent state among the worktree's terminal tabs. */
	function glyphOf(worktreeId: string): AgentState | null {
		const order: AgentState[] = ["blocked", "waiting", "working", "done", "idle"];
		const states = ws
			.tabsOf(worktreeId)
			.filter((t) => t.agent)
			.map((t) => agents.status[t.id]?.state ?? "idle");
		return order.find((s) => states.includes(s)) ?? null;
	}

	function select(e: MouseEvent, id: string) {
		if (e.metaKey) {
			multi[id] = !multi[id];
			return;
		}
		multi = {};
		shell.select(id);
	}

	function openTerminal(id: string) {
		store.selected = id;
		ws.select(id);
		void ws.newTerminal(id);
	}

	async function rename(id: string) {
		const w = ws.worktree(id);
		const v = w && (await promptDialog("Rename worktree", { value: w.name, confirmLabel: "Rename" }));
		if (w && v) w.name = v;
	}

	function askDelete(ids: string[]) {
		deleteIds = ids.filter((id) => store.projects.find((p) => p.id === id)?.parent);
		deleteForce = false;
	}

	async function confirmDelete() {
		const ids = deleteIds;
		deleteIds = [];
		multi = {};
		for (const id of ids) await worktrees.remove(id, deleteForce);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.metaKey && e.shiftKey && e.key === "Backspace") {
			const sel = Object.keys(multi).filter((k) => multi[k]);
			const ids = sel.length ? sel : hovered ? [hovered] : [];
			if (ids.length) {
				e.preventDefault();
				askDelete(ids);
			}
		}
	}

	let discoverOpen = $state(false);
	let found = $state<string[]>([]);
	let chosen = $state<Record<string, boolean>>({});

	async function addProject() {
		const dir = await pickFolder({ directory: true, title: "Add atlas project" });
		if (typeof dir !== "string") return;
		const p = await store.run("add", () => api.add(dir));
		if (p) {
			await store.refresh();
			store.selected = p.id;
		}
	}

	async function discover() {
		const paths = await store.run("discover", () => api.discover());
		if (!paths?.length) {
			if (paths) toast.info("No new atlas projects found");
			return;
		}
		found = paths;
		chosen = Object.fromEntries(paths.map((p) => [p, true]));
		discoverOpen = true;
	}

	async function addChosen() {
		discoverOpen = false;
		for (const p of found.filter((p) => chosen[p])) {
			await store.run("add", () => api.add(p));
		}
		await store.refresh();
		if (!store.selected) store.selected = store.sorted[0]?.id ?? null;
	}

	onMount(() =>
		register([
			{ id: "atlas::AddProject", label: "Add project…", run: () => void addProject() },
			{ id: "atlas::DiscoverProjects", label: "Find projects on this machine", run: () => void discover() },
		]),
	);

	function toggle(id: string, running: boolean) {
		store.run(`stack:${id}`, () => (running ? api.stop(id) : api.start(id)));
	}
</script>

<svelte:window {onkeydown} />

<aside class="sidebar" data-tour="sidebar" use:swipe={spaceSwipe()}>
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
	</div>
	<div class="filter">
		<div class="search">
			<Search size={12} />
			<input
				placeholder="Filter worktrees"
				bind:value={filter}
				onkeydown={(e) => e.key === "Escape" && ((filter = ""), e.currentTarget.blur())}
			/>
			{#if filter}
				<button class="clear" aria-label="Clear filter" onclick={() => (filter = "")}><X size={11} /></button>
			{/if}
		</div>
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
	<nav class="list">
		<button class="item fixed" class:active={ws.view === "agents"} onclick={() => (ws.view = "agents")}>
			<Bot size={13} /><span class="name">Agents</span>
			{#if workingCount}<span class="count">{workingCount}</span>{/if}
		</button>
		<button class="item fixed" class:active={ws.view === "dashboard"} onclick={() => (ws.view = "dashboard")}>
			<LayoutDashboard size={13} /><span class="name">Dashboard</span>
		</button>
		{#if activeRepo}
			{@render projectHead(activeRepo, true)}
			{#if activeKids.length}
				{#each [activeRepo, ...activeKids] as p (p.id)}{@render row(p)}{/each}
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
					{@render projectHead(r, false)}
					{#if openRepos[r.id] || filter.trim()}
						{#each childrenOf(r.id) as p (p.id)}{@render row(p)}{/each}
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
		{:else if filter.trim() && !visible}
			<p class="hint">No worktree matches “{filter.trim()}”.</p>
		{/if}
	</nav>
	{#if hasArchived}
		<label class="archived"><input type="checkbox" bind:checked={showArchived} /> Show archived</label>
	{/if}
	<footer>
		{#if updates.pill}
			<button class="pill" onclick={updates.showWhatsNew} aria-label="Update to {updates.release?.version}">
				<Sparkles size={13} />
				<span>Update to {updates.release?.version}</span>
			</button>
		{/if}
		<SpaceDots />
		<button class="add" onclick={addProject}><Plus size={14} /> Add project…</button>
		<div class="tools">
			<Tip label="Find projects" side="top">
				{#snippet child(props)}
					<IconButton {...props} variant="text" size="sm" aria-label="Find projects" onclick={discover}>
						<Search size={14} />
					</IconButton>
				{/snippet}
			</Tip>
			<Tip label="Settings" command="zed::OpenSettings" side="top">
				{#snippet child(props)}
					<IconButton
						{...props}
						variant="text"
						size="sm"
						aria-label="Settings"
						onclick={() => updates.show()}
					>
						<Settings size={14} />
					</IconButton>
				{/snippet}
			</Tip>
			<Kbd>{shortcutFor("atlas::Jump") ?? "⌘K"}</Kbd>
		</div>
	</footer>
</aside>

{#snippet creating(repoId: string)}
	{#each Object.values(worktrees.creations).filter((c) => c.repoId === repoId) as c (c.id)}
		<div class="item creating nested">
			<span class="glyph"><Spinner size={11} /></span>
			<span class="name">{c.name}<small class="branch">{c.step}…</small></span>
		</div>
	{/each}
{/snippet}

{#snippet menu(p: ProjectView, w: Worktree | undefined)}
	<DropdownMenuItem onclick={() => openTerminal(p.id)}>New terminal</DropdownMenuItem>
	<DropdownMenuItem onclick={() => onAgent?.(p.id)}>New agent</DropdownMenuItem>
	<DropdownMenuItem onclick={() => toggle(p.id, p.running)}>
		{p.running ? "Stop stack" : "Start stack"}
	</DropdownMenuItem>
	<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}
		>Open in editor</DropdownMenuItem
	>
	<DropdownMenuItem onclick={() => store.run("reveal", () => api.reveal(p.id))}
		>Reveal in Finder</DropdownMenuItem
	>
	<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
		{p.pinned ? "Unpin" : "Pin to top"}
	</DropdownMenuItem>
	{#if !p.parent}
		<DropdownMenuItem onclick={() => (identities.editing = p.id)}>Change icon and color…</DropdownMenuItem>
	{/if}
	{#if w}
		<DropdownMenuItem onclick={() => (w.unread = !w.unread)}>
			{w.unread ? "Mark as read" : "Mark as unread"}
		</DropdownMenuItem>
	{/if}
	{#if p.parent && w}
		<DropdownMenuItem onclick={() => rename(p.id)}>Rename</DropdownMenuItem>
		<DropdownMenuItem onclick={() => (w.archived = !w.archived)}>
			{w.archived ? "Unarchive" : "Archive"}
		</DropdownMenuItem>
		<DropdownMenuItem variant="destructive" onclick={() => askDelete([p.id])}>Delete…</DropdownMenuItem>
	{/if}
{/snippet}

<!-- A project: its icon, name, live status. The active one also acts as its own `main` row
     when it has no other worktrees (a lone "main" child would be redundant). -->
{#snippet projectHead(r: ProjectView, active: boolean)}
	{@const live = store.live[r.id]}
	{@const h = health(r, live)}
	{@const kids = children(r.id)}
	{@const solo = kids.length === 0}
	{@const w = ws.worktree(r.id)}
	{@const up = live?.services.filter((s) => s.state === "running").length ?? 0}
	<div
		class="project"
		class:current={active}
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
		onmouseenter={() => (hovered = r.id)}
		onmouseleave={() => hovered === r.id && (hovered = null)}
		onclick={() => (active && !solo ? shell.select(r.id) : shell.switchTo(r.id))}
		onkeydown={(e) => e.key === "Enter" && (active && !solo ? shell.select(r.id) : shell.switchTo(r.id))}
	>
		{#if active && !solo}
			<span class="twist" aria-hidden="true"></span>
		{:else if !active && kids.length}
			<button
				class="twist"
				aria-label="{openRepos[r.id] ? 'Hide' : 'Show'} worktrees of {r.name}"
				aria-expanded={!!openRepos[r.id]}
				onclick={(e) => {
					e.stopPropagation();
					openRepos[r.id] = !openRepos[r.id];
				}}
			>
				<ChevronRight size={11} class="chev {openRepos[r.id] ? 'open' : ''}" />
			</button>
		{:else}
			<span class="twist" aria-hidden="true"></span>
		{/if}
		<span class="ico">
			<ProjectIcon id={r.id} size={20} ring={active} />
			<i class="dot {h}" class:pulse={r.running} title={r.running ? "Running" : "Stopped"}></i>
		</span>
		<span class="pname">{r.name}</span>
		{#if r.host}<span class="chip">{r.host.replace(/^ssh:/, "")}</span>{/if}
		{#if r.pinned}<Pin size={10} class="pin" />{/if}
		<span class="meta">
			{#if r.running}<span class="count">{up}/{live?.services.length || r.services_total}</span>{/if}
			{#if !active && kids.length}<span class="count">{kids.length + 1} wt</span>{/if}
		</span>
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
		<DropdownMenu align="end">
			{#snippet trigger()}
				<span class="hover-btn" aria-label="Project menu"><Ellipsis size={13} /></span>
			{/snippet}
			{@render menu(r, w)}
		</DropdownMenu>
	</div>
{/snippet}

{#snippet row(p: ProjectView)}
	{@const w = ws.worktree(p.id)}
	{@const live = store.live[p.id]}
	{@const up = live?.services.filter((s) => s.state === "running").length ?? 0}
	{@const glyph = glyphOf(p.id)}
	{#if matches(p, w)}
		<div
			class="item nested"
			class:active={ws.view === "worktree" && store.selected === p.id}
			class:multi={multi[p.id]}
			class:unread={w?.unread}
			class:main={!p.parent}
			role="button"
			tabindex="0"
			onmouseenter={() => (hovered = p.id)}
			onmouseleave={() => hovered === p.id && (hovered = null)}
			onclick={(e) => select(e, p.id)}
			onkeydown={(e) => e.key === "Enter" && (store.selected = p.id)}
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
			{#if p.port_offset}<span class="chip">+{p.port_offset}</span>{/if}
			{#if p.parent && p.pinned}<Pin size={11} class="pin" />{/if}
			<span class="meta">
				{#if p.running}<span class="count">{up}/{live?.services.length || p.services_total}</span>{/if}
			</span>
			<DropdownMenu align="end">
				{#snippet trigger()}
					<span class="hover-btn" aria-label="Worktree menu"><Ellipsis size={13} /></span>
				{/snippet}
				{@render menu(p, w)}
			</DropdownMenu>
		</div>
	{/if}
{/snippet}

<Dialog
	open={deleteIds.length > 0}
	onOpenChange={(o) => !o && (deleteIds = [])}
	title={deleteIds.length > 1 ? `Delete ${deleteIds.length} worktrees?` : "Delete worktree?"}
	description="Removes the checkout and its terminals. Unmerged branches are kept unless forced."
	size="sm"
>
	<Checkbox bind:checked={deleteForce} label="Force (discard changes, delete unmerged branch)" />
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (deleteIds = [])}>Cancel</Button>
		<Button variant="destructive" onclick={confirmDelete}>Delete</Button>
	{/snippet}
</Dialog>

<IdentityDialog />

<Dialog
	bind:open={discoverOpen}
	title="Found atlas projects"
	description="Add them to the sidebar."
	size="lg"
>
	<div class="found">
		{#each found as p (p)}
			<Checkbox bind:checked={chosen[p]} label={p} />
		{/each}
	</div>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (discoverOpen = false)}>Not now</Button>
		<Button onclick={addChosen}>Add selected</Button>
	{/snippet}
</Dialog>

<style>
	.sidebar {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		/* Native vibrancy is behind; just a hint of tint for legibility. */
		background: color-mix(in oklab, var(--ca-glass-tint-ultra-thin) 40%, transparent);
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
	.drag :global(.hide-btn) {
		opacity: 0;
		transition: opacity var(--dur) var(--ease);
	}
	.sidebar:hover .drag :global(.hide-btn),
	.drag :global(.hide-btn:focus-visible) {
		opacity: 1;
	}
	.filter {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0 12px 8px;
	}
	.search {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		height: 26px;
		padding: 0 6px 0 8px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: var(--ca-text-secondary);
		transition:
			border-color 120ms ease-out,
			background 120ms ease-out;
	}
	.search:focus-within {
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.search input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12px;
		padding: 0;
		border: 0;
		outline: none;
		background: transparent;
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.clear {
		display: inline-grid;
		place-items: center;
		width: 16px;
		height: 16px;
		border: 0;
		border-radius: 50%;
		background: color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
		color: var(--ca-text-secondary);
	}
	.clear:hover {
		color: var(--ca-text-primary);
	}
	.jump {
		border: 0;
		background: none;
		padding: 0;
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
	.project :global(.chev),
	.section :global(.chev) {
		transition: transform var(--dur-fast) var(--ease);
	}
	.project :global(.chev.open),
	.section :global(.chev.open) {
		transform: rotate(90deg);
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
	.project .meta {
		display: inline-flex;
		gap: 6px;
	}
	.project:hover .meta {
		display: none;
	}
	.project:hover .hover-btn,
	.project:focus-within .hover-btn {
		display: inline-flex;
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
	.item.nested {
		margin-left: 22px;
		height: 28px;
	}
	.glyph {
		width: 14px;
		display: inline-flex;
		justify-content: center;
		color: var(--ca-text-disabled);
		font-size: 11px;
		font-weight: 700;
	}
	.branch {
		margin-left: 6px;
		font-size: 10.5px;
		font-weight: 400;
		color: var(--ca-text-secondary);
	}
	.chip {
		font-size: 10px;
		padding: 0 5px;
		border-radius: 4px;
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
		color: var(--ca-text-secondary);
		white-space: nowrap;
	}
	.chip.bad {
		background: color-mix(in oklab, var(--ca-semantic-error, #d33) 20%, transparent);
		color: var(--ca-semantic-error, #d33);
	}
	.item.unread .name {
		font-weight: 750;
	}
	.item.multi {
		box-shadow: inset 0 0 0 1px var(--ca-brand-main);
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
		opacity: 0.7;
	}
	.archived {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 18px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.list {
		flex: 1;
		overflow-y: auto;
		padding: 0 10px;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.item {
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
	.item:hover {
		background: var(--ca-vibrancy-fill);
	}
	.item.active {
		background: color-mix(in oklab, var(--space-accent) 16%, transparent);
		box-shadow: inset 2px 0 0 var(--space-accent);
	}
	.item:focus-visible {
		box-shadow: 0 0 0 2px var(--ca-action-focus);
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 500;
	}
	.item :global(.pin) {
		color: var(--ca-text-secondary);
	}
	.count {
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
	}
	.hover-btn {
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
	.hover-btn:hover {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.item:hover .hover-btn {
		display: inline-flex;
	}
	.item:hover .meta {
		display: none;
	}
	footer {
		padding: 10px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-top: 0.5px solid var(--ca-vibrancy-separator);
	}
	.pill {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 7px;
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 999px;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		color: var(--ca-brand-main);
		animation: rise 260ms var(--ca-ease-spring, ease-out);
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-raised));
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-brand-main) 45%, transparent),
			0 0 14px -2px color-mix(in oklab, var(--ca-brand-main) 45%, transparent);
		transition:
			transform 120ms ease-out,
			box-shadow 160ms ease-out;
	}
	.pill:hover {
		box-shadow:
			0 0 0 0.5px var(--ca-brand-main),
			0 0 18px -1px color-mix(in oklab, var(--ca-brand-main) 60%, transparent);
	}
	.pill:active {
		transform: scale(0.98);
	}
	@media (prefers-reduced-motion: no-preference) {
		.pill {
			animation:
				rise 260ms var(--ca-ease-spring, ease-out),
				glow 3.2s ease-in-out 600ms infinite;
		}
	}
	@keyframes glow {
		50% {
			box-shadow:
				0 0 0 0.5px color-mix(in oklab, var(--ca-brand-main) 55%, transparent),
				0 0 20px 0 color-mix(in oklab, var(--ca-brand-main) 55%, transparent);
		}
	}
	@keyframes rise {
		from {
			transform: translateY(6px);
			opacity: 0;
		}
	}
	.add {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 30px;
		padding: 0 8px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}
	.add:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.tools {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
	}
	.tools :global(.ca-kbd) {
		margin-left: auto;
	}
	.found {
		display: flex;
		flex-direction: column;
		gap: 10px;
		max-height: 50vh;
		overflow-y: auto;
		font-family: var(--mono);
		font-size: 12px;
	}
</style>
