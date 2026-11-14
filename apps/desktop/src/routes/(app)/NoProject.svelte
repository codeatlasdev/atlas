<script lang="ts">
	import { Alert, Button, EmptyState, Skeleton } from "@codeatlasdev/ui";
	import { FolderPlus } from "@lucide/svelte";
	import { addProject } from "#lib/components/sidebar/projects.ts";
	import { kbd } from "#lib/keys.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { showStanding } from "#lib/onboarding/flow.ts";
	import Hero from "#lib/onboarding/Hero.svelte";
	import { onboarding } from "#lib/onboarding/state.svelte.ts";
	import { welcome } from "#lib/onboarding/welcome.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";

	// The standing state is a fresh start: after "Skip for now" the flow sits at `done`, which renders no steps,
	// and the pane showed a bare globe with nothing to click (Windows first-run pass, 1.5.0-alpha.24).
	$effect(() => {
		if (showStanding(onboarding.state, onboarding.ready, store.projects.length) && welcome.phase === "done")
			welcome.reset();
	});
</script>

<div class="drag-strip" data-tauri-drag-region></div>
<div class="empty">
	{#if !store.loaded}
		<div class="loading" aria-busy="true">
			<Skeleton variant="rounded" width={220} height={22} />
			<Skeleton variant="text" rows={2} width={320} />
		</div>
	{:else if store.listError}
		<!-- An unreadable registry is not an empty one: say why, with the path in the message, and offer a re-read. -->
		<Alert severity="error" title="Could not read your projects">
			<div class="err">
				<span>{store.listError}</span>
				<Button size="sm" variant="glass" onclick={() => store.refresh()}>Try again</Button>
			</div>
		</Alert>
	{:else if store.projects.length === 0}
		<!-- First run: the full-window overlay owns the screen; this pane waits for it to be over. -->
		{#if showStanding(onboarding.state, onboarding.ready, store.projects.length)}
			<Hero onfinish={() => welcome.reset()} />
		{/if}
	{:else}
		<EmptyState
			icon={FolderPlus}
			title="No project selected"
			description="Pick a worktree in the sidebar, or press {shortcutFor('atlas::Jump') ??
				kbd('cmd-k')} to jump."
		>
			{#snippet actions()}
				<Button variant="glass" onclick={addProject}>Add project…</Button>
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
	.err {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		max-width: 520px;
		overflow-wrap: anywhere;
		user-select: text;
	}
	.loading {
		display: flex;
		flex-direction: column;
		gap: 10px;
		align-items: center;
	}
</style>
