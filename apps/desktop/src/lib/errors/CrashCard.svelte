<script lang="ts">
	/** "Closed unexpectedly last time" (#322): a dismissible card for a crash of an earlier session. */
	import { Button } from "@codeatlasdev/ui";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import { copy, crashReport } from "./crashCard.ts";
	import { crashCards } from "./crashCard.svelte.ts";
	import { errors } from "./errors.svelte.ts";

	let card = $derived(crashCards.card);
	let text = $derived(card ? copy(card) : null);

	/** The existing report screen, with the redacted summary and the diagnostics; nothing leaves until "Report issue". */
	async function report() {
		if (!card) return;
		errors.show(await errors.enrich({ ...crashReport(card), version: await errors.version() }));
	}
</script>

{#if card && text}
	{#snippet act()}
		<Button size="sm" onclick={report}>Report</Button>
		<Button size="sm" variant="glass" onclick={() => crashCards.dismiss()}>Dismiss</Button>
	{/snippet}
	<CornerNotice
		id="crash-card"
		kind="alert"
		tone="warning"
		title={text.title}
		body={text.body}
		actions={act}
		ondismiss={() => crashCards.dismiss()}
	/>
{/if}
