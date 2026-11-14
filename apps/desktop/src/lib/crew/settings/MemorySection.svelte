<!-- Memory: what the crewmate keeps between tasks, its own notes as readable text that fills the pane. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import SaveStatus from "#lib/crew/settings/SaveStatus.svelte";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

{#if settings.memoryLoading}
	<div class="loading">Loading…</div>
{:else}
	<section class="card memory-card">
		<div class="card-head">
			<span class="row-label">What {crewmate.name} remembers</span>
			<SaveStatus field={settings.memoryField} />
		</div>
		<p class="row-hint">
			Notes {crewmate.name} writes for itself and reads before every task. Edit or remove anything that is wrong.
		</p>
		<textarea
			class="field-textarea memory-textarea"
			bind:value={settings.memory}
			placeholder="Nothing yet. It learns as it works."
			aria-label="Crewmate memory"
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
