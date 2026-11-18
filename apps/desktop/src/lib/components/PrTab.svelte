<script lang="ts">
	import { usePoll } from "#lib/clock.svelte.ts";
	import { onMount } from "svelte";
	import {
		Button,
		DropdownMenu,
		DropdownMenuItem,
		EmptyState,
		Spinner,
		StatusChip,
		TextArea,
	} from "@codeatlasdev/ui";
	import {
		CircleAlert,
		CloudUpload,
		ExternalLink,
		GitBranch,
		GitPullRequest,
		LogIn,
		RefreshCw,
		Unplug,
		Wrench,
	} from "@lucide/svelte";
	import { api, errText } from "#lib/api.ts";
	import { pr, rollup, type Check } from "#lib/stores/pr.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { gitStatus } from "#lib/stores/gitStatus.ts";
	import { openBranchPicker } from "#lib/stores/branch.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import {
		describeState,
		ghAuth,
		repoState,
		type GhAuth,
		type GhLookup,
		type RepoAction,
		type RepoProbe,
	} from "#lib/repoState.ts";
	import NotRepoState from "./NotRepoState.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { worktreeId, focused = false }: { worktreeId: string; focused?: boolean } = $props();

	const POLL_MS = 60_000;
	let probe = $state<RepoProbe | null>(null);
	let auth = $state<GhAuth>("unknown");
	let lookup = $state<GhLookup | null>(null);
	/** Branch and upstream come from the status feed Source Control and the Explorer already share. */
	let branch = $state<string | null>(null);
	let upstream = $state<string | null>(null);
	let failed = $state("");
	let checks = $state<Check[]>([]);
	let loading = $state(true);
	let reply = $state("");
	let log = $state<{ name: string; text: string } | null>(null);

	/** Why there is, or is not, a PR: git's facts, then gh's (only asked when it can answer). */
	let repo = $derived(repoState({ probe, auth, branch, upstream, lookup }));
	let v = $derived(repo?.kind === "pr" ? repo.pr : null);

	async function refresh() {
		loading = true;
		failed = "";
		try {
			probe = await pr.probe(worktreeId);
			// Git already says why there can be no PR (no repository, no commit, no remote, a GitLab remote): gh adds nothing.
			if (repoState({ probe, auth: "ok", branch, upstream, lookup: null })) return void (lookup = null);
			if (auth !== "ok") auth = await ghAuth(pr.runner(worktreeId, 15_000));
			if (auth !== "ok") return void (lookup = null);
			lookup = await pr.lookup(worktreeId);
			if (lookup.kind === "signed-out") auth = "signed-out";
			checks = lookup.kind === "pr" ? await pr.checks(worktreeId, lookup.pr.number) : [];
		} catch (e) {
			failed = errText(e);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		let seen: string | null | undefined;
		const off = gitStatus.watch(worktreeId, ws.worktree(worktreeId)?.path ?? "", {
			status: (s) => {
				branch = s.branch;
				upstream = s.upstream;
				// Another branch has another PR (or none): do not keep showing the last one's answer.
				if (seen !== undefined && seen !== s.branch) void refresh();
				seen = s.branch;
			},
			error: () => {},
		});
		void refresh();
		return off;
	});

	const ICON: Record<string, typeof GitBranch> = {
		"no-commits": GitBranch,
		"no-remote": Unplug,
		"remote-not-github": Unplug,
		"gh-missing": LogIn,
		"gh-signed-out": LogIn,
		"gh-error": CircleAlert,
		"default-branch": GitBranch,
		"not-pushed": CloudUpload,
		ready: GitPullRequest,
	};

	async function act(id: RepoAction) {
		const remotes = probe?.remotes ?? [];
		if (id === "add-remote") {
			if (await pr.addRemote(worktreeId, remotes.some((r) => r.name === "origin") ? "github" : "origin"))
				await refresh();
		} else if (id === "install-gh") void api.openUrl("https://cli.github.com");
		else if (id === "sign-in") pr.signIn(worktreeId);
		else if (id === "create-branch") void openBranchPicker();
		else if (id === "push" && repo?.kind === "not-pushed") {
			if (await pr.pushBranch(worktreeId, repo.remote, repo.branch)) {
				// The upstream is new: do not wait for the watcher to say so.
				await gitStatus.refresh(worktreeId);
				await refresh();
			}
		} else if (id === "create-pr") pr.openCreate(worktreeId);
		else if (id === "retry") await refresh();
	}

	const REVIEW: Record<string, [string, "success" | "error" | "warning"]> = {
		APPROVED: ["approved", "success"],
		CHANGES_REQUESTED: ["changes requested", "error"],
		REVIEW_REQUIRED: ["review required", "warning"],
	};
	const CHECKS = { ok: "success", failed: "error", pending: "warning", none: "neutral" } as const;

	// Polls while this tab is the focused one, once on gaining focus; the shared clock sleeps with a hidden window.
	usePoll(
		POLL_MS,
		() => focused,
		() => void refresh(),
	);

	async function showLog(c: Check) {
		log = { name: c.name, text: "Loading…" };
		log = { name: c.name, text: await pr.failedLog(worktreeId, c) };
	}

	async function fix() {
		const failed = checks.filter((c) => c.state === "FAILURE");
		let text = `CI checks failed on PR #${v?.number}. Fix them.\n`;
		for (const c of failed.slice(0, 3))
			text += `\n## ${c.name}\n${(await pr.failedLog(worktreeId, c)).slice(-8000)}\n`;
		const t = agents.tabsIn(worktreeId)[0];
		if (t) await agents.send(t.id, text);
		else await agents.launch(worktreeId, agents.defaultAgent, { prompt: text });
	}

	const chip = (s: string) =>
		s === "SUCCESS"
			? "success"
			: s === "FAILURE"
				? "error"
				: s === "PENDING" || s === "IN_PROGRESS"
					? "warning"
					: "neutral";
</script>

<div class="pr">
	{#if !v}
		<div class="pad">
			{#if failed}
				<EmptyState icon={CircleAlert} title={m.git_pr_tab_could_not_read_the_repository()} description={failed} bare>
					{#snippet actions()}<Button size="sm" onclick={refresh}>{m.git_pr_tab_try_again()}</Button>{/snippet}
				</EmptyState>
			{:else if !repo || repo.kind === "pr"}
				{#if loading}<Spinner />{/if}
			{:else if repo.kind === "not-repo"}
				<NotRepoState {worktreeId} onInit={refresh} />
			{:else}
				{@const t = describeState(repo)}
				<EmptyState icon={ICON[repo.kind]} title={t.title} description={t.description} bare>
					{#snippet actions()}
						{#if t.action}
							{@const a = t.action}
							<Button size="sm" onclick={() => act(a.id)}>{a.label}</Button>
						{/if}
					{/snippet}
				</EmptyState>
			{/if}
		</div>
	{:else}
		<header>
			<h2>#{v.number} {v.title}</h2>
			<StatusChip
				status={v.state === "MERGED" ? "info" : v.state === "OPEN" ? "success" : "neutral"}
				label={v.isDraft ? "draft" : v.state.toLowerCase()}
			/>
			{#if REVIEW[v.reviewDecision]}
				<StatusChip status={REVIEW[v.reviewDecision][1]} label={REVIEW[v.reviewDecision][0]} />
			{/if}
			{#if rollup(v) !== "none"}
				<StatusChip status={CHECKS[rollup(v)]} label="checks {rollup(v)}" />
			{/if}
			<span class="dim">{v.headRefName} → {v.baseRefName} · {v.author.login}</span>
			<span class="spacer"></span>
			<button class="icon" title={m.git_diff_view_refresh()} onclick={refresh}><RefreshCw size={13} /></button>
			<button class="icon" title={m.git_pr_tab_open_on_github()} onclick={() => api.openUrl(v!.url)}
				><ExternalLink size={13} /></button
			>
			{#if v.state === "OPEN"}
				{#if v.isDraft}
					<Button
						size="sm"
						variant="ghost"
						onclick={() => pr.run(worktreeId, ["pr", "ready", String(v!.number)], "Marked ready")}
						>{m.git_pr_tab_mark_ready()}</Button
					>
				{/if}
				<DropdownMenu align="end">
					{#snippet trigger()}<span class="merge">{m.git_pr_tab_merge()}</span>{/snippet}
					{#each [["--squash", "Squash and merge"], ["--merge", "Create a merge commit"], ["--rebase", "Rebase and merge"]] as [flag, label] (flag)}
						<DropdownMenuItem
							onclick={() => pr.run(worktreeId, ["pr", "merge", String(v!.number), flag], "Merged")}
							>{label}</DropdownMenuItem
						>
					{/each}
					<DropdownMenuItem
						onclick={() =>
							pr.run(
								worktreeId,
								["pr", "merge", String(v!.number), "--squash", "--auto"],
								"Auto-merge enabled",
							)}
					>
						{m.git_pr_tab_enable_auto_merge_squash()}
					</DropdownMenuItem>
				</DropdownMenu>
			{/if}
		</header>
		<div class="cols">
			<section class="timeline">
				{#if v.body}<article>
						<b>{v.author.login}</b>
						<p class="ca-selectable">{v.body}</p>
					</article>{/if}
				{#each [...v.reviews.map( (r) => ({ who: r.author.login, at: r.submittedAt, text: r.body, tag: r.state }) ), ...v.comments.map( (c) => ({ who: c.author.login, at: c.createdAt, text: c.body, tag: "" }) )].sort( (a, b) => a.at.localeCompare(b.at) ) as e, i (i)}
					<article>
						<b>{e.who}</b>{#if e.tag}<span class="dim"> · {e.tag.toLowerCase()}</span>{/if}
						<span class="dim"> · {new Date(e.at).toLocaleString()}</span>
						{#if e.text}<p class="ca-selectable">{e.text}</p>{/if}
					</article>
				{/each}
				<div class="reply">
					<TextArea bind:value={reply} placeholder={m.git_pr_tab_comment()} rows={3} />
					<Button
						size="sm"
						disabled={!reply.trim()}
						onclick={async () => {
							await pr.run(worktreeId, ["pr", "comment", String(v!.number), "--body", reply], "Commented");
							reply = "";
						}}>{m.git_pr_tab_comment()}</Button
					>
				</div>
			</section>
			<section class="checks">
				<h3>Checks <span class="dim">{rollup(v)}</span></h3>
				{#each checks as c (c.name + c.link)}
					<button class="check" onclick={() => c.state === "FAILURE" && showLog(c)}>
						<StatusChip status={chip(c.state)} label={c.state.toLowerCase()} />
						<span class="name">{c.workflow ? `${c.workflow} / ` : ""}{c.name}</span>
					</button>
				{:else}
					<p class="dim">{m.git_pr_tab_no_checks()}</p>
				{/each}
				{#if checks.some((c) => c.state === "FAILURE")}
					<Button size="sm" onclick={fix}>
						{#snippet leading()}<Wrench size={12} />{/snippet}
						{m.git_pr_tab_fix_broken_checks()}
					</Button>
				{/if}
				{#if log}
					<div class="log">
						<b>{log.name}</b>
						<pre class="ca-selectable">{log.text}</pre>
					</div>
				{/if}
			</section>
		</div>
	{/if}
</div>

<style>
	.pr {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		font-size: 13px;
	}
	.pad {
		padding: 20px;
	}
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	h2 {
		margin: 0;
		font-size: 16px;
	}
	.spacer {
		flex: 1;
	}
	.icon {
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
	}
	.merge {
		padding: 4px 10px;
		border-radius: 6px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		font-weight: 600;
		font-size: 12px;
	}
	.cols {
		flex: 1;
		display: grid;
		grid-template-columns: minmax(0, 1fr) 320px;
		min-height: 0;
	}
	.timeline,
	.checks {
		overflow: auto;
		padding: 12px 16px;
	}
	.checks {
		border-left: 0.5px solid var(--ca-outline-subtle);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	h3 {
		margin: 0 0 6px;
		font-size: 12px;
	}
	article {
		padding: 8px 0;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	article p {
		margin: 6px 0 0;
		white-space: pre-wrap;
	}
	.reply {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding-top: 10px;
		align-items: flex-start;
	}
	.check {
		display: flex;
		align-items: center;
		gap: 8px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: inherit;
		text-align: left;
		padding: 2px 0;
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.log pre {
		max-height: 340px;
		overflow: auto;
		font-size: 11px;
		white-space: pre-wrap;
	}
	.dim {
		color: var(--ca-text-secondary);
		font-size: 12px;
	}
</style>
