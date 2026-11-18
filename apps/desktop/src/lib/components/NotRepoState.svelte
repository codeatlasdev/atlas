<script lang="ts">
	import { Button, EmptyState, Spinner } from "@codeatlasdev/ui";
	import { GitBranch } from "@lucide/svelte";
	import { initRepository } from "#lib/stores/repoInit.ts";
	import { m } from "#lib/i18n/m.ts";

	let { worktreeId, onInit }: { worktreeId: string; onInit?: () => void } = $props();

	let busy = $state(false);

	async function init() {
		busy = true;
		try {
			if (await initRepository(worktreeId)) onInit?.();
		} finally {
			busy = false;
		}
	}
</script>

<EmptyState
	icon={GitBranch}
	title={m.git_not_repo_state_this_folder_is_not_a()}
	description="Source control, diffs and pull requests need git. Initialize a repository here to start tracking changes."
	bare
>
	{#snippet actions()}
		<Button size="sm" disabled={busy} onclick={init}>
			{#snippet leading()}{#if busy}<Spinner size={12} />{/if}{/snippet}
			{m.git_not_repo_state_initialize_repository()}
		</Button>
	{/snippet}
</EmptyState>
