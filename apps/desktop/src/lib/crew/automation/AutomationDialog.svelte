<script lang="ts">
	/**
	 * AutomationDialog — create or edit a crewmate automation (trigger + prompt).
	 * Follows the CrewmateEditor.svelte pattern: kit Dialog + Button, CSS vars. The form state is `AutomationForm`,
	 * the per-kind fields are `AutomationFields`.
	 */
	import { Dialog, Button } from "@codeatlasdev/ui";
	import type { Trigger, TriggerKind } from "#lib/crew.ts";
	import { AutomationForm } from "#lib/crew/automation/automationForm.svelte.ts";
	import AutomationFields from "#lib/crew/automation/AutomationFields.svelte";
	import { TRIGGER_ICONS } from "#lib/crew/settings/triggerIcons.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		open?: boolean;
		/** Populated in edit mode; null/undefined in create mode. */
		trigger?: Trigger | null;
		crewmateName: string;
		/** IANA tz, e.g. "America/Sao_Paulo". Stored on the trigger and shown in the live sentence. */
		tz?: string;
		onsave?: (trigger: Trigger) => void;
		ondelete?: (id: string) => void;
	}

	let { open = $bindable(false), trigger = null, crewmateName, tz, onsave, ondelete }: Props = $props();

	const isEdit = $derived(trigger != null);
	const form = new AutomationForm();
	let saving = $state(false);
	const canSave = $derived(!saving && form.valid);

	// Each open starts from the trigger being edited, or blank.
	let prevOpen = false;
	$effect(() => {
		if (open && !prevOpen) form.load(trigger);
		prevOpen = open;
	});

	async function handleSave() {
		if (!canSave) return;
		const trig: Trigger = {
			id: trigger?.id ?? crypto.randomUUID(),
			kind: form.toKind(),
			prompt: form.prompt.trim(),
			enabled: trigger?.enabled ?? true,
			...(tz ? { tz } : {}),
		};
		saving = true;
		try {
			await onsave?.(trig);
		} finally {
			saving = false;
		}
	}

	async function handleDelete() {
		if (!trigger) return;
		await ondelete?.(trigger.id);
	}

	const KINDS = $derived<Array<{ kind: TriggerKind; label: string }>>([
		{ kind: "schedule", label: m.crew_automation_dialog_kind_schedule() },
		{ kind: "interval", label: m.crew_automation_dialog_kind_interval() },
		{ kind: "github", label: "GitHub" },
		{ kind: "file", label: m.crew_automation_dialog_kind_file() },
		{ kind: "crewmate", label: m.crew_automation_dialog_kind_crewmate() },
		{ kind: "service", label: m.crew_automation_fields_service() },
		{ kind: "webhook", label: m.crew_automation_dialog_kind_webhook() },
	]);

	const placeholder = $derived(
		form.kind === "schedule"
			? m.crew_automation_dialog_placeholder_schedule()
			: form.kind === "github"
				? // `{{event}}` is the template variable the automation fills in when it fires, not a message placeholder.
					m.crew_automation_dialog_placeholder_github({ event: "{{event}}" })
				: m.crew_automation_dialog_placeholder_other(),
	);
</script>

<Dialog
	bind:open
	title={isEdit ? m.crew_automation_dialog_title_edit() : m.crew_automation_dialog_title_add()}
	size="lg"
>
	<div class="body">
		<!-- Kind tabs -->
		<div class="kind-tabs" role="tablist" aria-label={m.crew_automation_dialog_trigger_kind()}>
			{#each KINDS as k (k.kind)}
				{@const Icon = TRIGGER_ICONS[k.kind]}
				<button
					role="tab"
					aria-selected={form.kind === k.kind}
					class:active={form.kind === k.kind}
					onclick={() => (form.kind = k.kind)}
				>
					<Icon size={13} aria-hidden="true" />
					{k.label}
				</button>
			{/each}
		</div>

		<AutomationFields {form} {trigger} {tz} />

		<!-- Prompt (all kinds) -->
		<div class="prompt-section">
			<label class="field-label" for="auto-prompt"
				>{crewmateName
					? m.crew_automation_dialog_prompt_label({ crewmate: crewmateName })
					: m.crew_automation_dialog_prompt_label_default()}</label
			>
			<textarea
				id="auto-prompt"
				class="prompt-area"
				bind:value={form.prompt}
				rows={3}
				{placeholder}
				aria-label={m.crew_automation_dialog_automation_prompt()}></textarea>
		</div>
	</div>

	{#snippet actions()}
		{#if isEdit}
			<Button
				variant="ghost"
				class="delete-action"
				onclick={handleDelete}
				aria-label={m.crew_automation_dialog_delete_automation()}
			>
				{m.common_action_delete()}
			</Button>
		{/if}
		<Button variant="ghost" onclick={() => (open = false)}>{m.common_action_cancel()}</Button>
		<Button disabled={!canSave} onclick={handleSave}>
			{isEdit ? m.common_action_save() : m.common_action_add()}
		</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 4px 24px 20px;
		min-width: 420px;
	}

	/* ── Kind tabs ──────────────────────────────────────────────────────────── */
	.kind-tabs {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}

	.kind-tabs button {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
		transition:
			background 80ms,
			color 80ms,
			border-color 80ms;
	}

	.kind-tabs button.active {
		background: color-mix(in oklab, var(--ca-brand-main) 12%, transparent);
		border-color: color-mix(in oklab, var(--ca-brand-main) 40%, transparent);
		color: var(--ca-text-primary);
	}

	.field-label {
		width: 80px;
		flex: none;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}

	/* ── Prompt ──────────────────────────────────────────────────────────────── */
	.prompt-section {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.prompt-area {
		width: 100%;
		box-sizing: border-box;
		font: inherit;
		font-size: 13px;
		padding: 9px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		resize: vertical;
		line-height: 1.4;
	}

	.prompt-area:focus {
		outline: none;
		border-color: var(--ca-action-focus);
	}

	/* ── Delete action (left-aligned in actions snippet) ─────────────────────── */
	:global(.delete-action) {
		color: var(--ca-semantic-error) !important;
		margin-right: auto;
	}
</style>
