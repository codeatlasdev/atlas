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
	import { classifyGitError } from "#lib/gitErrors.ts";
	import { commitErrorText, commitNotice } from "#lib/commitMessage.ts";
	import { gitToast } from "#lib/gitToast.ts";
	import PathText from "./PathText.svelte";
	import NotRepoState from "./NotRepoState.svelte";
	import { m } from "#lib/i18n/m.ts";
	import { operationText } from "#lib/gitText.ts";

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
	/** The folder is not a git repository: nothing to stage or commit, only `git init` (#505). */
	let notRepo = $state(false);
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
	/** A failed command in words, with git's own output next to it (the raw text is what "Fix with AI" sends). */
	let problem = $derived(failure ? classifyGitError(failure) : null);

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
			[m.git_commit_group_conflicts(), conflicts, "conflicts"],
			[m.git_commit_group_staged(), staged, "staged"],
			[m.git_commit_group_changes(), changes, "changes"],
			[m.git_commit_group_untracked(), untracked, "untracked"],
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
			status: (s) => {
				st = s;
				notRepo = false;
			},
			error: (e) => {
				st = null;
				notRepo = classifyGitError(errText(e)).kind === "not-repo";
				if (!notRepo) gitToast(errText(e));
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
		(await confirmDialog(m.git_commit_discard_title({ path: e.path }), {
			description: e.x === "?" ? m.git_commit_discard_untracked() : m.git_commit_discard_tracked(),
			confirmLabel: m.git_source_control_discard(),
			destructive: true,
		})) &&
		(e.x === "?"
			? run("discard", ["clean", "-f", "--", e.path])
			: run("discard", ["checkout", "--", e.path]));

	async function commit(amend = false) {
		if (!message.trim() && !amend) return toast.error(m.git_commit_write_message());
		// Ask before git does: its answer is a wall of text with a shell recipe in it.
		if (await identityMissing(gitHere)) return askWho(amend);
		const args = ["commit", "-F", "-", ...(amend ? ["--amend"] : [])];
		if (await run("commit", args, message)) {
			message = "";
			toast.success(amend ? m.git_commit_amended() : m.git_commit_committed());
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

	type Primary = "stage" | "commit" | "pull" | "push" | "done";
	let primaryKind = $derived<Primary>(
		!staged.length && (changes.length || untracked.length)
			? "stage"
			: staged.length
				? "commit"
				: behind > 0
					? "pull"
					: ahead > 0 || !st?.upstream
						? "push"
						: "done",
	);
	let primaryLabel = $derived.by(() => {
		switch (primaryKind) {
			case "stage":
				return m.git_source_control_stage_all();
			case "commit":
				return m.git_commit_primary_commit();
			case "pull":
				return m.git_commit_primary_pull({ count: behind });
			case "push":
				return ahead ? m.git_commit_primary_push_count({ count: ahead }) : m.git_source_control_push();
			case "done":
				return m.git_commit_primary_up_to_date();
		}
	});

	async function toAgent(text: string) {
		const t = agents.tabsIn(worktreeId)[0];
		if (t) await agents.send(t.id, text);
		else await agents.launch(worktreeId, agents.defaultAgent, { prompt: text });
	}
</script>

<aside class="sc">
	{#if notRepo}
		<NotRepoState {worktreeId} />
	{:else}
		<div class="head">
			<span class="branch mono">{st?.branch ?? "…"}</span>
			{#if st?.base && st.base !== "HEAD"}<span class="dim">→ {st.base}</span>{/if}
			{#if st?.vs_base}<span class="dim">↑{st.vs_base.ahead} ↓{st.vs_base.behind}</span>{/if}
			<DropdownMenu align="end">
				{#snippet trigger()}<span class="more"><Ellipsis size={13} /></span>{/snippet}
				<DropdownMenuItem onclick={() => commit(true)}
					>{m.git_source_control_amend_last_commit()}</DropdownMenuItem
				>
				<DropdownMenuItem onclick={() => run("pull", ["pull", "--rebase"])}
					>{m.git_source_control_pull_rebase()}</DropdownMenuItem
				>
				<DropdownMenuItem onclick={() => run("push", ["push"])}
					>{m.git_source_control_push()}</DropdownMenuItem
				>
				<DropdownMenuItem
					variant="destructive"
					onclick={async () =>
						(await confirmDialog(m.git_commit_force_push_title(), {
							description: m.git_commit_force_push_body(),
							confirmLabel: m.git_commit_force_push_confirm(),
							destructive: true,
						})) && run("push", ["push", "--force-with-lease"])}
				>
					{m.git_source_control_force_push_with_lease()}
				</DropdownMenuItem>
				<DropdownMenuItem onclick={() => openDiff(worktreeId)}
					>{m.git_source_control_open_changes()}</DropdownMenuItem
				>
			</DropdownMenu>
		</div>

		{#if st?.operation}
			<div class="conflict">
				<b>{operationText(st.operation)}</b>
				{#if conflicts.length}<span>{m.git_commit_conflicted_files({ count: conflicts.length })}</span>{/if}
				<div class="row-btns">
					<Button
						size="sm"
						onclick={() =>
							toAgent(
								// i18n-ignore: the prompt an agent reads
								`Resolve the git ${st!.operation} conflicts in: ${conflicts.map((c) => c.path).join(", ")}. Then stage the files and continue the ${st!.operation}.`,
							)}
					>
						{m.git_source_control_resolve_with_ai()}
					</Button>
					<Button size="sm" variant="ghost" onclick={() => run("abort", [st!.operation!, "--abort"])}
						>{m.git_source_control_abort()}</Button
					>
				</div>
			</div>
		{/if}

		<div class="msg" role="presentation" onkeydown={(e) => e.key === "Enter" && primaryKey(e) && commit()}>
			<TextArea
				bind:value={message}
				placeholder={m.git_commit_message_placeholder({ key: kbd("cmd-enter") })}
				rows={3}
			/>
			{#if gen.phase === "running"}
				<span class="dim"
					>{gen.agent ? m.git_commit_writing_with({ agent: gen.agent }) : m.git_commit_writing()}</span
				>
			{:else if gen.error}
				<span class="err">{commitErrorText(gen.error)}</span>
			{:else if commitNotice(gen)}
				<span class="dim">{commitNotice(gen)}</span>
			{/if}
			{#if hint.show}
				{#if showInstruction}
					<input
						class="instr"
						aria-label={m.git_source_control_instruction_for_the_commit_message()}
						placeholder={m.git_source_control_e_g_mention_issue_72()}
						bind:value={instruction}
						onkeydown={(e) =>
							e.key === "Enter" && (e.preventDefault(), commitMessages.generate(worktreeId, instruction))}
					/>
				{:else}
					<button class="link" onclick={() => (showInstruction = true)}
						>{m.git_source_control_add_an_instruction()}</button
					>
				{/if}
			{/if}
			<div class="row-btns">
				<Button size="sm" disabled={!!busy || primaryKind === "done"} onclick={primary}>
					{#snippet leading()}{#if busy && busy !== "ai"}<Spinner size={12} />{/if}{/snippet}
					{primaryLabel}
				</Button>
				{#if hint.show}
					{#if gen.phase === "running"}
						<Button size="sm" variant="ghost" onclick={() => commitMessages.cancel(worktreeId)}>
							{#snippet leading()}<Square size={12} />{/snippet}
							{m.git_source_control_stop()}
						</Button>
					{:else}
						<Button
							size="sm"
							variant="ghost"
							title={staged.length ? m.git_commit_write_from_staged() : m.git_commit_write_from_all()}
							data-tour="commit-ai"
							onclick={() => commitMessages.generate(worktreeId, instruction)}
						>
							{#snippet leading()}{#if message.trim() && gen.agent}<RefreshCw size={12} />{:else}<Sparkles
										size={12}
									/>{/if}{/snippet}
							{message.trim() && gen.agent ? m.git_commit_regenerate() : m.git_commit_generate()}
						</Button>
					{/if}
				{/if}
				<Button size="sm" variant="ghost" onclick={() => pr.openCreate(worktreeId)}>
					{#snippet leading()}<GitPullRequest size={12} />{/snippet}
					{m.git_source_control_pr()}
				</Button>
			</div>
		</div>

		{#if who}
			<form
				class="who"
				aria-label={m.git_source_control_who_are_you()}
				novalidate
				onsubmit={(e) => {
					e.preventDefault();
					void saveWho();
				}}
			>
				<b>{m.git_source_control_who_is_committing()}</b>
				<span class="dim">{m.git_source_control_git_puts_a_name_and()}</span>
				<label>
					{m.git_new_worktree_dialog_name()}
					<input
						class="instr"
						autocomplete="name"
						placeholder={m.git_source_control_ada_lovelace()}
						bind:value={who.name}
					/>
				</label>
				<label>
					{m.git_source_control_e_mail()}
					<!-- i18n-ignore: an example address, not prose -->
					<input
						class="instr"
						type="email"
						autocomplete="email"
						placeholder="you@example.com"
						bind:value={who.email}
					/>
				</label>
				<fieldset>
					<legend class="dim">{m.git_source_control_save_it_for()}</legend>
					<label class="radio">
						<input type="radio" name="who-scope" value="global" bind:group={who.scope} />
						{m.git_source_control_all_repositories_global()}
					</label>
					<label class="radio">
						<input type="radio" name="who-scope" value="local" bind:group={who.scope} />
						{m.git_source_control_this_repository_only()}
					</label>
				</fieldset>
				{#if who.error}<span class="err" role="alert">{who.error}</span>{/if}
				<div class="row-btns">
					<Button size="sm" type="submit" disabled={who.saving}>
						{#snippet leading()}{#if who?.saving}<Spinner size={12} />{/if}{/snippet}
						{who.amend ? m.git_commit_save_amend() : m.git_commit_save_commit()}
					</Button>
					<Button size="sm" variant="ghost" onclick={() => (who = null)}>{m.common_action_cancel()}</Button>
				</div>
			</form>
		{/if}

		{#if failure && problem}
			<div class="failure">
				<span class="ca-selectable">{problem.message}</span>
				{#if problem.details}
					<details>
						<summary>{m.git_source_control_details()}</summary>
						<pre class="ca-selectable">{problem.details}</pre>
					</details>
				{/if}
				<Button
					size="sm"
					variant="ghost"
					onclick={() => toAgent(`This git command failed. Fix the cause:\n\n${failure}`)}
					>{m.git_source_control_fix_with_ai()}</Button
				>
			</div>
		{/if}

		<VirtualList bind:this={vl} bind:scroller class="lists" items={rows} key={(r) => r.key} estimate={24}>
			{#snippet item(r)}
				{#if r.kind === "group"}
					<div class="group">
						<span>{r.title} <span class="dim">{r.list.length}</span></span>
						{#if r.group === "staged"}
							<button
								title={m.git_source_control_unstage_all()}
								onclick={() => unstage(r.list.map((e) => e.path))}><Minus size={12} /></button
							>
						{:else if r.group !== "conflicts"}
							<button
								title={m.git_source_control_stage_all()}
								onclick={() => stage(r.list.map((e) => e.path))}><Plus size={12} /></button
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
							<button
								title={m.git_source_control_unstage()}
								onclick={(ev) => (ev.stopPropagation(), unstage([e.path]))}><Minus size={12} /></button
							>
						{:else if r.group !== "conflicts"}
							<button
								title={m.git_source_control_discard()}
								onclick={(ev) => (ev.stopPropagation(), discard(e))}><Undo2 size={12} /></button
							>
							<button
								title={m.git_source_control_stage()}
								onclick={(ev) => (ev.stopPropagation(), stage([e.path]))}><Plus size={12} /></button
							>
						{/if}
					</div>
				{/if}
			{/snippet}
		</VirtualList>
	{/if}
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
	.failure summary {
		cursor: pointer;
		font-size: 11px;
		color: var(--ca-text-secondary);
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
