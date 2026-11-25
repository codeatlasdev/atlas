<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	// Shown once per update; the store clears it when dismissed or when the sheet opens.
	let v = $derived(updates.updatedTo);
	/** One line about this release: its summary when the changelog has one, else the first change. */
	let summary = $state("");
	$effect(() => {
		summary = "";
		if (!v) return;
		let live = true;
		void updates.summaryOf(v).then((t) => live && (summary = t));
		return () => (live = false);
	});
</script>

<!-- Good news, not urgent: a suggestion, so it waits behind anything that needs attention. -->
{#if v && !updates.whatsNewOpen}
	{#snippet act()}
		<Button size="sm" variant="glass" onclick={updates.showWhatsNew}>
			{m.settings_updates_card_see_whats_new()}
		</Button>
	{/snippet}
	<CornerNotice
		id="updated"
		kind="suggestion"
		tone="success"
		title={m.settings_updates_card_title({ version: v })}
		body={summary || m.settings_updates_card_fallback_body()}
		actions={act}
		ondismiss={updates.dismissUpdated}
	/>
{/if}
