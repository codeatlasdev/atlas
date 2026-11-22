<!-- Behaviour: one row per setting (permission mode, short replies), words on the left, the select on the right. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";
	import { modeLabel, modeLocked, modeOf, selectableModes } from "#lib/crew/permissionMode.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();

	const locked = $derived(modeLocked(crewmate));
	const modes = $derived(selectableModes(crew.policyOf(crew.selected?.hostKey ?? "")));

	// The mode changes under the dialog too (a phone, Telegram, the composer's picker): follow it unless a save is on its way.
	$effect(() => {
		const live = modeOf(crewmate);
		if (settings.modeField.status !== "saving") settings.mode = live;
	});
</script>

<section class="card rows">
	<div class="row">
		<div class="row-text">
			<span class="row-label"
				>{m.crew_permission_settings_mode()} <SaveStatus field={settings.modeField} /></span
			>
			<span class="row-hint">
				{locked ? m.crew_permission_fixed_reason() : m.crew_permission_settings_mode_hint()}
			</span>
		</div>
		<select
			class="field-select compact"
			bind:value={settings.mode}
			disabled={locked}
			aria-label={m.crew_permission_settings_mode()}
			onchange={() => void settings.saveMode(crewmate)}
		>
			{#each modes as option (option)}
				<option value={option}>{modeLabel(option)}</option>
			{/each}
			{#if !modes.includes(settings.mode)}
				<option value={settings.mode}>{modeLabel(settings.mode)}</option>
			{/if}
		</select>
	</div>
	<div class="row">
		<div class="row-text">
			<span class="row-label"
				>{m.crew_settings_behaviour_card_short_replies()}
				<SaveStatus field={settings.shortRepliesField} /></span
			>
			<span class="row-hint">{m.crew_settings_behaviour_card_summarize_long_replies_with_a()}</span>
		</div>
		<select
			class="field-select compact"
			bind:value={settings.shortReplies}
			aria-label={m.crew_settings_behaviour_card_short_replies_mode()}
			onchange={() => void settings.saveShortReplies(crewmate)}
		>
			<option value="crew">{m.crew_settings_behaviour_card_crew_setting()}</option>
			<option value="on">{m.crew_settings_behaviour_card_always()}</option>
			<option value="off">{m.crew_settings_behaviour_card_never()}</option>
		</select>
	</div>
</section>
