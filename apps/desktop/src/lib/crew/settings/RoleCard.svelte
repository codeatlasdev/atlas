<!-- "What it does": the crewmate's role text, autosaved on blur / Cmd+Enter. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

<section class="card">
	<div class="card-head">
		<span class="row-label">What it does</span>
		<SaveStatus field={settings.roleField} />
	</div>
	<p class="row-hint">Its job in a sentence or two. {crewmate.name} reads it before every task.</p>
	<textarea
		class="field-textarea"
		bind:value={settings.role}
		rows={4}
		maxlength={2000}
		aria-label="What this crewmate does"
		onblur={() => void settings.saveRole(crewmate)}
		onkeydown={(e) => {
			if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
				e.preventDefault();
				void settings.saveRole(crewmate);
			}
		}}></textarea>
</section>
