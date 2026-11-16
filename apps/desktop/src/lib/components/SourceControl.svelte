<script lang="ts">
	import { usePoll } from "#lib/clock.svelte.ts";
	import { onMount, tick, untrack } from "svelte";
	import { kbd, primaryKey } from "#lib/keys.ts";
	import { Button, DropdownMenu, DropdownMenuItem, Spinner, TextArea, toast } from "@codeatlasdev/ui";
	import { Plus, Minus, Undo2, Sparkles, Ellipsis, GitPullRequest, Square, RefreshCw } from "@lucide/svelte";
	import { git, errText, type GitEntry, type GitStatus } from "#lib/api.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { commitMessages } from "#lib/stores/commitMessage.svelte.ts";
	import { providerHint } from "#lib/commitMessage.ts";
	import { openFile, openDiff } from "#lib/stores/files.svelte.ts";
	import {
		identityMissing,
		identityProblem,
		isIdentityError,
		readIdentity,
		saveIdentity,
		type IdentityScope,
	} from "#lib/gitIdentity.ts";
	import VirtualList from "#lib/components/VirtualList.svelte";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { statusGroups } from "#lib/stores/tree.ts";
	import { gitStatus } from "#lib/stores/gitStatus.ts";
	import PathText from "./PathText.svelte";

	/** Porcelain status letter → `.code` class (anything else reads as modified). */
	const CODE_KIND: Record<string, string> = {
		A: "added",
		U: "added",
		D: "deleted",
		R: "renamed",
		C: "renamed",
		"!": "conflict",
	};

	let { worktreeId }: { worktreeId: string } = $props();

	let st = $state<GitStatus | null>(null);
	let message = $state("");
	let busy = $state<string | null>(null);
	let failure = $state<string | null>(null);
	/** The action `failure` belongs to: a commit's error is about that message and that staged set, nothing else's. */
	let failureOf = $state<string | null>(null);
	/** Commit identity form: shown instead of git's "Author identity unknown"; `amend` is what Save & commit retries. */
	let who = $state<{
		name: string;
		email: string;
		scope: IdentityScope;
		amend: boolean;
		saving: boolean;
		error: string;
	} | null>(null);
	let instruction = $state("");
	let showInstruction = $state(false);
	let gen = $derived(commitMessages.run(worktreeId));
	let hint = $derived(providerHint(commitMessages.provider));

	let groups = $derived(statusGroups(st?.entries ?? []));
	let staged = $derived(groups.staged);
	let changes = $derived(groups.changes);
	let untracked = $derived(groups.untracked);
	let conflicts = $derived(groups.conflicts);

	type Group = "staged" | "changes" | "untracked" | "conflicts";
	/** The panel's one windowed list: each non-empty group's header, then its entries. */
	type Row =
		| { key: string; kind: "group"; title: string; list: GitEntry[]; group: Group }
		| { key: string; kind: "entry"; e: GitEntry; group: Group };
	let rows = $derived.by((): Row[] => {
		const out: Row[] = [];
		const shown: [string, GitEntry[], Group][] = [
			["Conflicts", conflicts, "conflicts"],
			["Staged", staged, "staged"],
			["Changes", changes, "changes"],
			["Untracked", untracked, "untracked"],
		];
		for (const [title, list, group] of shown) {
			if (!list.length) continue;
			out.push({ key: `g:${group}`, kind: "group", title, list, group });
			for (const e of list) out.push({ key: group + e.path, kind: "entry", e, group });
		}
		return out;
	});
	let vl: ReturnType<typeof VirtualList<Row>> | undefined = $state();
	let scroller: HTMLDivElement | undefined = $state();

	/** Arrow keys, Home and End between entries: rows are windowed, so Tab alone would never reach the far ones. */
	async function stepEntry(key: string, to: "next" | "prev" | "first" | "last") {
		const forward = to === "next" || to === "first";
		let i =
			to === "first"
				? 0
				: to === "last"
					? rows.length - 1
					: rows.findIndex((r) => r.key === key) + (forward ? 1 : -1);
		while (rows[i] && rows[i].kind !== "entry") i += forward ? 1 : -1;
		if (!rows[i]) return;
		vl?.reveal(i);
		await tick();
		scroller?.querySelector<HTMLElement>(`[data-key="${CSS.escape(rows[i].key)}"]`)?.focus();
	}

	const STEP: Record<string, "next" | "prev" | "first" | "last"> = {
		ArrowDown: "next",
		ArrowUp: "prev",
		Home: "first",
		End: "last",
	};
	let ahead = $derived(st?.ab ? Number(st.ab.split(" ")[0].slice(1)) : 0);
	let behind = $derived(st?.ab ? Number(st.ab.split(" ")[1].slice(1)) : 0);

	const refresh = () => gitStatus.refresh(worktreeId);

	usePoll(5000, () => true, refresh);

	onMount(() => {
		void commitMessages.loadProvider();
		const detach = commitMessages.attach(worktreeId, (text) => (message = text));
		// Fed on mount and on every fs event inside this worktree, debounced and shared with the Explorer (#307).
		const off = gitStatus.watch(worktreeId, ws.worktree(worktreeId)?.path ?? "", {
			status: (s) => (st = s),
			error: (e) => {
				toast.error(errText(e));
				st = null;
			},
		});
		return () => {
			detach();
			off();
		};
	});

	const gitHere = (args: string[]) => git.run(worktreeId, args);

	async function run(label: string, args: string[], stdin?: string): Promise<boolean> {
		busy = label;
		failure = null;
		failureOf = null;
		try {
			const r = await git.run(worktreeId, args, stdin);
			if (r.code !== 0) {
				failure = (r.stderr || r.stdout).trim();
				failureOf = label;
				return false;
			}
			return true;
		} finally {
			busy = null;
			await refresh();
		}
	}

	// A commit error describes one message and one staged set. Once either changes (an edit, a commit or an unstage
	// from the terminal or an agent) it is history, and leaving it next to a different state reads as a live failure.
	let stagedKey = $derived(staged.map((e) => `${e.x}${e.path}`).join("\0"));
	$effect(() => {
		message;
		stagedKey;
		untrack(() => {
			if (failureOf === "commit") {
				failure = null;
				failureOf = null;
			}
		});
	});

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
		// Ask before git does: its answer is a wall of text with a shell recipe in it.
		if (await identityMissing(gitHere)) return askWho(amend);
		const args = ["commit", "-F", "-", ...(amend ? ["--amend"] : [])];
		if (await run("commit", args, message)) {
			message = "";
			toast.success(amend ? "Amended" : "Committed");
		} else if (failure && isIdentityError(failure)) {
			failure = null;
			failureOf = null;
			await askWho(amend);
		}
	}

	/** Open the identity form with what git and `gh` already know. */
	async function askWho(amend: boolean) {
		who = { name: "", email: "", scope: "global", amend, saving: false, error: "" };
		const known = await readIdentity(gitHere);
		const gh = !known.name || !known.email ? await git.ghIdentity(worktreeId).catch(() => null) : null;
		// The reader may already be typing: only fill what is still empty.
		if (who) {
			who.name ||= known.name ?? gh?.name ?? "";
			who.email ||= known.email ?? gh?.email ?? "";
		}
	}

	async function saveWho() {
		if (!who || who.saving) return;
		const problem = identityProblem(who);
		if (problem) {
			who.error = problem;
			return;
		}
		who.saving = true;
		who.error = "";
		try {
			await saveIdentity(gitHere, who, who.scope);
		} catch (e) {
			who.saving = false;
			who.error = errText(e);
			return;
		}
		const amend = who.amend;
		who = null;
		await commit(amend);
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

	<div class="msg" role="presentation" onkeydown={(e) => e.key === "Enter" && primaryKey(e) && commit()}>
		<TextArea bind:value={message} placeholder={`Commit message (${kbd("cmd-enter")} to commit)`} rows={3} />
		{#if gen.phase === "running"}
			<span class="dim">Writing with {gen.agent || "the agent"}…</span>
		{:else if gen.error}
			<span class="err">{gen.error}</span>
		{:else if gen.notice}
			<span class="dim">{gen.notice}</span>
		{/if}
		{#if hint.show}
			{#if showInstruction}
				<input
					class="instr"
					aria-label="Instruction for the commit message"
					placeholder="e.g. mention issue #72"
					bind:value={instruction}
					onkeydown={(e) =>
						e.key === "Enter" && (e.preventDefault(), commitMessages.generate(worktreeId, instruction))}
				/>
			{:else}
				<button class="link" onclick={() => (showInstruction = true)}>Add an instruction…</button>
			{/if}
		{/if}
		<div class="row-btns">
			<Button size="sm" disabled={!!busy || primaryLabel === "Up to date"} onclick={primary}>
				{#snippet leading()}{#if busy && busy !== "ai"}<Spinner size={12} />{/if}{/snippet}
				{primaryLabel}
			</Button>
			{#if hint.show}
				{#if gen.phase === "running"}
					<Button size="sm" variant="ghost" onclick={() => commitMessages.cancel(worktreeId)}>
						{#snippet leading()}<Square size={12} />{/snippet}
						Stop
					</Button>
				{:else}
					<Button
						size="sm"
						variant="ghost"
						title="Write the message from {staged.length ? 'the staged diff' : 'all changes'}"
						data-tour="commit-ai"
						onclick={() => commitMessages.generate(worktreeId, instruction)}
					>
						{#snippet leading()}{#if message.trim() && gen.agent}<RefreshCw size={12} />{:else}<Sparkles
									size={12}
								/>{/if}{/snippet}
						{message.trim() && gen.agent ? "Regenerate" : "Generate"}
					</Button>
				{/if}
			{/if}
			<Button size="sm" variant="ghost" onclick={() => pr.openCreate(worktreeId)}>
				{#snippet leading()}<GitPullRequest size={12} />{/snippet}
				PR
			</Button>
		</div>
	</div>

	{#if who}
		<form
			class="who"
			aria-label="Who are you?"
			novalidate
			onsubmit={(e) => {
				e.preventDefault();
				void saveWho();
			}}
		>
			<b>Who is committing?</b>
			<span class="dim">Git puts a name and an e-mail on every commit. Atlas saves them and commits.</span>
			<label>
				Name
				<input class="instr" autocomplete="name" placeholder="Ada Lovelace" bind:value={who.name} />
			</label>
			<label>
				E-mail
				<input
					class="instr"
					type="email"
					autocomplete="email"
					placeholder="you@example.com"
					bind:value={who.email}
				/>
			</label>
			<fieldset>
				<legend class="dim">Save it for</legend>
				<label class="radio">
					<input type="radio" name="who-scope" value="global" bind:group={who.scope} />
					All repositories (global)
				</label>
				<label class="radio">
					<input type="radio" name="who-scope" value="local" bind:group={who.scope} />
					This repository only
				</label>
			</fieldset>
			{#if who.error}<span class="err" role="alert">{who.error}</span>{/if}
			<div class="row-btns">
				<Button size="sm" type="submit" disabled={who.saving}>
					{#snippet leading()}{#if who?.saving}<Spinner size={12} />{/if}{/snippet}
					{who.amend ? "Save & amend" : "Save & commit"}
				</Button>
				<Button size="sm" variant="ghost" onclick={() => (who = null)}>Cancel</Button>
			</div>
		</form>
	{/if}

	{#if failure}
		<div class="failure">
			<pre class="ca-selectable">{failure}</pre>
			<Button
				size="sm"
				variant="ghost"
				onclick={() => toAgent(`This git command failed. Fix the cause:\n\n${failure}`)}>Fix with AI</Button
			>
		</div>
	{/if}

	<VirtualList bind:this={vl} bind:scroller class="lists" items={rows} key={(r) => r.key} estimate={24}>
		{#snippet item(r)}
			{#if r.kind === "group"}
				<div class="group">
					<span>{r.title} <span class="dim">{r.list.length}</span></span>
					{#if r.group === "staged"}
						<button title="Unstage all" onclick={() => unstage(r.list.map((e) => e.path))}
							><Minus size={12} /></button
						>
					{:else if r.group !== "conflicts"}
						<button title="Stage all" onclick={() => stage(r.list.map((e) => e.path))}
							><Plus size={12} /></button
						>
					{/if}
				</div>
			{:else}
				{@const e = r.e}
				{@const code =
					r.group === "staged" ? e.x : r.group === "untracked" ? "U" : r.group === "conflicts" ? "!" : e.y}
				<div
					class="entry"
					role="button"
					tabindex="0"
					data-key={r.key}
					onclick={() => openFile(worktreeId, e.path)}
					onkeydown={(ev) => {
						if (ev.key === "Enter") return openFile(worktreeId, e.path);
						const to = ev.target === ev.currentTarget ? STEP[ev.key] : undefined;
						if (!to) return;
						ev.preventDefault();
						void stepEntry(r.key, to);
					}}
				>
					<span class="code {CODE_KIND[code] ?? 'modified'}">{code}</span>
					<PathText path={e.path} />
					{#if r.group === "staged"}
						<button title="Unstage" onclick={(ev) => (ev.stopPropagation(), unstage([e.path]))}
							><Minus size={12} /></button
						>
					{:else if r.group !== "conflicts"}
						<button title="Discard" onclick={(ev) => (ev.stopPropagation(), discard(e))}
							><Undo2 size={12} /></button
						>
						<button title="Stage" onclick={(ev) => (ev.stopPropagation(), stage([e.path]))}
							><Plus size={12} /></button
						>
					{/if}
				</div>
			{/if}
		{/snippet}
	</VirtualList>
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
	.who,
	.conflict,
	.failure {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 0 10px 10px;
	}
	.who {
		gap: 8px;
	}
	.who label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.who fieldset {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
		padding: 0;
		border: 0;
	}
	.who .radio {
		flex-direction: row;
		align-items: center;
		gap: 6px;
		color: var(--ca-text-primary);
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
		color: var(--ca-semantic-error);
	}
	.row-btns {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.sc :global(.lists) {
		flex: 1;
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
	}
	.entry:hover {
		background: var(--ca-action-hover);
	}
	.code {
		width: 12px;
		font-weight: 700;
		font-size: 10.5px;
		color: var(--atlas-vcs-modified, var(--ca-semantic-warning));
	}
	.code.added {
		color: var(--atlas-vcs-added, var(--ca-semantic-success));
	}
	.code.deleted {
		color: var(--atlas-vcs-deleted, var(--ca-semantic-error));
	}
	.code.renamed {
		color: var(--atlas-vcs-renamed, var(--ca-semantic-info));
	}
	.code.conflict {
		color: var(--ca-semantic-error);
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
	.err {
		font-size: 11px;
		color: var(--ca-semantic-error);
	}
	.link {
		align-self: flex-start;
		padding: 0;
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-decoration: underline;
	}
	.instr {
		font: inherit;
		font-size: 12px;
		padding: 5px 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-primary);
	}
	.dim {
		color: var(--ca-text-secondary);
		font-weight: 400;
		font-size: 11.5px;
	}
</style>
