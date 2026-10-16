<script lang="ts">
	/** The "+" menu of a tab strip: new terminal/agent/panel/changes/PR tabs, split, rename, close. */
	import { DropdownMenu, DropdownMenuItem } from "@codeatlasdev/ui";
	import {
		FileCode2,
		FileDiff,
		GitPullRequest,
		LayoutGrid,
		Plus,
		ScrollText,
		SquareTerminal,
	} from "@lucide/svelte";
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import Overlay from "#lib/components/Overlay.svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { openDiff } from "#lib/stores/files.svelte.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { pinned } from "#lib/stores/tabs.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	let { groupId, onrename }: { groupId: string; onrename: (tabId: string) => void } = $props();

	const g = $derived(ws.group(groupId));
	const active = $derived(ws.tab(g?.activeTab));
	const w = $derived(ws.worktree(g?.worktreeId));
	const project = $derived(store.projects.find((p) => p.id === w?.repoId));

	function openKind(kind: "services" | "logs" | "config") {
		if (!w) return;
		const labels = { services: "Panel", logs: "Logs", config: "Config" };
		ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
	}
</script>

<Overlay width={24} height={24}>
	<DropdownMenu align="start">
		{#snippet trigger()}
			<span class="plus" aria-label="New tab" title="New tab {shortcutFor('workspace::NewTerminal') ?? ''}"
				><Plus size={14} /></span
			>
		{/snippet}
		<DropdownMenuItem onclick={() => w && ws.newTerminal(w.id, { groupId })}>
			<SquareTerminal size={14} /> Terminal
			<span class="kbd">{shortcutFor("workspace::NewTerminal") ?? ""}</span>
		</DropdownMenuItem>
		{#each agents.list.filter((a) => a.installed) as a (a.id)}
			<DropdownMenuItem onclick={() => w && agents.launch(w.id, a.id, { groupId })}>
				<AgentLogo agent={a.id} /> New {a.label} session
			</DropdownMenuItem>
		{/each}
		{#if agents.list.find((a) => a.id === "omp" && a.installed)}
			<DropdownMenuItem onclick={() => w && agents.launch(w.id, "omp", { groupId, chat: true })}>
				<AgentLogo agent="omp" /> New omp chat
			</DropdownMenuItem>
		{/if}
		{#if project}
			<DropdownMenuItem onclick={() => openKind("services")}><LayoutGrid size={14} /> Panel</DropdownMenuItem>
			<DropdownMenuItem onclick={() => openKind("logs")}><ScrollText size={14} /> Logs</DropdownMenuItem>
			<DropdownMenuItem onclick={() => openKind("config")}><FileCode2 size={14} /> Config</DropdownMenuItem>
		{/if}
		{#if w}
			<DropdownMenuItem onclick={() => openDiff(w.id)}><FileDiff size={14} /> Changes</DropdownMenuItem>
			<DropdownMenuItem onclick={() => pr.openTab(w.id)}
				><GitPullRequest size={14} /> Pull request</DropdownMenuItem
			>
		{/if}
		<DropdownMenuItem onclick={() => g?.activeTab && ws.split(groupId, "h", g.activeTab)}
			>Split right <span class="kbd">{shortcutFor("pane::SplitRight") ?? ""}</span></DropdownMenuItem
		>
		<DropdownMenuItem onclick={() => g?.activeTab && ws.split(groupId, "v", g.activeTab)}
			>Split down <span class="kbd">{shortcutFor("pane::SplitDown") ?? ""}</span></DropdownMenuItem
		>
		{#if active}
			{#if !pinned(active)}<DropdownMenuItem onclick={() => onrename(active.id)}>Rename tab</DropdownMenuItem
				>{/if}
			<DropdownMenuItem onclick={() => ws.closeMany(groupId, active.id)}>
				Close others <span class="kbd">{shortcutFor("pane::CloseOtherItems") ?? ""}</span>
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => ws.closeMany(groupId)}>
				Close all <span class="kbd">{shortcutFor("pane::CloseAllItems") ?? ""}</span>
			</DropdownMenuItem>
		{/if}
	</DropdownMenu>
</Overlay>

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
