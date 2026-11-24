<script lang="ts">
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	/** The "+" menu of a tab strip: new terminal/agent/panel/changes/PR tabs, split, rename, close. */
	import { DropdownMenu, DropdownMenuItem, toast } from "@codeatlasdev/ui";
	import {
		FileCode2,
		FileDiff,
		GitPullRequest,
		LayoutGrid,
		Plus,
		ScrollText,
		SquareTerminal,
	} from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { openDiff } from "#lib/stores/files.svelte.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { hasTermView, reloadTermView } from "#lib/stores/termViews.ts";
	import { pinned } from "#lib/stores/tabs.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { groupId, onrename }: { groupId: string; onrename: (tabId: string) => void } = $props();

	const g = $derived(ws.group(groupId));
	const active = $derived(ws.tab(g?.activeTab));
	const w = $derived(ws.worktree(g?.worktreeId));
	const project = $derived(store.projects.find((p) => p.id === w?.repoId));
	const newTabKey = $derived(shortcutFor("workspace::NewTerminal"));

	function openKind(kind: "services" | "logs" | "config") {
		if (!w) return;
		// Stored as these English names and drawn in the app's language by `tabLabel`.
		const labels = { services: "Panel", logs: "Logs", config: "Config" };
		ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
	}
</script>

<DropdownMenu align="start">
	{#snippet triggerChild(props)}
		<button
			type="button"
			{...props}
			class="plus"
			aria-label={m.workbench_new_tab_menu_new_tab()}
			title={newTabKey
				? m.workbench_new_tab_menu_new_tab_key({ key: newTabKey })
				: m.workbench_new_tab_menu_new_tab()}
		>
			<Plus size={14} />
		</button>
	{/snippet}
	<DropdownMenuItem onclick={() => w && ws.newTerminal(w.id, { groupId })}>
		{#snippet icon()}<SquareTerminal />{/snippet}
		{m.crew_chat_header_terminal()}
		<span class="kbd">{shortcutFor("workspace::NewTerminal") ?? ""}</span>
	</DropdownMenuItem>
	{#each agents.list.filter((a) => a.installed) as a (a.id)}
		<DropdownMenuItem brandIcon onclick={() => w && agents.launch(w.id, a.id, { groupId })}>
			{#snippet icon()}<AgentLogo agent={a.id} bare />{/snippet}
			{m.workbench_new_tab_menu_new_session({ label: a.label })}
		</DropdownMenuItem>
	{/each}
	{#if agents.list.find((a) => a.id === "omp" && a.installed)}
		<DropdownMenuItem brandIcon onclick={() => w && agents.launch(w.id, "omp", { groupId, chat: true })}>
			{#snippet icon()}<AgentLogo agent="omp" bare />{/snippet}
			{m.workbench_new_tab_menu_new_omp_chat()}
		</DropdownMenuItem>
	{/if}
	{#if project && project.kind !== "folder"}
		<DropdownMenuItem onclick={() => openKind("services")}>
			{#snippet icon()}<LayoutGrid />{/snippet}
			{m.workbench_tab_panel()}
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => openKind("logs")}>
			{#snippet icon()}<ScrollText />{/snippet}
			{m.panel_tab_logs()}
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => openKind("config")}>
			{#snippet icon()}<FileCode2 />{/snippet}
			{m.panel_tab_config()}
		</DropdownMenuItem>
	{/if}
	{#if w && project?.kind !== "folder"}
		<DropdownMenuItem onclick={() => openDiff(w.id)}>
			{#snippet icon()}<FileDiff />{/snippet}
			{m.workbench_tab_changes()}
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => pr.openTab(w.id)}>
			{#snippet icon()}<GitPullRequest />{/snippet}
			{m.workbench_tab_pull_request()}
		</DropdownMenuItem>
	{/if}
	<DropdownMenuItem onclick={() => g?.activeTab && ws.split(groupId, "h", g.activeTab)}>
		{m.workbench_new_tab_menu_split_right()}
		<span class="kbd">{shortcutFor("pane::SplitRight") ?? ""}</span>
	</DropdownMenuItem>
	<DropdownMenuItem onclick={() => g?.activeTab && ws.split(groupId, "v", g.activeTab)}>
		{m.workbench_new_tab_menu_split_down()}
		<span class="kbd">{shortcutFor("pane::SplitDown") ?? ""}</span>
	</DropdownMenuItem>
	{#if active}
		{#if !pinned(active)}<DropdownMenuItem onclick={() => onrename(active.id)}>
				{m.workbench_new_tab_menu_rename_tab()}
			</DropdownMenuItem>{/if}
		{#if hasTermView(active.id)}
			<DropdownMenuItem onclick={() => void reloadTermView(active.id).catch((e) => toast.error(errText(e)))}>
				{m.workbench_new_tab_menu_reload_terminal()}
			</DropdownMenuItem>
		{/if}
		<DropdownMenuItem onclick={() => ws.closeMany(groupId, active.id)}>
			{m.workbench_new_tab_menu_close_others()}
			<span class="kbd">{shortcutFor("pane::CloseOtherItems") ?? ""}</span>
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => ws.closeMany(groupId)}>
			{m.workbench_new_tab_menu_close_all()}
			<span class="kbd">{shortcutFor("pane::CloseAllItems") ?? ""}</span>
		</DropdownMenuItem>
	{/if}
</DropdownMenu>

<style>
	.plus {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: 4px;
		background: none;
		/* the kit's trigger sets a dark-theme glass white here: inheriting it left the + at 1.03:1 on light themes */
		color: var(--ca-text-secondary);
		transition:
			opacity 120ms ease-out,
			background 120ms ease-out;
	}
	.plus:hover {
		opacity: 1;
		background: var(--ca-action-hover);
	}
	.kbd {
		margin-left: auto;
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
</style>
