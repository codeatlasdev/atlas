<script lang="ts">
	import { Button, EmptyState, Skeleton } from "@codeatlasdev/ui";
	import { FolderPlus } from "@lucide/svelte";
	import { open as pickFolder } from "@tauri-apps/plugin-dialog";
	import { api } from "#lib/api.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { showStanding } from "#lib/onboarding/flow.ts";
	import Hero from "#lib/onboarding/Hero.svelte";
	import { onboarding } from "#lib/onboarding/state.svelte.ts";
	import { welcome } from "#lib/onboarding/welcome.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";

	async function add() {
		const dir = await pickFolder({ directory: true, title: "Add atlas project" });
		if (typeof dir !== "string") return;
		const v = await store.run("add", () => api.add(dir));
		if (v) {
			await store.refresh();
			store.selected = v.id;
		}
	}
</script>

<div class="drag-strip" data-tauri-drag-region></div>
<div class="empty">
	{#if !store.loaded}
		<div class="loading" aria-busy="true">
			<Skeleton variant="rounded" width={220} height={22} />
			<Skeleton variant="text" rows={2} width={320} />
		</div>
	{:else if store.projects.length === 0}
		<!-- First run: the full-window overlay owns the screen; this pane waits for it to be over. -->
		{#if showStanding(onboarding.state, onboarding.loaded, store.projects.length)}
			<Hero onfinish={() => welcome.reset()} />
		{/if}
	{:else}
		<EmptyState
			icon={FolderPlus}
			title="No project selected"
			description="Pick a worktree in the sidebar, or press {shortcutFor('atlas::Jump') ?? '⌘K'} to jump."
		>
			{#snippet actions()}
				<Button variant="glass" onclick={add}>Add project…</Button>
			{/snippet}
		</EmptyState>
	{/if}
</div>

<style>
	.drag-strip {
		height: var(--title-row);
	}
	.empty {
		position: relative;
		flex: 1;
		display: grid;
		place-items: center;
	}
	.loading {
		display: flex;
		flex-direction: column;
		gap: 10px;
		align-items: center;
	}
</style>
