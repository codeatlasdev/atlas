<script lang="ts">
	import { Button, Kbd } from "@codeatlasdev/ui";
	import { Compass, Keyboard, ListChecks } from "@lucide/svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { progress } from "./checklist.ts";
	import { onboarding } from "./state.svelte.ts";

	let p = $derived(progress(onboarding.state));

	/** Settings is a dialog over the app: close it first so the tour can spotlight the UI. */
	function run(fn: () => void) {
		updates.open = false;
		setTimeout(fn, 180);
	}
</script>

<Group title="Help">
	<Row
		id="tour"
		label="Guided tour"
		description="A two-minute walk through spaces, the header, the side panel and search."
	>
		{#snippet lead()}<Compass size={16} />{/snippet}
		<Button variant="glass" size="sm" onclick={() => run(() => onboarding.startTour())}>Take the tour</Button>
	</Row>
	<Row id="help-keys" label="Shortcut cheat sheet" description="Every key in one searchable list.">
		{#snippet lead()}<Keyboard size={16} />{/snippet}
		<Kbd>{shortcutFor("atlas::Help") ?? "?"}</Kbd>
		<Button variant="glass" size="sm" onclick={() => run(() => (onboarding.helpOpen = true))}>
			Open cheat sheet
		</Button>
	</Row>
	<Row
		id="checklist"
		label="Getting started checklist"
		description={`${p.done} of ${p.total} first steps done${onboarding.state.checklist.dismissed ? ". The card is hidden." : "."}`}
	>
		{#snippet lead()}<ListChecks size={16} />{/snippet}
		{#if onboarding.state.checklist.dismissed}
			<Button variant="glass" size="sm" onclick={() => run(() => void onboarding.showChecklist())}
				>Show the card</Button
			>
		{/if}
		<Button variant="ghost" size="sm" onclick={() => run(() => void onboarding.restart())}>Start over</Button>
	</Row>
</Group>
