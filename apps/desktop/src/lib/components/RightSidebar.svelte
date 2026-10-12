<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { Select, Skeleton, toast } from "@codeatlasdev/ui";
	import { Play, Copy, FolderSearch, Globe, RefreshCw, PanelRightClose, Pin } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { docks, toggleDock } from "#lib/stores/docks.svelte.ts";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
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

	let w = $derived(ws.selected);

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
		const pids = Object.values(ws.ptys)
			.filter((p) => p.scope.worktree_id === w.id && p.pid)
			.map((p) => p.pid!);
		await ws.syncPtys();
		ports = pids.length ? await invoke<Port[]>("listening_ports", { pids }) : [];
	}

	$effect(() => {
		void [scope, w?.id, tab];
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

	/** Local ports open directly; remote ones through an ssh -L forward first. */
	async function openPort(port: number) {
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
		void openPreview(w.id, `http://localhost:${local}`);
	}

	const when = (ts: number) => new Date(ts * 1000).toLocaleString();
	const resumeCommand = (s: Session) =>
		s.agent === "codex" ? `codex resume ${s.resume}` : `${s.agent} --resume ${s.resume}`;
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
				bind:value={query}
				oninput={onSearch}
				onkeydown={(e) => e.key === "Enter" && (clearTimeout(typing), loadSessions())}
			/>
			<Select
				size="sm"
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
			{#each sessions as s (s.file)}
				<button
					class="row"
					class:open={open === s.file}
					onclick={() => (open = open === s.file ? null : s.file)}
				>
					<span class="agent">{s.agent}</span>
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
									navigator.clipboard.writeText(`omp --resume ${s.file}`).then(
										() => toast.success("Copied"),
										(e) => toast.error(errText(e)),
									)}><Copy size={12} /> Copy command</button
							>
							<button onclick={() => api.openUrl(`file://${s.file.replace(/\/[^/]+$/, "")}`)}
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
				{:else}
					<p class="empty">No sessions{query ? ` match “${query}”` : " yet"}.</p>
				{/if}
			{/each}
		</div>
	{:else}
		<div class="list">
			{#each ports as p (p.port)}
				<button class="row" onclick={() => w && openPreview(w.id, `http://localhost:${p.port}`)}>
					<Globe size={12} /><span class="title">:{p.port}</span><span class="dim">{p.command} ({p.pid})</span
					>
				</button>
			{:else}
				<p class="empty">No listening ports in this worktree's terminals.</p>
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
	.empty {
		padding: 16px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
