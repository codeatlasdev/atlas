<!-- Memory: what the crewmate keeps between tasks, its own notes as readable text that fills the pane. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

{#if settings.memoryLoading}
	<div class="loading">{m.common_state_loading()}</div>
{:else}
	<section class="card memory-card">
		<div class="card-head">
			<span class="row-label"
				>{m.crew_settings_memory_section_what_name_remembers({ name: crewmate.name })}</span
			>
			<SaveStatus field={settings.memoryField} />
		</div>
		<p class="row-hint">
			{m.crew_settings_memory_section_notes_name_writes_for_itself({ name: crewmate.name })}
		</p>
		<textarea
			class="field-textarea memory-textarea"
			bind:value={settings.memory}
			placeholder={m.crew_settings_memory_section_nothing_yet_it_learns_as()}
			aria-label={m.crew_settings_memory_section_crewmate_memory()}
			onblur={() => void settings.saveMemory()}
			onkeydown={(e) => {
				if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
					e.preventDefault();
					void settings.saveMemory();
				}
			}}></textarea>
	</section>
{/if}

<style>
	.memory-card {
		flex: 1;
		min-height: 280px;
	}
	.memory-textarea {
		flex: 1;
		resize: none;
		font-size: 13px;
		line-height: 1.6;
		padding: 12px 14px;
	}
	.loading {
		color: var(--ca-text-secondary);
		font-size: 14px;
	}
</style>
