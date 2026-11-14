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

	const KINDS: Array<{ kind: TriggerKind; label: string }> = [
		{ kind: "schedule", label: "Schedule" },
		{ kind: "interval", label: "Interval" },
		{ kind: "github", label: "GitHub" },
		{ kind: "file", label: "File" },
		{ kind: "crewmate", label: "After crewmate" },
		{ kind: "service", label: "Service" },
		{ kind: "webhook", label: "Webhook" },
	];
</script>

<Dialog bind:open title={isEdit ? "Edit automation" : "Add automation"} size="lg">
	<div class="body">
		<!-- Kind tabs -->
		<div class="kind-tabs" role="tablist" aria-label="Trigger kind">
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
			<label class="field-label" for="auto-prompt">What should {crewmateName || "the crewmate"} do?</label>
			<textarea
				id="auto-prompt"
				class="prompt-area"
				bind:value={form.prompt}
				rows={3}
				placeholder={form.kind === "schedule"
					? "Review the open PRs and post a summary"
					: form.kind === "github"
						? "A PR was opened: {{event}} — review it"
						: "Describe what the crewmate should do when this fires"}
				aria-label="Automation prompt"></textarea>
		</div>
	</div>

	{#snippet actions()}
		{#if isEdit}
			<Button variant="ghost" class="delete-action" onclick={handleDelete} aria-label="Delete automation">
				Delete
			</Button>
		{/if}
		<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
		<Button disabled={!canSave} onclick={handleSave}>
			{isEdit ? "Save" : "Add"}
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
