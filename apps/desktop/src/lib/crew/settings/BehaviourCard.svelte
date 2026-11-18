<!-- Behaviour: one row per setting (approval, short replies), words on the left, the select on the right. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

<section class="card rows">
	<div class="row">
		<div class="row-text">
			<span class="row-label"
				>{m.crew_settings_behaviour_card_approval()} <SaveStatus field={settings.approvalField} /></span
			>
			<span class="row-hint">
				{crewmate.approval_supported === false
					? m.crew_settings_behaviour_card_opencode_no_approval()
					: m.crew_settings_behaviour_card_asks_before_acting()}
			</span>
		</div>
		<select
			class="field-select compact"
			bind:value={settings.approval}
			disabled={crewmate.approval_supported === false}
			aria-label={m.crew_settings_behaviour_card_approval_mode()}
			onchange={() => void settings.saveApproval(crewmate)}
		>
			<option value="ask">{m.crew_settings_behaviour_card_ask_before_acting()}</option>
			<option value="auto">{m.crew_settings_behaviour_card_automatic()}</option>
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
