<!-- A crewmate has one project: a row here, not a page of its own. Link one of the projects open in Atlas, or unlink. -->
<script lang="ts">
	import { Button, toast } from "@codeatlasdev/ui";
	import { Folder, GitBranch } from "@lucide/svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { errText } from "#lib/api.ts";
	import type { Crewmate } from "#lib/crew.ts";
	import { projectForServer } from "#lib/crew/projectRef.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	let { crewmate }: { crewmate: Crewmate } = $props();

	const ideProjects = $derived(
		ws.doc.worktrees.map((w) => ({ id: w.id, name: w.name, path: w.path, branch: w.branch ?? "" })),
	);

	async function linkProject(p: { id: string; name: string; path: string; branch: string }) {
		if (!crew.selected) return;
		const origin = await invoke<{ url: string | null }>("project_git_info", { path: p.path })
			.then((i) => i.url)
			.catch(() => null);
		const r = projectForServer(p, origin, crew.selected.hostKey);
		if (!r.ok) {
			toast.error(r.error);
			return;
		}
		await crew
			.updateCrewmate(crew.selected.hostKey, crewmate.id, { project: r.project })
			.catch((e) => toast.error(errText(e)));
	}

	async function unlinkProject() {
		if (!crew.selected) return;
		await crew
			.updateCrewmate(crew.selected.hostKey, crewmate.id, { project: null })
			.catch((e) => toast.error(errText(e)));
	}
</script>

<section class="card rows">
	<div class="row">
		<span class="auto-icon" aria-hidden="true">
			{#if crewmate.project?.kind === "path"}<Folder size={15} />{:else}<GitBranch size={15} />{/if}
		</span>
		<div class="row-text auto-text">
			<span class="row-label">Project</span>
			{#if crewmate.project}
				<span class="row-hint auto-prompt" title={crewmate.project.url ?? crewmate.project.path}
					>{(crewmate.project.url ?? crewmate.project.path ?? crewmate.project.name ?? "").replace(
						/^https:\/\/github\.com\/|\.git$/g,
						"",
					)}{crewmate.project.branch ? ` · ${crewmate.project.branch}` : ""}</span
				>
			{:else}
				<span class="row-hint">Not linked. Pick one of your projects:</span>
			{/if}
		</div>
		{#if crewmate.project}
			<Button size="sm" variant="ghost" onclick={() => void unlinkProject()}>Unlink</Button>
		{/if}
	</div>
	{#if !crewmate.project}
		<div class="project-options">
			{#each ideProjects as p (p.id)}
				<button class="project-option" onclick={() => void linkProject(p)}>
					<GitBranch size={12} aria-hidden="true" />
					{p.name}
					{#if p.branch}<span class="muted">{p.branch}</span>{/if}
				</button>
			{:else}
				<p class="row-hint">No projects open in Atlas yet.</p>
			{/each}
		</div>
	{/if}
</section>

<style>
	.project-options {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.project-option {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		border: none;
		background: none;
		border-radius: 6px;
		text-align: left;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.project-option:hover {
		background: var(--ca-vibrancy-fill);
	}
	.muted {
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
</style>
