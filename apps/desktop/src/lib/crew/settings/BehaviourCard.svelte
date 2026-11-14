<!-- Behaviour: one row per setting (approval, short replies), words on the left, the select on the right. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

<section class="card rows">
	<div class="row">
		<div class="row-text">
			<span class="row-label">Approval <SaveStatus field={settings.approvalField} /></span>
			<span class="row-hint">
				{crewmate.approval_supported === false
					? "OpenCode crewmates run without approval prompts."
					: "Whether it asks you before running commands or changing files."}
			</span>
		</div>
		<select
			class="field-select compact"
			bind:value={settings.approval}
			disabled={crewmate.approval_supported === false}
			aria-label="Approval mode"
			onchange={() => void settings.saveApproval(crewmate)}
		>
			<option value="ask">Ask before acting</option>
			<option value="auto">Automatic</option>
		</select>
	</div>
	<div class="row">
		<div class="row-text">
			<span class="row-label">Short replies <SaveStatus field={settings.shortRepliesField} /></span>
			<span class="row-hint">Summarize long replies with a cheap model before showing them.</span>
		</div>
		<select
			class="field-select compact"
			bind:value={settings.shortReplies}
			aria-label="Short replies mode"
			onchange={() => void settings.saveShortReplies(crewmate)}
		>
			<option value="crew">Crew setting</option>
			<option value="on">Always</option>
			<option value="off">Never</option>
		</select>
	</div>
</section>
