<script lang="ts">
	import { onMount } from "svelte";
	import { Button, DropdownMenu, DropdownMenuItem, Spinner, TextArea, toast } from "@codeatlasdev/ui";
	import { Plus, Minus, Undo2, Sparkles, Ellipsis, GitPullRequest } from "@lucide/svelte";
	import { listen } from "@tauri-apps/api/event";
	import { git, errText, type GitEntry, type GitStatus } from "#lib/api.ts";
	import { host } from "#lib/host.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { openFile, openDiff } from "#lib/stores/files.svelte.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";

	let { worktreeId }: { worktreeId: string } = $props();

	let st = $state<GitStatus | null>(null);
	let message = $state("");
	let busy = $state<string | null>(null);
	let failure = $state<string | null>(null);

	let staged = $derived(st?.entries.filter((e) => e.x !== "." && e.x !== "?" && !e.conflict) ?? []);
	let changes = $derived(st?.entries.filter((e) => e.y !== "." && e.y !== "?" && !e.conflict) ?? []);
	let untracked = $derived(st?.entries.filter((e) => e.x === "?") ?? []);
	let conflicts = $derived(st?.entries.filter((e) => e.conflict) ?? []);
	let ahead = $derived(st?.ab ? Number(st.ab.split(" ")[0].slice(1)) : 0);
	let behind = $derived(st?.ab ? Number(st.ab.split(" ")[1].slice(1)) : 0);
	let w = $derived(ws.worktree(worktreeId));

	async function refresh() {
		st = await git.status(worktreeId).catch((e) => {
			toast.error(errText(e));
			return null;
		});
	}

	onMount(() => {
		void refresh();
		const off = listen<{ ev: string }>("host://event", (e) => {
			if (e.payload.ev === "fs_event") void refresh();
		});
		const t = window.setInterval(refresh, 5000);
		return () => {
			clearInterval(t);
			void off.then((u) => u());
		};
	});

	async function run(label: string, args: string[], stdin?: string): Promise<boolean> {
		busy = label;
		failure = null;
		try {
			const r = await git.run(worktreeId, args, stdin);
			if (r.code !== 0) {
				failure = (r.stderr || r.stdout).trim();
				return false;
			}
			return true;
		} finally {
			busy = null;
			await refresh();
		}
	}

	const stage = (paths: string[]) => run("stage", ["add", "--", ...paths]);
	const unstage = (paths: string[]) => run("unstage", ["restore", "--staged", "--", ...paths]);
	const discard = async (e: GitEntry) =>
		(await confirmDialog(`Discard changes to ${e.path}?`, {
			description: e.x === "?" ? "The untracked file is deleted." : "This cannot be undone.",
			confirmLabel: "Discard",
			destructive: true,
		})) &&
		(e.x === "?"
			? run("discard", ["clean", "-f", "--", e.path])
			: run("discard", ["checkout", "--", e.path]));

	async function commit(amend = false) {
		if (!message.trim() && !amend) return toast.error("Write a commit message");
		const args = ["commit", "-F", "-", ...(amend ? ["--amend"] : [])];
		if (await run("commit", args, message)) {
			message = "";
			toast.success(amend ? "Amended" : "Committed");
		}
	}

	async function primary() {
		if (!staged.length && (changes.length || untracked.length)) await run("stage", ["add", "-A"]);
		else if (staged.length) await commit();
		else if (behind > 0) await run("pull", ["pull", "--rebase"]);
		else if (ahead > 0 || !st?.upstream) await run("push", ["push"]);
	}

	let primaryLabel = $derived(
		!staged.length && (changes.length || untracked.length)
			? "Stage all"
			: staged.length
				? "Commit"
				: behind > 0
					? `Pull (${behind})`
					: ahead > 0 || !st?.upstream
						? `Push${ahead ? ` (${ahead})` : ""}`
						: "Up to date",
	);

	async function generate() {
		busy = "ai";
		try {
			const diff = await git.run(worktreeId, ["diff", "--cached", "--no-color"]);
			if (!diff.stdout.trim()) return toast.error("Stage something first");
			const r = await host.exec(
				w!.path,
				[
					"omp",
					"-p",
					"--no-session",
					`Write a conventional commit message for this diff. Reply with the message only.\n\n${diff.stdout.slice(0, 60_000)}`,
				],
				{ timeout_ms: 60_000 },
			);
			if (r.code === 0) message = r.stdout.trim();
			else toast.error(r.stderr || "omp failed");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = null;
		}
	}

	async function toAgent(text: string) {
		const t = agents.tabsIn(worktreeId)[0];
		if (t) await agents.send(t.id, text);
		else await agents.launch(worktreeId, agents.defaultAgent, { prompt: text });
	}
</script>

<aside class="sc">
	<div class="head">
		<span class="branch mono">{st?.branch ?? "…"}</span>
		{#if st?.base && st.base !== "HEAD"}<span class="dim">→ {st.base}</span>{/if}
		{#if st?.vs_base}<span class="dim">↑{st.vs_base.ahead} ↓{st.vs_base.behind}</span>{/if}
		<DropdownMenu align="end">
			{#snippet trigger()}<span class="more"><Ellipsis size={13} /></span>{/snippet}
			<DropdownMenuItem onclick={() => commit(true)}>Amend last commit</DropdownMenuItem>
			<DropdownMenuItem onclick={() => run("pull", ["pull", "--rebase"])}>Pull (rebase)</DropdownMenuItem>
			<DropdownMenuItem onclick={() => run("push", ["push"])}>Push</DropdownMenuItem>
			<DropdownMenuItem
				variant="destructive"
				onclick={async () =>
					(await confirmDialog("Force push with lease?", {
						description: "Overwrites the remote branch if nobody else pushed to it.",
						confirmLabel: "Force push",
						destructive: true,
					})) && run("push", ["push", "--force-with-lease"])}
			>
				Force push with lease
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => openDiff(worktreeId)}>Open changes</DropdownMenuItem>
		</DropdownMenu>
	</div>

	{#if st?.operation}
		<div class="conflict">
			<b>{st.operation} in progress</b>
			{#if conflicts.length}<span>{conflicts.length} conflicted file{conflicts.length > 1 ? "s" : ""}</span
				>{/if}
			<div class="row-btns">
				<Button
					size="sm"
					onclick={() =>
						toAgent(
							`Resolve the git ${st!.operation} conflicts in: ${conflicts.map((c) => c.path).join(", ")}. Then stage the files and continue the ${st!.operation}.`,
						)}
				>
					Resolve with AI
				</Button>
				<Button size="sm" variant="ghost" onclick={() => run("abort", [st!.operation!, "--abort"])}
					>Abort</Button
				>
			</div>
		</div>
	{/if}

	<div class="msg" role="presentation" onkeydown={(e) => e.key === "Enter" && e.metaKey && commit()}>
		<TextArea bind:value={message} placeholder="Commit message (⌘Enter to commit)" rows={3} />
		<div class="row-btns">
			<Button size="sm" disabled={!!busy || primaryLabel === "Up to date"} onclick={primary}>
				{#snippet leading()}{#if busy && busy !== "ai"}<Spinner size={12} />{/if}{/snippet}
				{primaryLabel}
			</Button>
			<Button size="sm" variant="ghost" disabled={busy === "ai"} onclick={generate}>
				{#snippet leading()}{#if busy === "ai"}<Spinner size={12} />{:else}<Sparkles
							size={12}
						/>{/if}{/snippet}
				Generate
			</Button>
			<Button size="sm" variant="ghost" onclick={() => pr.openCreate(worktreeId)}>
				{#snippet leading()}<GitPullRequest size={12} />{/snippet}
				PR
			</Button>
		</div>
	</div>

	{#if failure}
		<div class="failure">
			<pre class="selectable">{failure}</pre>
			<Button
				size="sm"
				variant="ghost"
				onclick={() => toAgent(`This git command failed. Fix the cause:\n\n${failure}`)}>Fix with AI</Button
			>
		</div>
	{/if}

	{#snippet group(title: string, list: GitEntry[], kind: "staged" | "changes" | "untracked" | "conflicts")}
		{#if list.length}
			<div class="group">
				<span>{title} <span class="dim">{list.length}</span></span>
				{#if kind === "staged"}
					<button title="Unstage all" onclick={() => unstage(list.map((e) => e.path))}
						><Minus size={12} /></button
					>
				{:else if kind !== "conflicts"}
					<button title="Stage all" onclick={() => stage(list.map((e) => e.path))}><Plus size={12} /></button>
				{/if}
			</div>
			{#each list as e (kind + e.path)}
				<div
					class="entry"
					role="button"
					tabindex="0"
					onclick={() => openFile(worktreeId, e.path)}
					onkeydown={(ev) => ev.key === "Enter" && openFile(worktreeId, e.path)}
				>
					<span class="code"
						>{kind === "staged" ? e.x : kind === "untracked" ? "U" : kind === "conflicts" ? "!" : e.y}</span
					>
					<span class="name">{e.path}</span>
					{#if kind === "staged"}
						<button title="Unstage" onclick={(ev) => (ev.stopPropagation(), unstage([e.path]))}
							><Minus size={12} /></button
						>
					{:else if kind !== "conflicts"}
						<button title="Discard" onclick={(ev) => (ev.stopPropagation(), discard(e))}
							><Undo2 size={12} /></button
						>
						<button title="Stage" onclick={(ev) => (ev.stopPropagation(), stage([e.path]))}
							><Plus size={12} /></button
						>
					{/if}
				</div>
			{/each}
		{/if}
	{/snippet}

	<div class="lists">
		{@render group("Conflicts", conflicts, "conflicts")}
		{@render group("Staged", staged, "staged")}
		{@render group("Changes", changes, "changes")}
		{@render group("Untracked", untracked, "untracked")}
	</div>
</aside>

<style>
	.sc {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		min-height: 0;
		font-size: 12.5px;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
	}
	.branch {
		font-weight: 650;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.more {
		margin-left: auto;
		display: inline-flex;
	}
	.msg,
	.conflict,
	.failure {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 0 10px 10px;
	}
	.conflict {
		padding-top: 8px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, transparent);
	}
	.failure pre {
		margin: 0;
		max-height: 160px;
		overflow: auto;
		font-size: 11px;
		white-space: pre-wrap;
		color: var(--ca-semantic-error, #d33);
	}
	.row-btns {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.lists {
		flex: 1;
		overflow: auto;
	}
	.group {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 10px 2px;
		font-size: 11px;
		font-weight: 650;
		color: var(--ca-text-secondary);
	}
	.entry {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 2px 10px;
		cursor: default;
	}
	.entry:hover {
		background: var(--ca-action-hover);
	}
	.code {
		width: 12px;
		font-weight: 700;
		font-size: 10.5px;
		color: var(--ca-semantic-warning);
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		direction: rtl;
		text-align: left;
	}
	button {
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
		display: inline-flex;
		padding: 2px;
	}
	.entry button {
		opacity: 0;
	}
	.entry:hover button {
		opacity: 1;
	}
	.mono {
		font-family: var(--mono);
	}
	.dim {
		color: var(--ca-text-secondary);
		font-weight: 400;
		font-size: 11.5px;
	}
</style>
