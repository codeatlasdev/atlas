<script lang="ts">
	import { Button, SegmentedControl, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import { onMount } from "svelte";
	import { api, errText, type WorktreeStats } from "#lib/api.ts";
	import { relativeTime } from "#lib/i18n/fmt.ts";
	import {
		cleanupPrefs,
		restorable,
		type Cleaned,
		type CleanupMode,
		type CleanupPrefs,
	} from "#lib/stores/autoCleanup.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";
	import WorktreePreview from "#lib/settings/WorktreePreview.svelte";
	import HygieneGroup from "./settings/HygieneGroup.svelte";
	import { m } from "#lib/i18n/m.ts";

	let workspaceDir = $state("");
	let branchPrefix = $state("git-username");
	let branchPrefixCustom = $state("");
	let nest = $state(true);
	let stats = $state<Record<string, WorktreeStats | "loading">>({});
	let picked = $state<Record<string, boolean>>({});
	let linked = $derived(store.projects.filter((p) => p.parent));
	let auto = $state<CleanupPrefs>({ mode: "off", days: 14 });
	let cleaned = $state<Cleaned[]>([]);

	let loadError = $state<string | null>(null);

	onMount(async () => {
		try {
			const s = await api.settings();
			workspaceDir = (s.workspaceDir as string) ?? "";
			branchPrefix = (s.branchPrefix as string) ?? "git-username";
			branchPrefixCustom = (s.branchPrefixCustom as string) ?? "";
			nest = s.nestWorkspaces !== "false";
			auto = cleanupPrefs(s);
			cleaned = restorable(s.cleanedWorktrees, Date.now());
		} catch (e) {
			loadError = errText(e);
		}
	});

	async function restore(c: Cleaned) {
		// Drop the entry only once the worktree exists again: a failed create keeps the branch reachable from here.
		if (!(await worktrees.create({ repoId: c.repoId, name: c.name, branch: c.branch }))) return;
		cleaned = cleaned.filter((x) => x !== c);
		save({ cleanedWorktrees: cleaned.length ? cleaned : null });
	}

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
	const ago = (ts: number) => relativeTime(ts);
</script>

{#if loadError}
	<LoadError what="workspace" error={loadError} />
{/if}

<Group title={m.settings_general_workspace_new_worktrees()}>
	<Row
		id="ws-dir"
		label={m.settings_general_workspace_worktree_folder()}
		description={m.settings_general_workspace_where_atlas_creates_checkouts_leave()}
		stack
	>
		<!-- i18n-ignore: an example path, not prose -->
		<TextInput
			mono
			width="100%"
			label={m.settings_general_workspace_worktree_folder()}
			placeholder="~/.atlas/worktrees"
			value={workspaceDir}
			onchange={(v) => {
				workspaceDir = v;
				save({ workspaceDir: v || null });
			}}
		/>
		{#snippet below()}
			<WorktreePreview
				prefs={{ dir: workspaceDir, nest, prefix: branchPrefix, custom: branchPrefixCustom }}
			/>
		{/snippet}
	</Row>
	<Row
		id="ws-nest"
		label={m.settings_general_workspace_group_by_repository()}
		description={m.settings_general_workspace_put_each_worktree_in_a()}
	>
		<Switch
			aria-label={m.settings_general_workspace_group_by_repository()}
			checked={nest}
			onchange={(v) => {
				nest = v;
				save({ nestWorkspaces: v ? null : "false" });
			}}
		/>
	</Row>
	<Row
		id="ws-prefix"
		label={m.settings_general_workspace_branch_name_prefix()}
		description={m.settings_general_workspace_added_in_front_of_every()}
	>
		<SegmentedControl
			aria-label={m.settings_general_workspace_branch_name_prefix()}
			size="sm"
			value={branchPrefix}
			items={[
				{ value: "git-username", label: m.settings_general_workspace_prefix_git_name() },
				{ value: "custom", label: m.settings_general_workspace_prefix_custom() },
				{ value: "none", label: m.settings_general_workspace_prefix_none() },
			]}
			onchange={(v) => {
				branchPrefix = v;
				save({ branchPrefix: v });
			}}
		/>
	</Row>
	{#if branchPrefix === "custom"}
		<Row
			label={m.settings_general_workspace_custom_prefix()}
			description={m.settings_general_workspace_text_to_put_before_each()}
		>
			<TextInput
				label={m.settings_general_workspace_custom_prefix()}
				placeholder={m.settings_general_workspace_team()}
				value={branchPrefixCustom}
				onchange={(v) => {
					branchPrefixCustom = v;
					save({ branchPrefixCustom: v });
				}}
			/>
		</Row>
	{/if}
</Group>

<Group
	title={m.settings_general_workspace_auto_clean_up()}
	description={m.settings_general_workspace_atlas_never_removes_a_worktree()}
>
	<Row
		id="ws-autoclean"
		label={m.settings_general_workspace_remove_worktrees_automatically()}
		description={m.settings_general_workspace_checked_every_15_minutes_pinned()}
	>
		<SegmentedControl
			aria-label={m.settings_general_workspace_remove_worktrees_automatically()}
			size="sm"
			value={auto.mode}
			items={[
				{ value: "off", label: m.settings_general_git_option_off() },
				{ value: "merged", label: m.settings_general_workspace_cleanup_merged() },
				{ value: "idle", label: m.settings_general_workspace_cleanup_idle() },
			]}
			onchange={(v) => {
				auto.mode = v as CleanupMode;
				save({ autoCleanup: v === "off" ? null : v });
			}}
		/>
	</Row>
	{#if auto.mode === "idle"}
		<Row
			label={m.settings_general_workspace_idle_for_days()}
			description={m.settings_general_workspace_days_without_agent_activity_before()}
		>
			<TextInput
				label={m.settings_general_workspace_idle_for_days()}
				value={String(auto.days)}
				onchange={(v) => {
					auto = cleanupPrefs({ autoCleanup: auto.mode, autoCleanupDays: v });
					save({ autoCleanupDays: auto.days });
				}}
			/>
		</Row>
	{/if}
	{#each cleaned as c (c.at + c.name)}
		<Row
			label={c.name}
			description={m.settings_general_workspace_cleaned({ when: ago(c.at), branch: c.branch })}
		>
			<Button size="sm" variant="glass" onclick={() => restore(c)}
				>{m.settings_general_workspace_restore()}</Button
			>
		</Row>
	{/each}
</Group>

<Group
	title={m.settings_general_workspace_clean_up()}
	description={m.settings_general_workspace_linked_worktrees_take_disk_space()}
>
	<Row
		id="ws-cleanup"
		label={m.settings_general_workspace_linked_worktrees()}
		description={m.settings_general_workspace_tick_the_ones_you_no()}
	>
		<Button size="sm" variant="glass" onclick={scan}>{m.settings_general_workspace_scan()}</Button>
		<Button
			size="sm"
			variant="destructive"
			disabled={!Object.values(picked).some(Boolean)}
			onclick={deleteSelected}>{m.settings_general_workspace_delete_selected()}</Button
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
					{s.dirty
						? m.settings_general_workspace_state_dirty()
						: m.settings_general_workspace_state_clean()}{#if s.ahead !== null}
						· ↑{s.ahead} ↓{s.behind}{/if}
				</span>
			{/if}
		</label>
	{:else}
		<p class="empty">{m.settings_general_workspace_no_linked_worktrees_yet_create()}</p>
	{/each}
</Group>

<HygieneGroup />

<style>
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
