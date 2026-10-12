<script lang="ts">
	import { onMount } from "svelte";
	import { Button, DropdownMenu, DropdownMenuItem, Spinner, StatusChip, TextArea } from "@codeatlasdev/ui";
	import { ExternalLink, RefreshCw, Wrench } from "@lucide/svelte";
	import { api } from "#lib/api.ts";
	import { pr, rollup, type Check, type PrView } from "#lib/stores/pr.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";

	let { worktreeId, focused = false }: { worktreeId: string; focused?: boolean } = $props();

	const POLL_MS = 60_000;
	let v = $state<PrView | null>(null);
	let checks = $state<Check[]>([]);
	let loading = $state(true);
	let reply = $state("");
	let log = $state<{ name: string; text: string } | null>(null);

	async function refresh() {
		loading = true;
		v = await pr.view(worktreeId);
		checks = v ? await pr.checks(worktreeId, v.number) : [];
		loading = false;
	}

	onMount(() => {
		void pr.checkAuth(worktreeId);
		void refresh();
		const t = window.setInterval(refresh, POLL_MS);
		return () => clearInterval(t);
	});

	$effect(() => {
		if (focused) void refresh();
	});

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
	{#if pr.auth === "missing"}
		<p class="pad">GitHub CLI is not signed in. Run <code>gh auth login</code> in a terminal.</p>
	{:else if !v}
		<div class="pad">
			{#if loading}<Spinner />{:else}
				<p>No pull request for this branch.</p>
				<Button onclick={() => pr.openCreate(worktreeId)}>Create pull request</Button>
			{/if}
		</div>
	{:else}
		<header>
			<h2>#{v.number} {v.title}</h2>
			<StatusChip
				status={v.state === "MERGED" ? "info" : v.state === "OPEN" ? "success" : "neutral"}
				label={v.isDraft ? "draft" : v.state.toLowerCase()}
			/>
			<span class="dim">{v.headRefName} → {v.baseRefName} · {v.author.login}</span>
			<span class="spacer"></span>
			<button class="icon" title="Refresh" onclick={refresh}><RefreshCw size={13} /></button>
			<button class="icon" title="Open on GitHub" onclick={() => api.openUrl(v!.url)}
				><ExternalLink size={13} /></button
			>
			{#if v.state === "OPEN"}
				{#if v.isDraft}
					<Button
						size="sm"
						variant="ghost"
						onclick={() => pr.run(worktreeId, ["pr", "ready", String(v!.number)], "Marked ready")}
						>Mark ready</Button
					>
				{/if}
				<DropdownMenu align="end">
					{#snippet trigger()}<span class="merge">Merge…</span>{/snippet}
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
						Enable auto-merge (squash)
					</DropdownMenuItem>
				</DropdownMenu>
			{/if}
		</header>
		<div class="cols">
			<section class="timeline">
				{#if v.body}<article>
						<b>{v.author.login}</b>
						<p class="selectable">{v.body}</p>
					</article>{/if}
				{#each [...v.reviews.map( (r) => ({ who: r.author.login, at: r.submittedAt, text: r.body, tag: r.state }) ), ...v.comments.map( (c) => ({ who: c.author.login, at: c.createdAt, text: c.body, tag: "" }) )].sort( (a, b) => a.at.localeCompare(b.at) ) as e, i (i)}
					<article>
						<b>{e.who}</b>{#if e.tag}<span class="dim"> · {e.tag.toLowerCase()}</span>{/if}
						<span class="dim"> · {new Date(e.at).toLocaleString()}</span>
						{#if e.text}<p class="selectable">{e.text}</p>{/if}
					</article>
				{/each}
				<div class="reply">
					<TextArea bind:value={reply} placeholder="Comment" rows={3} />
					<Button
						size="sm"
						disabled={!reply.trim()}
						onclick={async () => {
							await pr.run(worktreeId, ["pr", "comment", String(v!.number), "--body", reply], "Commented");
							reply = "";
						}}>Comment</Button
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
					<p class="dim">No checks.</p>
				{/each}
				{#if checks.some((c) => c.state === "FAILURE")}
					<Button size="sm" onclick={fix}>
						{#snippet leading()}<Wrench size={12} />{/snippet}
						Fix broken checks
					</Button>
				{/if}
				{#if log}
					<div class="log">
						<b>{log.name}</b>
						<pre class="selectable">{log.text}</pre>
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
