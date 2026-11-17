<script lang="ts">
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { invoke } from "@tauri-apps/api/core";
	import { EmptyState, Select, Skeleton, toast } from "@codeatlasdev/ui";
	import {
		Play,
		Copy,
		FolderSearch,
		Globe,
		RefreshCw,
		PanelRightClose,
		Pin,
		Bot,
		ExternalLink,
	} from "@lucide/svelte";
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { ago } from "#lib/components/sidebar/discovery.ts";
	import { useNow } from "#lib/clock.svelte.ts";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import { docks, peek, toggleDock } from "#lib/stores/docks.svelte.ts";
	import { liveAgents, portRows, STATE_LABEL } from "#lib/shell/rightPanel.ts";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { resumeCommand } from "#lib/agentRegistry.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import { host } from "#lib/host.ts";

	interface Session {
		agent: string;
		id: string;
		/** Value for the agent's resume flag (omp: file, claude/codex: id). */
		resume: string;
		file: string;
		cwd: string;
		title: string | null;
		first_prompt: string | null;
		mtime: number;
		size: number;
	}
	interface Port {
		port: number;
		pid: number;
		command: string;
	}

	let tab = $state<"agents" | "ports">("agents");
	let scope = $state<"workspace" | "project" | "all">("workspace");
	let query = $state("");
	let sessions = $state<Session[]>([]);
	let open = $state<string | null>(null);
	let ports = $state<Port[]>([]);
	let loading = $state(false);

	const space = useSpace();
	let w = $derived(space.worktree);

	function cwds(): string[] {
		if (!w || scope === "all") return [];
		if (scope === "workspace") return [w.path];
		return store.projects.filter((p) => p.id === w.repoId || p.parent === w.repoId).map((p) => p.root);
	}

	async function loadSessions() {
		loading = true;
		try {
			sessions = await invoke<Session[]>("agent_sessions", { cwds: cwds(), query: query || null });
		} finally {
			loading = false;
		}
	}

	async function loadPorts() {
		if (!w) return;
		if (w.hostId) {
			// SSH host: every listening TCP port there (process names when visible).
			const r = await host.exec(w.path, ["sh", "-c", "ss -Hltnp 2>/dev/null"]).catch(() => null);
			ports = (r?.stdout ?? "")
				.split("\n")
				.map((l) => {
					const port = Number(l.split(/\s+/)[3]?.split(":").pop());
					const cmd = l.match(/users:\(\("([^"]+)",pid=(\d+)/);
					return { port, pid: Number(cmd?.[2] ?? 0), command: cmd?.[1] ?? "" };
				})
				.filter((p, i, a) => p.port > 0 && a.findIndex((x) => x.port === p.port) === i)
				.sort((a, b) => a.port - b.port);
			return;
		}
		// Refresh the terminal list first: a terminal opened since the last sync is not in `ws.ptys` yet.
		await ws.syncPtys();
		const pids = Object.values(ws.ptys)
			.filter((p) => p.scope.worktree_id === w.id && p.pid)
			.map((p) => p.pid!);
		ports = pids.length ? await invoke<Port[]>("listening_ports", { pids }) : [];
	}

	/** On screen: pinned as a dock, or peeking from the edge. A hidden panel reads nothing until it is shown. */
	let shown = $derived(docks.right || peek.right);

	$effect(() => {
		void [scope, w?.id, tab];
		if (!shown) return;
		if (tab === "agents") void loadSessions();
		else void loadPorts();
	});

	// Search as you type, after a short pause.
	let typing: number | undefined;
	function onSearch() {
		clearTimeout(typing);
		typing = window.setTimeout(loadSessions, 250);
	}
	$effect(() => () => clearTimeout(typing));

	function resume(s: Session, chat = false) {
		const target = store.projects.find((p) => p.root === s.cwd)?.id ?? w?.id;
		if (!target) return;
		ws.view = "worktree";
		store.selected = target;
		ws.select(target);
		void agents.launch(target, s.agent, { resume: s.resume, chat });
	}

	let forwards = $state<Record<number, number>>({});

	/** In the app's browser, or in the system browser (`external`); remote ports go through an ssh -L forward first. */
	async function openPort(port: number, external = false) {
		if (!w) return;
		let local = port;
		if (w.hostId) {
			try {
				local = forwards[port] ??= await invoke<number>("ssh_forward", {
					alias: w.hostId.replace(/^ssh:/, ""),
					port,
				});
			} catch (e) {
				toast.error(errText(e));
				return;
			}
		}
		void openPreview(w.id, `http://localhost:${local}`, external);
	}

	// Reading `clk.now` re-renders the labels on each tick; the age itself comes from the real clock (the stamp is only
	// refreshed on a tick, so it can be older than the render).
	const clk = useNow(30_000);
	const age = (ms: number | null) => (void clk.now, ms ? ago(ms / 1000) : "");
	const when = (ts: number) => (void clk.now, ago(ts));
	const resumeCommandOf = (s: Session) => resumeCommand(s.agent, s.resume);

	let live = $derived(w ? liveAgents(ws.doc.tabs, agents.status, w.id) : []);
	let rows = $derived(
		w ? portRows(ports, store.configs[w.id]?.services ?? [], store.live[w.id]?.services ?? []) : [],
	);
</script>

<aside class="right">
	<div class="tabs" data-tauri-drag-region>
		<button class:on={tab === "agents"} onclick={() => (tab = "agents")}>Agents</button>
		<button class:on={tab === "ports"} onclick={() => (tab = "ports")}>Ports</button>
		<button
			class="icon"
			aria-label="Refresh"
			onclick={() => (tab === "agents" ? loadSessions() : loadPorts())}
		>
			<RefreshCw size={12} />
		</button>
		<Tip
			label={docks.right ? "Hide right sidebar" : "Pin right sidebar"}
			command="workspace::ToggleRightDock"
		>
			{#snippet child(props)}
				<button
					{...props}
					class="hide"
					aria-label={docks.right ? "Hide right sidebar" : "Pin right sidebar"}
					onclick={() => toggleDock("right")}
				>
					{#if docks.right}<PanelRightClose size={13} />{:else}<Pin size={12} />{/if}
				</button>
			{/snippet}
		</Tip>
	</div>
	{#if tab === "agents"}
		<div class="controls">
			<input
				placeholder="Search sessions"
				aria-label="Search sessions"
				bind:value={query}
				oninput={onSearch}
				onkeydown={(e) => e.key === "Enter" && (clearTimeout(typing), loadSessions())}
			/>
			<Select
				class="scope"
				size="sm"
				aria-label="Show sessions of"
				value={scope}
				options={[
					{ value: "workspace", label: "Workspace" },
					{ value: "project", label: "Project" },
					{ value: "all", label: "All" },
				]}
				onchange={(v) => (scope = v as typeof scope)}
			/>
		</div>
		<div class="list">
			{#if live.length}
				<h3 class="head">Running here</h3>
				{#each live as a (a.tabId)}
					<button class="row live" data-live-agent={a.tabId} onclick={() => ws.activate(a.tabId)}>
						<span class="l1">
							<AgentGlyph state={a.state} />
							<span class="agent"><AgentLogo agent={a.agent} size={10} />{a.agent}</span>
							<span class="state {a.state}">{STATE_LABEL[a.state]}</span>
							<span class="dim">{age(a.since)}</span>
						</span>
						{#if a.detail}<span class="dim detail-line">{a.detail}</span>{/if}
					</button>
				{/each}
				{#if sessions.length}<h3 class="head">History</h3>{/if}
			{/if}
			{#each sessions as s (s.file)}
				<button
					class="row"
					class:open={open === s.file}
					onclick={() => (open = open === s.file ? null : s.file)}
				>
					<span class="agent"><AgentLogo agent={s.agent} size={10} />{s.agent}</span>
					<span class="title">{s.title ?? s.first_prompt ?? s.id}</span>
					<span class="dim">{when(s.mtime)}</span>
				</button>
				{#if open === s.file}
					<div class="detail">
						<div class="dim mono">{s.cwd}</div>
						{#if s.first_prompt}<p>{s.first_prompt}</p>{/if}
						<div class="acts">
							<button onclick={() => resume(s)}><Play size={12} /> Resume</button>
							{#if s.agent === "omp"}<button onclick={() => resume(s, true)}
									><Play size={12} /> Resume in native chat</button
								>{/if}
							<button
								onclick={() =>
									navigator.clipboard.writeText(resumeCommandOf(s)).then(
										() => toast.success("Copied"),
										(e) => toast.error(errText(e)),
									)}><Copy size={12} /> Copy command</button
							>
							<button onclick={() => api.revealPath(s.file).catch((e) => toast.error(errText(e)))}
								><FolderSearch size={12} /> Reveal</button
							>
						</div>
					</div>
				{/if}
			{:else}
				{#if loading}
					<div class="skel" aria-busy="true">
						{#each [0, 1, 2, 3] as i (i)}<Skeleton variant="rounded" height={26} />{/each}
					</div>
				{:else if query}
					<p class="empty">No sessions match “{query}”.</p>
				{:else if live.length}
					<p class="empty">No earlier sessions {scope === "workspace" ? "in this worktree" : "here"} yet.</p>
				{:else}
					<EmptyState
						icon={Bot}
						title="No agent sessions yet"
						description="Sessions of agents you run in this worktree show up here, with what each one last did, so you can resume them later.{scope ===
						'workspace'
							? ' Choose Project or All to see sessions from other worktrees.'
							: ''}"
					>
						{#snippet actions()}
							<button class="start" onclick={() => runCommand("atlas::NewAgentSession")}>
								New agent session <kbd>{shortcutFor("atlas::NewAgentSession") ?? ""}</kbd>
							</button>
						{/snippet}
					</EmptyState>
				{/if}
			{/each}
		</div>
	{:else}
		<div class="list">
			{#each rows as r (r.port)}
				<div class="port" data-port={r.port}>
					<button class="row" onclick={() => openPort(r.port)}>
						<Globe size={12} /><span class="title">:{r.port}</span>
						{#if r.service}<span class="agent">{r.service}</span>{/if}
						{#if r.process}<span class="dim">{r.process}</span>{/if}
					</button>
					<button
						class="ext"
						aria-label="Open :{r.port} in the system browser"
						title="Open in the system browser"
						onclick={() => openPort(r.port, true)}><ExternalLink size={12} /></button
					>
				</div>
			{:else}
				<EmptyState
					icon={Globe}
					title="No ports listening"
					description="A dev server or a service started from this worktree shows up here with its port and a button to open it. Run one in a terminal (for example bun run dev) or start a service from the project panel."
				/>
			{/each}
		</div>
	{/if}
</aside>

<style>
	.right {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.tabs {
		display: flex;
		align-items: center;
		gap: 4px;
		height: 32px;
		flex-shrink: 0;
		padding: 0 8px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	/* Windows: the caption buttons sit over this row's right end (WindowControls.svelte). */
	:global(html.custom-chrome) .tabs {
		padding-right: var(--caption-controls);
	}
	.skel {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 4px 6px;
	}
	.tabs button {
		border: 0;
		background: none;
		font: inherit;
		font-size: 12.5px;
		font-weight: 600;
		padding: 3px 8px;
		border-radius: 6px;
		color: var(--ca-text-secondary);
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.tabs button:hover {
		color: var(--ca-text-primary);
	}
	.tabs button.on {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.tabs .icon {
		margin-left: auto;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
	}
	/* The kit select is `width: 100%`, which in this row left the search field a few pixels: an empty square. */
	.controls :global(.scope) {
		flex: none;
		width: 104px;
	}
	.controls input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: inherit;
		outline: none;
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.controls input:focus {
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
	}
	.list {
		flex: 1;
		overflow-y: auto;
		padding: 0 6px 10px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 8px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 12px;
		color: inherit;
		text-align: left;
	}
	.row {
		transition: background 120ms ease-out;
	}
	.row:hover,
	.row.open {
		background: var(--ca-action-hover);
	}
	.agent {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 10px;
		padding: 0 4px;
		border-radius: 3px;
		background: var(--ca-action-hover);
		color: var(--ca-text-secondary);
	}
	.title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dim {
		font-size: 11px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.mono {
		font-family: ui-monospace, monospace;
	}
	.detail {
		padding: 4px 10px 10px;
		font-size: 12px;
	}
	.detail p {
		margin: 6px 0;
		white-space: pre-wrap;
	}
	.acts {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.acts button {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font: inherit;
		font-size: 11.5px;
		padding: 3px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: inherit;
	}
	.head {
		margin: 8px 8px 2px;
		font-size: 10.5px;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	.row.live {
		flex-direction: column;
		align-items: stretch;
		gap: 2px;
	}
	.l1 {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}
	.state {
		flex: 1;
		min-width: 0;
		font-size: 11.5px;
		font-weight: 600;
	}
	.state.waiting,
	.state.blocked {
		color: var(--ca-warning-main, var(--ca-brand-main));
	}
	.detail-line {
		padding-left: 20px;
	}
	.port {
		display: flex;
		align-items: center;
	}
	.port .row {
		flex: 1;
		min-width: 0;
	}
	.ext {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
	}
	.ext:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.start {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font: inherit;
		font-size: 12px;
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: inherit;
	}
	.start:hover {
		background: var(--ca-action-hover);
	}
	.start kbd {
		font: inherit;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.empty {
		padding: 16px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
