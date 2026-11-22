<script lang="ts">
	import { Button, Kbd } from "@codeatlasdev/ui";
	import { Compass, Keyboard, ListChecks } from "@lucide/svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { progress } from "./checklist.ts";
	import { onboarding } from "./state.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let p = $derived(progress(onboarding.state));

	/** Settings is a dialog over the app: close it first so the tour can spotlight the UI. */
	function run(fn: () => void) {
		updates.open = false;
		setTimeout(fn, 180);
	}
</script>

<Group title={m.onboarding_help_settings_help()}>
	<Row
		id="tour"
		label={m.onboarding_help_settings_tour_label()}
		description={m.onboarding_help_settings_tour_description()}
	>
		{#snippet lead()}<Compass size={16} />{/snippet}
		<Button variant="glass" size="sm" onclick={() => run(() => onboarding.startTour())}
			>{m.onboarding_welcome_take_the_tour()}</Button
		>
	</Row>
	<Row
		id="help-keys"
		label={m.onboarding_help_settings_shortcuts_label()}
		description={m.onboarding_help_settings_shortcuts_description()}
	>
		{#snippet lead()}<Keyboard size={16} />{/snippet}
		<Kbd>{shortcutFor("atlas::Help") ?? "?"}</Kbd>
		<Button variant="glass" size="sm" onclick={() => run(() => (onboarding.helpOpen = true))}>
			{m.onboarding_help_settings_open_shortcuts()}
		</Button>
	</Row>
	<Row
		id="checklist"
		label={m.onboarding_help_settings_checklist_label()}
		description={onboarding.state.checklist.dismissed
			? m.onboarding_help_settings_progress_hidden({ done: p.done, total: p.total })
			: m.onboarding_help_settings_progress({ done: p.done, total: p.total })}
	>
		{#snippet lead()}<ListChecks size={16} />{/snippet}
		{#if onboarding.state.checklist.dismissed}
			<Button variant="glass" size="sm" onclick={() => run(() => void onboarding.showChecklist())}
				>{m.onboarding_help_settings_show_checklist()}</Button
			>
		{/if}
		<Button variant="ghost" size="sm" onclick={() => run(() => void onboarding.restart())}
			>{m.onboarding_help_settings_start_over()}</Button
		>
	</Row>
</Group>
