<!-- "What it does": the crewmate's role text, autosaved on blur / Cmd+Enter. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

<section class="card">
	<div class="card-head">
		<span class="row-label">{m.crew_settings_role_card_what_it_does()}</span>
		<SaveStatus field={settings.roleField} />
	</div>
	<p class="row-hint">{m.crew_settings_role_card_its_job_in_a_sentence({ name: crewmate.name })}</p>
	<textarea
		class="field-textarea"
		bind:value={settings.role}
		rows={4}
		maxlength={2000}
		aria-label={m.crew_settings_role_card_what_this_crewmate_does()}
		onblur={() => void settings.saveRole(crewmate)}
		onkeydown={(e) => {
			if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
				e.preventDefault();
				void settings.saveRole(crewmate);
			}
		}}></textarea>
</section>
