<script lang="ts">
	/** Confirmation for deleting one or several worktrees (driven by `sidebar.deleteIds`). */
	import { Button, Checkbox, Dialog } from "@codeatlasdev/ui";
	import { sidebar } from "./state.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
</script>

<Dialog
	open={sidebar.deleteIds.length > 0}
	onOpenChange={(o) => !o && (sidebar.deleteIds = [])}
	title={m.sidebar_delete_dialog_title({ count: sidebar.deleteIds.length })}
	description={m.sidebar_delete_dialog_body()}
	size="sm"
>
	<Checkbox bind:checked={sidebar.deleteForce} label={m.sidebar_delete_dialog_force()} />
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (sidebar.deleteIds = [])}>{m.common_action_cancel()}</Button>
		<Button variant="destructive" onclick={() => sidebar.confirmDelete()}>{m.common_action_delete()}</Button>
	{/snippet}
</Dialog>
