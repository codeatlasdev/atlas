<script lang="ts">
	/** The one-time offer of the Ghostty theme. `Use theme` only opens Settings; nothing is applied here. */
	import { Button } from "@codeatlasdev/ui";
	import { Ghost } from "@lucide/svelte";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import { corner } from "#lib/shell/corner.svelte.ts";
	import { ghosttyOffer, openGhosttyOffer } from "#lib/shell/themeCommands.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	/** How long it stays once it is on screen: there is a choice to make. */
	const LIFETIME_MS = 15_000;

	let live = $derived(ghosttyOffer.shown && themes.ghostty.offered);

	// The clock starts when the card is on screen, not while it waits behind a toast or another suggestion.
	$effect(() => {
		if (!live || !corner.shown("ghostty")) return;
		const t = setTimeout(() => (ghosttyOffer.shown = false), LIFETIME_MS);
		return () => clearTimeout(t);
	});
</script>

{#if live}
	{#snippet act()}
		<Button size="sm" onclick={openGhosttyOffer}>{m.settings_theme_ghostty_offer_use_theme()}</Button>
	{/snippet}
	<CornerNotice
		id="ghostty"
		kind="suggestion"
		tone="brand"
		icon={Ghost}
		title={m.settings_theme_ghostty_offer_title()}
		body={m.settings_theme_ghostty_offer_body()}
		actions={act}
		ondismiss={() => (ghosttyOffer.shown = false)}
	/>
{/if}
