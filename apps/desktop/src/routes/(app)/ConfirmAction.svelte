<script lang="ts">
	import { Button, Dialog } from "@codeatlasdev/ui";
	import { pending, runAction } from "#lib/actions.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
</script>

<Dialog
	open={!!pending.value}
	onOpenChange={(o) => !o && (pending.value = null)}
	title={pending.value?.label}
	description={pending.value?.text}
	size="sm"
>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (pending.value = null)}>{m.common_action_cancel()}</Button>
		<Button
			variant="destructive"
			onclick={() => {
				const v = pending.value;
				pending.value = null;
				if (v) runAction(v.id, v.target, true);
			}}
		>
			{m.shell_home_confirm_action()}
		</Button>
	{/snippet}
</Dialog>
