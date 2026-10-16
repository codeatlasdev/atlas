<script lang="ts">
	/** The "…" menu of a project head or worktree row. */
	import { DropdownMenuItem } from "@codeatlasdev/ui";
	import { api } from "#lib/api.ts";
	import { locateProject, removeProject } from "./projects.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { promptDialog } from "#lib/stores/ask.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws, type Worktree } from "#lib/stores/workspace.svelte.ts";
	import { sidebar } from "./state.svelte.ts";
	import type { ProjectView } from "#lib/api.ts";

	let { p, w, onAgent }: { p: ProjectView; w: Worktree | undefined; onAgent?: (worktreeId: string) => void } =
		$props();

	function openTerminal(id: string) {
		store.selected = id;
		ws.select(id);
		void ws.newTerminal(id);
	}

	async function rename(id: string) {
		const w = ws.worktree(id);
		const v = w && (await promptDialog("Rename worktree", { value: w.name, confirmLabel: "Rename" }));
		if (w && v) w.name = v;
	}

	function toggleStack() {
		store.run(`stack:${p.id}`, () => (p.running ? api.stop(p.id) : api.start(p.id)));
	}
</script>

{#if p.missing}
	<DropdownMenuItem onclick={() => locateProject(p.id)}>Locate…</DropdownMenuItem>
{:else}
	<DropdownMenuItem onclick={() => openTerminal(p.id)}>New terminal</DropdownMenuItem>
	<DropdownMenuItem onclick={() => onAgent?.(p.id)}>New agent</DropdownMenuItem>
	<DropdownMenuItem onclick={toggleStack}>
		{p.running ? "Stop stack" : "Start stack"}
	</DropdownMenuItem>
	<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}
		>Open in editor</DropdownMenuItem
	>
	<DropdownMenuItem onclick={() => store.run("reveal", () => api.reveal(p.id))}
		>Reveal in Finder</DropdownMenuItem
	>
{/if}
<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
	{p.pinned ? "Unpin" : "Pin to top"}
</DropdownMenuItem>
{#if !p.parent}
	<DropdownMenuItem onclick={() => (identities.editing = p.id)}>Change icon and color…</DropdownMenuItem>
	<DropdownMenuItem variant="destructive" onclick={() => removeProject(p.id)}
		>Remove from Atlas…</DropdownMenuItem
	>
{/if}
{#if w}
	<DropdownMenuItem onclick={() => (w.unread = !w.unread)}>
		{w.unread ? "Mark as read" : "Mark as unread"}
	</DropdownMenuItem>
{/if}
{#if p.parent && w}
	<DropdownMenuItem onclick={() => rename(p.id)}>Rename</DropdownMenuItem>
	<DropdownMenuItem onclick={() => (w.archived = !w.archived)}>
		{w.archived ? "Unarchive" : "Archive"}
	</DropdownMenuItem>
	<DropdownMenuItem variant="destructive" onclick={() => sidebar.askDelete([p.id])}>Delete…</DropdownMenuItem>
{/if}
