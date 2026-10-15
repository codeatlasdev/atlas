<script lang="ts">
	import { DropdownMenu, DropdownMenuItem, toast } from "@codeatlasdev/ui";
	import { Plus, Minus, Send, RefreshCw } from "@lucide/svelte";
	import { git, errText, type FileDiff, type DiffHunk } from "#lib/api.ts";
	import { hunkPatch, reviewPrompt } from "#lib/review.ts";
	import { uid, ws, type DiffComment } from "#lib/stores/workspace.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import DiffTable, { type Draft } from "./diff/DiffTable.svelte";
	import PathText from "./PathText.svelte";
	import { highlightFile } from "./diff/highlight.ts";

	let {
		worktreeId,
		mode = "base",
		only,
		focused = false,
	}: {
		worktreeId: string;
		mode?: "working" | "staged" | "base";
		only?: string;
		focused?: boolean;
	} = $props();

	const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

	let files = $state<FileDiff[]>([]);
	let base = $state<string | null>(null);
	let baseRef = $state("");
	let curMode = $state<"working" | "staged" | "base">("base");
	let loading = $state(true);
	let selected = $state<string | null>(null);
	let wrap = $state(false);
	let tokens = $state<Record<string, string[]>>({});
	let draft = $state<Draft | null>(null);
	let commits = $state<string[]>([]);
	let fileEls: Record<string, HTMLElement> = {};

	$effect(() => {
		curMode = mode;
	});

	let comments = $derived(ws.doc.diffComments[worktreeId] ?? []);
	let shown = $derived(only ? files.filter((f) => f.path === only) : files);
	let open = $derived(comments.filter((c) => !c.resolved));

	async function load() {
		loading = true;
		try {
			const r = await git.diff(worktreeId, curMode, baseRef || undefined);
			files = r.files;
			base = r.base;
			tokens = {};
			for (const f of r.files) void highlightFile(f, tokens);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void [worktreeId, curMode, baseRef];
		void load();
	});

	$effect(() => {
		if (curMode === "base" && !only)
			git
				.run(worktreeId, ["log", "--oneline", "-50"])
				.then((r) => (commits = r.stdout.split("\n").filter(Boolean)));
	});

	async function stage(f: FileDiff, h?: DiffHunk) {
		const unstage = curMode === "staged";
		let r;
		if (!h || f.status === "untracked")
			r = await git.run(worktreeId, unstage ? ["restore", "--staged", "--", f.path] : ["add", "--", f.path]);
		else
			r = await git.run(worktreeId, ["apply", "--cached", ...(unstage ? ["-R"] : []), "-"], hunkPatch(f, h));
		if (r.code !== 0) toast.error(r.stderr || "git failed");
		await load();
	}

	function addComment() {
		if (!draft?.text.trim()) return;
		const c: DiffComment = {
			id: uid(),
			path: draft.path,
			line: draft.line,
			side: draft.side,
			text: draft.text.trim(),
			resolved: false,
			createdAt: Date.now(),
		};
		ws.doc.diffComments[worktreeId] = [...comments, c];
		draft = null;
	}

	function setResolved(id: string, v: boolean) {
		ws.doc.diffComments[worktreeId] = comments.map((c) => (c.id === id ? { ...c, resolved: v } : c));
	}

	async function sendTo(tabId: string | null) {
		const notes = open;
		if (!notes.length) return;
		const text = reviewPrompt(notes);
		if (tabId) await agents.send(tabId, text);
		else await agents.launch(worktreeId, agents.defaultAgent, { prompt: text });
		ws.doc.diffComments[worktreeId] = comments.map((c) =>
			notes.some((n) => n.id === c.id) ? { ...c, sent: true } : c,
		);
		toast.success(`Sent ${notes.length} note${notes.length > 1 ? "s" : ""}`);
	}

	function jump(delta: 1 | -1) {
		if (!shown.length) return;
		const i = Math.max(
			0,
			shown.findIndex((f) => f.path === selected),
		);
		const next = shown[(i + delta + shown.length) % shown.length];
		selected = next.path;
		fileEls[next.path]?.scrollIntoView({ block: "start" });
	}

	function onkeydown(e: KeyboardEvent) {
		if (!focused || only) return;
		if (e.key === "F7") {
			jump(e.shiftKey ? -1 : 1);
			e.preventDefault();
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="diff" class:wrap>
	{#if !only}
		<div class="toolbar">
			<select bind:value={curMode}>
				<option value="base">vs base{base ? ` (${base})` : ""}</option>
				<option value="working">Unstaged</option>
				<option value="staged">Staged</option>
			</select>
			{#if curMode === "base"}
				<select bind:value={baseRef}>
					<option value="">{"branch base"}</option>
					<option value="HEAD">HEAD</option>
					{#each commits as c (c)}<option value={c.split(" ")[0]}>{c}</option>{/each}
				</select>
			{/if}
			<label><input type="checkbox" bind:checked={wrap} /> Wrap</label>
			<button title="Refresh" onclick={load}><RefreshCw size={12} /></button>
			<span class="spacer"></span>
			{#if open.length}
				<DropdownMenu align="end">
					{#snippet trigger()}
						<span class="send"
							><Send size={12} /> Send {open.length} note{open.length > 1 ? "s" : ""} to agent</span
						>
					{/snippet}
					{#each agents.tabsIn(worktreeId) as t (t.id)}
						<DropdownMenuItem onclick={() => sendTo(t.id)}
							>{(t.pty && ws.titles[t.pty]) || t.label}</DropdownMenuItem
						>
					{/each}
					<DropdownMenuItem onclick={() => sendTo(null)}>New {agents.defaultAgent} session</DropdownMenuItem>
				</DropdownMenu>
			{/if}
		</div>
	{/if}
	<div class="split">
		{#if !only}
			<nav class="tree">
				{#each files as f (f.path)}
					<button
						class="file {f.status}"
						class:on={selected === f.path}
						onclick={() => {
							selected = f.path;
							fileEls[f.path]?.scrollIntoView({ block: "start" });
						}}
					>
						<span class="st">{f.status[0].toUpperCase()}</span>
						<PathText path={f.path} />
						{#if comments.some((c) => c.path === f.path && !c.resolved)}<span class="dot"></span>{/if}
					</button>
				{:else}
					<p class="empty">{loading ? "Loading…" : "No changes"}</p>
				{/each}
			</nav>
		{/if}
		<div class="files">
			{#each shown as f (f.path)}
				<section bind:this={fileEls[f.path]}>
					<header>
						<span class="mono">{f.old_path ? `${f.old_path} → ` : ""}{f.path}</span>
						<span class="dim">{f.status}</span>
						<button class="act" onclick={() => stage(f)}
							>{curMode === "staged" ? "Unstage file" : "Stage file"}</button
						>
					</header>
					{#if f.binary}
						{#if IMAGE.test(f.path)}
							<p class="dim pad">Image changed (open the file to view).</p>
						{:else}
							<p class="dim pad">Binary file</p>
						{/if}
					{/if}
					{#each f.hunks as h, hi (h.header + hi)}
						<div class="hunk-head">
							<span class="mono">{h.header}</span>
							{#if f.status !== "untracked"}
								<button
									class="act"
									onclick={() => stage(f, h)}
									title={curMode === "staged" ? "Unstage hunk" : "Stage hunk"}
								>
									{#if curMode === "staged"}<Minus size={11} />{:else}<Plus size={11} />{/if}
								</button>
							{/if}
						</div>
						<DiffTable
							path={f.path}
							hunk={h}
							tokens={tokens[`${f.path}#${hi}`]}
							{comments}
							bind:draft
							onsave={addComment}
							onresolve={setResolved}
						/>
					{/each}
				</section>
			{:else}
				{#if only}<p class="empty">{loading ? "Loading…" : "No changes in this file"}</p>{/if}
			{/each}
		</div>
	</div>
</div>

<style>
	.diff {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		font-size: 12px;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.toolbar select,
	.toolbar label {
		font: inherit;
		font-size: 12px;
	}
	.toolbar button {
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
	}
	.spacer {
		flex: 1;
	}
	.send {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 10px;
		border-radius: 6px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		font-weight: 600;
	}
	.split {
		flex: 1;
		display: flex;
		min-height: 0;
	}
	.tree {
		width: 240px;
		flex-shrink: 0;
		overflow-y: auto;
		border-right: 0.5px solid var(--ca-outline-subtle);
		padding: 4px;
		resize: horizontal;
	}
	.file {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 4px 6px;
		border: 0;
		border-radius: 5px;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
	}
	.file.on,
	.file:hover {
		background: var(--ca-action-hover);
	}
	.st {
		width: 12px;
		font-weight: 700;
		font-size: 10.5px;
	}
	.added .st,
	.untracked .st {
		color: var(--atlas-vcs-added, var(--ca-semantic-success));
	}
	.modified .st {
		color: var(--atlas-vcs-modified, var(--ca-semantic-warning));
	}
	.renamed .st {
		color: var(--atlas-vcs-renamed, var(--ca-semantic-info));
	}
	.deleted .st {
		color: var(--atlas-vcs-deleted, var(--ca-semantic-error));
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 3px;
		background: var(--ca-brand-main);
	}
	.files {
		flex: 1;
		overflow: auto;
		min-width: 0;
	}
	section header {
		position: sticky;
		top: 0;
		z-index: 1;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		background: var(--ca-surface-base);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-weight: 600;
	}
	.act {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 5px;
		background: none;
		font: inherit;
		font-size: 11px;
		padding: 1px 6px;
		color: var(--ca-text-secondary);
	}
	.hunk-head {
		display: flex;
		align-items: center;
		padding: 3px 10px;
		background: color-mix(in oklab, var(--ca-semantic-info) 8%, transparent);
		color: var(--ca-text-secondary);
	}
	.mono {
		font-family: var(--mono);
	}
	.dim {
		color: var(--ca-text-secondary);
		font-weight: 400;
	}
	.pad {
		padding: 10px;
	}
	.empty {
		padding: 16px;
		color: var(--ca-text-secondary);
	}
</style>
