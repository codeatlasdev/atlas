<script lang="ts">
	import { EmptyState, Card, Select } from "@codeatlasdev/ui";
	import { Bot, LayoutDashboard } from "@lucide/svelte";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import { agents, type AgentState } from "#lib/stores/agents.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	let { mode }: { mode: "agents" | "dashboard" } = $props();
	/** `all` = no filter (the kit Select has no empty value). */
	let repo = $state("all");
	let stateFilter = $state<AgentState | "all">("all");

	const COLUMNS: { title: string; states: AgentState[] }[] = [
		{ title: "Needs you", states: ["blocked", "waiting"] },
		{ title: "Working", states: ["working"] },
		{ title: "Done", states: ["done"] },
		{ title: "Idle", states: ["idle"] },
	];

	let panes = $derived(
		ws.doc.tabs
			.filter((t) => t.agent && t.pty)
			.map((t) => ({ tab: t, st: agents.status[t.id], w: ws.worktree(t.worktreeId) }))
			.filter((x) => x.w),
	);
	let feed = $derived(
		ws.doc.feed.filter(
			(e) =>
				(repo === "all" || ws.worktree(e.worktreeId)?.repoId === repo) &&
				(stateFilter === "all" || e.state === stateFilter),
		),
	);
	let running = $derived(panes.filter((p) => p.st?.state === "working"));
	const repos = $derived(store.projects.filter((p) => !p.parent));

	function focus(tabId: string) {
		const t = ws.tab(tabId);
		if (!t) return;
		ws.view = "worktree";
		store.selected = t.worktreeId;
		ws.select(t.worktreeId);
		ws.activate(t.id);
	}

	const age = (ts: number) => {
		const s = Math.max(0, Math.floor((Date.now() - ts) / 1000));
		return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h`;
	};
	const where = (worktreeId: string) => {
		const w = ws.worktree(worktreeId);
		return w
			? `${store.projects.find((p) => p.id === w.repoId)?.name ?? ""} / ${w.isMain ? "main" : w.name}`
			: "?";
	};
</script>

<header data-tauri-drag-region>
	<h1>{mode === "agents" ? "Agents" : "Dashboard"}</h1>
	{#if panes.length}
		<span class="sub">{running.length} working · {panes.length} total</span>
	{/if}
	{#if mode === "agents"}
		<div class="filters">
			<Select
				size="sm"
				value={repo}
				options={[
					{ value: "all", label: "All projects" },
					...repos.map((r) => ({ value: r.id, label: r.name })),
				]}
				onchange={(v) => (repo = v)}
			/>
			<Select
				size="sm"
				value={stateFilter}
				options={[
					{ value: "all", label: "All states" },
					...["working", "waiting", "blocked", "done", "idle"].map((s) => ({ value: s, label: s })),
				]}
				onchange={(v) => (stateFilter = v as AgentState | "all")}
			/>
		</div>
	{/if}
</header>

{#snippet none(title: string, filtered = false)}
	<div class="none">
		<EmptyState
			icon={mode === "agents" ? Bot : LayoutDashboard}
			{title}
			description={filtered
				? "Nothing matches these filters."
				: !store.projects.length
					? "Add a project first. Agents run inside its worktrees and report back here."
					: !ws.selected
						? "Pick a worktree in the sidebar, then start one. Its progress shows up here."
						: "Start one in this worktree and its progress shows up here."}
		>
			{#snippet actions()}
				{#if filtered}
					<button class="start" onclick={() => ((repo = "all"), (stateFilter = "all"))}>Clear filters</button>
				{:else if !store.projects.length}
					<button class="start" onclick={() => runCommand("atlas::AddProject")}>Add a project</button>
				{:else if !ws.selected}
					<button class="start" onclick={() => (ws.view = "worktree")}>Choose a worktree</button>
				{:else}
					<button class="start" onclick={() => runCommand("atlas::NewAgentSession")}>
						New agent session <kbd>{shortcutFor("atlas::NewAgentSession") ?? ""}</kbd>
					</button>
				{/if}
			{/snippet}
		</EmptyState>
	</div>
{/snippet}

{#if mode === "dashboard"}
	{#if !panes.length}
		{@render none("No agents yet")}
	{:else}
		<div class="board">
			{#each COLUMNS as col (col.title)}
				{@const items = panes.filter((p) => col.states.includes(p.st?.state ?? "idle"))}
				<section>
					<h2>{col.title} <span>{items.length}</span></h2>
					{#each items as p (p.tab.id)}
						<button class="card" onclick={() => focus(p.tab.id)}>
							<Card material="glass" p="3">
								<div class="row">
									<AgentGlyph state={p.st?.state ?? "idle"} /><AgentLogo agent={p.tab.agent} size={12} /><b
										>{p.tab.agent}</b
									><span class="dim">{where(p.tab.worktreeId)}</span>
								</div>
								{#if p.st?.prompt}<p class="prompt">{p.st.prompt}</p>{/if}
								{#if p.st?.last_message}<p class="msg">{p.st.last_message}</p>{/if}
								<div class="dim">{age(p.st?.state_started_at ?? p.tab.createdAt)} in state</div>
							</Card>
						</button>
					{:else}
						<div class="col-empty">Nothing here</div>
					{/each}
				</section>
			{/each}
		</div>
	{/if}
{:else if !running.length && !feed.length}
	{@render none(
		repo !== "all" || stateFilter !== "all" ? "No activity matches" : "No agent activity yet",
		repo !== "all" || stateFilter !== "all",
	)}
{:else}
	<div class="feed">
		{#each running as p (p.tab.id)}
			<button class="line pinned" onclick={() => focus(p.tab.id)}>
				<AgentGlyph state="working" /><AgentLogo agent={p.tab.agent} size={12} /><b>{p.tab.agent}</b><span
					>{where(p.tab.worktreeId)}</span
				>
				<span class="dim">{p.st?.prompt ?? ""}</span>
			</button>
		{/each}
		{#each feed as e, i (i + ":" + e.ts)}
			<button class="line" onclick={() => focus(e.tabId)}>
				<AgentGlyph state={e.state as AgentState} />
				<span class="time">{new Date(e.ts).toLocaleTimeString()}</span>
				<AgentLogo agent={e.agent} size={12} /><b>{e.agent}</b>
				<span class="where">{where(e.worktreeId)}</span>
				<span class="state">{e.state}</span>
				{#if e.text}<span class="dim">{e.text}</span>{/if}
			</button>
		{/each}
	</div>
{/if}

<style>
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 16px 20px 12px;
	}
	h1 {
		margin: 0;
		font-size: 22px;
		font-weight: 700;
		letter-spacing: -0.02em;
	}
	.sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.filters {
		margin-left: auto;
		display: flex;
		gap: 8px;
	}
	.none {
		flex: 1;
		display: grid;
		place-items: center;
	}
	.start {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 30px;
		padding: 0 12px;
		border: 0;
		border-radius: 8px;
		font: inherit;
		font-weight: 600;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		transition: background 120ms ease-out;
	}
	.start:hover {
		background: var(--ca-brand-fill-hover);
	}
	.start kbd {
		font: inherit;
		font-size: 11px;
		color: inherit;
	}
	.col-empty {
		padding: 14px 0;
		text-align: center;
		font-size: 12px;
		color: var(--ca-text-secondary);
		border: 0.5px dashed var(--ca-outline-subtle);
		border-radius: 10px;
	}
	.where {
		white-space: nowrap;
	}
	.board {
		flex: 1;
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 12px;
		padding: 0 16px 16px;
		overflow: auto;
	}
	section {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
	}
	h2 {
		font-size: 12px;
		font-weight: 650;
		color: var(--ca-text-secondary);
		margin: 0 0 4px;
	}
	h2 span {
		color: var(--ca-text-secondary);
	}
	.card {
		border: 0;
		padding: 0;
		background: none;
		text-align: left;
		font: inherit;
		color: inherit;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12.5px;
		min-width: 0;
	}
	.prompt,
	.msg {
		margin: 6px 0 0;
		font-size: 12px;
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.msg {
		color: var(--ca-text-secondary);
	}
	.dim {
		font-size: 11px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.feed {
		flex: 1;
		overflow: auto;
		padding: 0 14px 16px;
	}
	.line {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 6px 8px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 12.5px;
		color: inherit;
		text-align: left;
	}
	.line {
		transition: background 120ms ease-out;
	}
	.line:hover {
		background: var(--ca-action-hover);
	}
	.card {
		transition: transform 120ms ease-out;
	}
	.card:active {
		transform: scale(0.99);
	}
	.line.pinned {
		background: color-mix(in oklab, var(--ca-brand-main) 8%, transparent);
	}
	.time {
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
	.state {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
</style>
