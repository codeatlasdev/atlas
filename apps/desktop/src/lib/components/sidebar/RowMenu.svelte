<script lang="ts">
	/** The "…" menu of a project head or worktree row. */
	import { DropdownMenuItem } from "@codeatlasdev/ui";
	import { api } from "#lib/api.ts";
	import { revealLabel } from "#lib/keys.ts";
	import { locateProject, removeProject } from "./projects.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { promptDialog } from "#lib/stores/ask.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws, type Worktree } from "#lib/stores/workspace.svelte.ts";
	import { sidebar } from "./state.svelte.ts";
	import type { ProjectView } from "#lib/api.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import type { CrewmateProject } from "#lib/crew.ts";
	import { m } from "#lib/i18n/m.ts";

	let { p, w, onAgent }: { p: ProjectView; w: Worktree | undefined; onAgent?: (worktreeId: string) => void } =
		$props();

	function openTerminal(id: string) {
		store.selected = id;
		ws.select(id);
		void ws.newTerminal(id);
	}

	async function rename(id: string) {
		const w = ws.worktree(id);
		const v =
			w &&
			(await promptDialog(m.sidebar_row_menu_rename_title(), {
				value: w.name,
				confirmLabel: m.common_action_rename(),
			}));
		if (w && v) w.name = v;
	}

	function toggleStack() {
		store.run(`stack:${p.id}`, () => (p.running ? api.stop(p.id) : api.start(p.id)));
	}

	function createCrewmateForProject() {
		ws.appMode = "crew";
		const project: CrewmateProject = {
			name: p.name,
			kind: "path",
			branch: w?.branch || undefined,
		};
		crew.pendingProjectLink = project;
	}
</script>

{#if p.missing}
	<DropdownMenuItem onclick={() => locateProject(p.id)}>
		{m.shell_home_missing_project_locate()}
	</DropdownMenuItem>
{:else}
	<DropdownMenuItem onclick={() => openTerminal(p.id)}>{m.sidebar_row_menu_new_terminal()}</DropdownMenuItem>
	<DropdownMenuItem onclick={() => onAgent?.(p.id)}>{m.tray_page_new_agent()}</DropdownMenuItem>
	<DropdownMenuItem onclick={toggleStack}>
		{p.running ? m.shell_commands_stop_stack() : m.shell_commands_start_stack()}
	</DropdownMenuItem>
	<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}>
		{m.sidebar_row_menu_open_in_editor()}
	</DropdownMenuItem>
	<DropdownMenuItem onclick={() => store.run("reveal", () => api.reveal(p.id))}>
		{revealLabel()}
	</DropdownMenuItem>
	<DropdownMenuItem onclick={createCrewmateForProject}>
		{m.sidebar_row_menu_create_crewmate()}
	</DropdownMenuItem>
{/if}
<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
	{p.pinned ? m.shell_project_header_unpin() : m.shell_project_header_pin_to_top()}
</DropdownMenuItem>
{#if !p.parent}
	<DropdownMenuItem onclick={() => (identities.editing = p.id)}>
		{m.shell_commands_project_menu_change_icon()}
	</DropdownMenuItem>
	<DropdownMenuItem variant="destructive" onclick={() => removeProject(p.id)}>
		{m.shell_project_header_remove_from_atlas()}
	</DropdownMenuItem>
{/if}
{#if w}
	<DropdownMenuItem onclick={() => (w.unread = !w.unread)}>
		{w.unread ? m.sidebar_row_menu_mark_read() : m.sidebar_row_menu_mark_unread()}
	</DropdownMenuItem>
{/if}
{#if p.parent && w}
	<DropdownMenuItem onclick={() => rename(p.id)}>{m.common_action_rename()}</DropdownMenuItem>
	<DropdownMenuItem onclick={() => (w.archived = !w.archived)}>
		{w.archived ? m.sidebar_row_menu_unarchive() : m.sidebar_row_menu_archive()}
	</DropdownMenuItem>
	<DropdownMenuItem variant="destructive" onclick={() => sidebar.askDelete([p.id])}>
		{m.sidebar_row_menu_delete()}
	</DropdownMenuItem>
{/if}
