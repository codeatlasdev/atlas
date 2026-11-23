<!-- Automations: the crewmate's triggers (toggle / edit from the list) and the add / edit dialog. -->
<script lang="ts">
	import { onMount } from "svelte";
	import { clock } from "#lib/clock.svelte.ts";
	import { Button } from "@codeatlasdev/ui";
	import type { Crewmate, Trigger, TriggerKind } from "#lib/crew.ts";
	import { describeNext, describeTrigger } from "#lib/crew/automations.ts";
	import { optionIn } from "#lib/crew/motion.ts";
	import { m } from "#lib/i18n/m.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import { triggerIcon } from "#lib/crew/settings/triggerIcons.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();

	const triggers = $derived(crewmate.triggers ?? []);

	let dialogOpen = $state(false);
	let editing = $state<Trigger | null>(null);

	// "Next: in 5 min" ticks.
	let now = $state(Date.now());
	onMount(() => clock.every(1000, () => (now = Date.now())));

	function openDialog(t: Trigger | null) {
		editing = t;
		dialogOpen = true;
	}

	async function save(trig: Trigger) {
		if (await settings.saveTrigger(triggers, trig)) dialogOpen = false;
	}

	async function remove(id: string) {
		if (await settings.deleteTrigger(triggers, id)) dialogOpen = false;
	}
</script>

{#if triggers.length > 0}
	<section class="card rows">
		{#each triggers as trig, i (trig.id)}
			{@const TrigIcon = triggerIcon(trig.kind.type as TriggerKind)}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="row automation-row"
				role="button"
				tabindex="0"
				aria-label={m.crew_settings_automations_section_edit_automation()}
				onclick={() => openDialog(trig)}
				onkeydown={(e) => e.key === "Enter" && openDialog(trig)}
				in:optionIn={{ index: i }}
			>
				<span class="auto-icon" aria-hidden="true"><TrigIcon size={15} /></span>
				<div class="row-text auto-text">
					<span class="row-label">{describeTrigger(trig.kind, trig.tz)}</span>
					{#if trig.prompt}<span class="row-hint auto-prompt">{trig.prompt}</span>{/if}
					{#if trig.invalid}
						<span class="row-hint auto-invalid"
							>{m.settings_automation_invalid({ reason: trig.invalid })}</span
						>
					{:else if trig.next_fire && trig.enabled}
						<span class="row-hint"
							>{m.crew_settings_automations_section_next({ when: describeNext(trig.next_fire, now) })}</span
						>
					{/if}
				</div>
				<!-- A switch, not a checkbox: on/off is the one thing to do from the row. -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<input
					class="switch"
					type="checkbox"
					role="switch"
					checked={trig.enabled}
					aria-label={m.crew_settings_automations_section_automation_on()}
					onclick={(e) => e.stopPropagation()}
					onchange={(e) =>
						void settings.toggleTrigger(triggers, trig.id, (e.currentTarget as HTMLInputElement).checked)}
				/>
			</div>
		{/each}
	</section>
{:else}
	<section class="card empty-card">
		<span class="row-label">{m.crew_settings_automations_section_no_automations_yet()}</span>
		<p class="row-hint">
			{m.crew_settings_automations_section_add_one_below_or_just({ name: crewmate.name })}
		</p>
	</section>
{/if}
<div class="pane-actions">
	<Button size="sm" variant="outlined" onclick={() => openDialog(null)}
		>{m.crew_settings_automations_section_add_automation()}</Button
	>
</div>

<!-- A chunk of its own: the dialog and its fields carry their messages, which must not grow the startup bundle. -->
{#await import("#lib/crew/automation/AutomationDialog.svelte") then { default: AutomationDialog }}
	<AutomationDialog
		bind:open={dialogOpen}
		trigger={editing}
		crewmateName={crewmate.name}
		tz={Intl.DateTimeFormat().resolvedOptions().timeZone}
		onsave={save}
		ondelete={remove}
	/>
{/await}

<style>
	.automation-row {
		justify-content: flex-start;
		gap: 12px;
		margin: 0 -16px;
		padding: 14px 16px;
		cursor: default;
		transition: background 80ms;
	}
	.automation-row:hover {
		background: var(--ca-vibrancy-fill);
	}
	.automation-row:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: -2px;
	}
	.card.rows .automation-row:first-child {
		border-top-left-radius: 12px;
		border-top-right-radius: 12px;
	}
	.card.rows .automation-row:last-child {
		border-bottom-left-radius: 12px;
		border-bottom-right-radius: 12px;
	}
	.auto-invalid {
		color: var(--ca-semantic-error);
	}
	.empty-card {
		align-items: flex-start;
	}

	/* iOS-style switch on a native checkbox (keeps keyboard and screen-reader behaviour). */
	.switch {
		appearance: none;
		flex: none;
		position: relative;
		width: 34px;
		height: 20px;
		margin: 0;
		border-radius: 999px;
		background: color-mix(in oklab, var(--ca-text-primary) 18%, transparent);
		cursor: default;
		transition: background 140ms;
	}
	.switch::after {
		content: "";
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
		transition: transform 140ms;
	}
	.switch:checked {
		background: var(--ca-semantic-success);
	}
	.switch:checked::after {
		transform: translateX(14px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: 2px;
	}
</style>
