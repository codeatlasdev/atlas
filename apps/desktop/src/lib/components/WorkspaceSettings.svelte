<script lang="ts">
	import { Button, SegmentedControl, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import { onMount } from "svelte";
	import { api, errText, type WorktreeStats } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";

	let workspaceDir = $state("");
	let branchPrefix = $state("git-username");
	let branchPrefixCustom = $state("");
	let nest = $state(true);
	let stats = $state<Record<string, WorktreeStats | "loading">>({});
	let picked = $state<Record<string, boolean>>({});
	let linked = $derived(store.projects.filter((p) => p.parent));

	onMount(async () => {
		const s = await api.settings();
		workspaceDir = (s.workspaceDir as string) ?? "";
		branchPrefix = (s.branchPrefix as string) ?? "git-username";
		branchPrefixCustom = (s.branchPrefixCustom as string) ?? "";
		nest = s.nestWorkspaces !== "false";
	});

	function save(patch: Record<string, unknown>) {
		api.settingsPatch(patch).catch((e) => toast.error(errText(e)));
	}

	async function scan() {
		for (const p of linked) {
			stats[p.id] = "loading";
			api.worktreeStats(p.id).then(
				(s) => (stats[p.id] = s),
				() => delete stats[p.id],
			);
		}
	}

	async function deleteSelected() {
		const ids = Object.keys(picked).filter((k) => picked[k]);
		picked = {};
		for (const id of ids) await worktrees.remove(id, true);
	}

	const size = (kb: number | null) =>
		kb === null
			? "?"
			: kb > 1024 * 1024
				? `${(kb / 1024 / 1024).toFixed(1)} GB`
				: `${Math.round(kb / 1024)} MB`;
	const ago = (ts: number) => {
		const d = Math.floor((Date.now() - ts) / 86_400_000);
		return d <= 0 ? "today" : `${d}d ago`;
	};
</script>

<PageHead
	title="Workspaces"
	description="Each worktree is its own checkout of a repository. Choose where Atlas puts them and how their branches are named."
/>

<Group title="New worktrees">
	<Row
		id="ws-dir"
		label="Worktree folder"
		description="Where Atlas creates checkouts. Leave empty to use ~/.atlas/worktrees."
		stack
	>
		<TextInput
			mono
			width="100%"
			label="Worktree folder"
			placeholder="~/.atlas/worktrees"
			value={workspaceDir}
			onchange={(v) => {
				workspaceDir = v;
				save({ workspaceDir: v || null });
			}}
		/>
	</Row>
	<Row
		id="ws-nest"
		label="Group by repository"
		description="Put each worktree in a folder named after its repository, so projects do not mix."
	>
		<Switch
			aria-label="Group by repository"
			checked={nest}
			onchange={(v) => {
				nest = v;
				save({ nestWorkspaces: v ? null : "false" });
			}}
		/>
	</Row>
	<Row
		id="ws-prefix"
		label="Branch name prefix"
		description="Added in front of every branch Atlas creates, such as your-name/fix-login."
	>
		<SegmentedControl
			aria-label="Branch name prefix"
			size="sm"
			value={branchPrefix}
			items={[
				{ value: "git-username", label: "My git name" },
				{ value: "custom", label: "Custom" },
				{ value: "none", label: "None" },
			]}
			onchange={(v) => {
				branchPrefix = v;
				save({ branchPrefix: v });
			}}
		/>
	</Row>
	{#if branchPrefix === "custom"}
		<Row label="Custom prefix" description="Text to put before each new branch name.">
			<TextInput
				label="Custom prefix"
				placeholder="team"
				value={branchPrefixCustom}
				onchange={(v) => {
					branchPrefixCustom = v;
					save({ branchPrefixCustom: v });
				}}
			/>
		</Row>
	{/if}
</Group>

<div class="gap"></div>

<Group
	title="Clean up"
	description="Linked worktrees take disk space. Scan to see how big each one is, its last commit and whether it has unsaved work."
>
	<Row
		id="ws-cleanup"
		label="Linked worktrees"
		description="Tick the ones you no longer need, then delete them."
	>
		<Button size="sm" variant="glass" onclick={scan}>Scan</Button>
		<Button
			size="sm"
			variant="destructive"
			disabled={!Object.values(picked).some(Boolean)}
			onclick={deleteSelected}>Delete selected</Button
		>
	</Row>
	{#each linked as p (p.id)}
		{@const s = stats[p.id]}
		<label class="wt">
			<input type="checkbox" bind:checked={picked[p.id]} />
			<span class="name">{ws.worktree(p.id)?.name ?? p.root}</span>
			<span class="mono">{p.branch ?? ""}</span>
			{#if s === "loading"}<Spinner size={11} />{:else if s}
				<span>{size(s.kb)}</span>
				<span>{s.last_commit ? ago(s.last_commit * 1000) : "-"}</span>
				<span>
					{s.dirty ? "unsaved changes" : "clean"}{#if s.ahead !== null}
						· ↑{s.ahead} ↓{s.behind}{/if}
				</span>
			{/if}
		</label>
	{:else}
		<p class="empty">No linked worktrees yet. Create one from the sidebar and it will show up here.</p>
	{/each}
</Group>

<style>
	.gap {
		height: 4px;
	}
	.wt {
		display: grid;
		grid-template-columns: 20px 1.4fr 1.4fr 70px 70px 1fr;
		gap: 8px;
		align-items: center;
		padding: 8px 14px;
		font-size: 12px;
	}
	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.mono {
		font-family: ui-monospace, monospace;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--ca-text-secondary);
	}
	.empty {
		margin: 0;
		padding: 12px 14px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
