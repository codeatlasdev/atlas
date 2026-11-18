<!-- Audit: everything the crewmate did and was asked, append-only on the crew server: tool calls with their input,
     approvals and who answered, tasks, setting changes. Filter by group; Copy puts the whole entry (nothing shortened,
     secrets already replaced by hostd) on the clipboard. -->
<script lang="ts">
	import { toast } from "@codeatlasdev/ui";
	import { Copy, RefreshCw } from "@lucide/svelte";
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import {
		AUDIT_GROUPS,
		auditCopyText,
		auditGroupLabel,
		auditKindLabel,
		auditSummary,
		filterAudit,
		type AuditGroup,
	} from "#lib/crew/settings/audit.ts";
	import { dateTime } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();

	let group = $state<AuditGroup | null>(null);
	const shown = $derived(filterAudit(settings.auditEntries, group));

	// Loaded when the section opens, not with the dialog: a long log is not worth reading until someone asks.
	$effect(() => {
		void settings.loadAudit();
	});

	const when = (ts: number) => dateTime(ts * 1000, { dateStyle: "short", timeStyle: "medium" });

	function copy(text: string) {
		navigator.clipboard.writeText(text).then(
			() => toast.success(m.crew_settings_audit_section_copied()),
			() => toast.error(m.crew_settings_audit_section_could_not_copy()),
		);
	}
</script>

<section class="card audit-card">
	<div class="card-head">
		<span class="row-label">{m.crew_settings_audit_section_what_name_did({ name: crewmate.name })}</span>
		<span class="head-actions">
			<select
				class="field-select compact"
				bind:value={group}
				aria-label={m.crew_settings_audit_section_filter_by_kind()}
			>
				<option value={null}>{m.crew_settings_audit_section_everything()}</option>
				{#each AUDIT_GROUPS as g (g)}
					<option value={g}>{auditGroupLabel(g)}</option>
				{/each}
			</select>
			<button
				class="icon-btn"
				aria-label={m.crew_settings_audit_section_refresh()}
				title={m.crew_settings_audit_section_refresh()}
				onclick={() => void settings.loadAudit()}
			>
				<RefreshCw size={14} />
			</button>
			<button
				class="icon-btn"
				aria-label={m.crew_settings_audit_section_copy_everything_shown()}
				title={m.crew_settings_audit_section_copy_everything_shown()}
				disabled={!shown.length}
				onclick={() => copy(shown.map(auditCopyText).join("\n\n"))}
			>
				<Copy size={14} />
			</button>
		</span>
	</div>
	<p class="row-hint">
		{m.crew_settings_audit_section_written_by_the_crew_server({ name: crewmate.name })}
	</p>

	{#if settings.auditLoading && !settings.auditEntries.length}
		<div class="empty">{m.common_state_loading()}</div>
	{:else if settings.auditError}
		<div class="empty">
			{m.crew_settings_audit_section_could_not_read_the_audit({ auditError: settings.auditError })}
		</div>
	{:else if !shown.length}
		<div class="empty">
			{settings.auditEntries.length
				? m.crew_settings_audit_section_nothing_of_this_kind()
				: m.crew_settings_audit_section_nothing_recorded_yet()}
		</div>
	{:else}
		<ul class="entries" role="list">
			{#each shown as e (e.seq)}
				<li class="entry">
					<span class="when">{when(e.ts)}</span>
					<span class="kind kind-{e.kind}">{auditKindLabel(e.kind)}</span>
					<span class="summary" title={auditSummary(e)}>{auditSummary(e)}</span>
					<button
						class="icon-btn"
						aria-label={m.crew_settings_audit_section_copy_this_entry()}
						title={m.crew_settings_audit_section_copy_this_entry()}
						onclick={() => copy(auditCopyText(e))}><Copy size={13} /></button
					>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.audit-card {
		flex: 1;
		min-height: 280px;
		min-width: 0;
	}
	.head-actions {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.icon-btn {
		display: inline-grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.icon-btn:hover:not(:disabled) {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.icon-btn:disabled {
		opacity: 0.4;
	}
	.icon-btn:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: -2px;
	}
	.entries {
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-y: auto;
		min-height: 0;
	}
	.entry {
		display: grid;
		grid-template-columns: 9.5em 8.5em minmax(0, 1fr) auto;
		align-items: center;
		gap: 10px;
		padding: 6px 0;
		font-size: 12px;
	}
	.entry + .entry {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.when {
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.kind {
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.kind-approval_decision,
	.kind-approval_asked {
		color: var(--ca-brand-main);
	}
	.kind-secret_set,
	.kind-config_changed {
		color: var(--ca-semantic-warning, var(--ca-text-primary));
	}
	.summary {
		min-width: 0;
		color: var(--ca-text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--ca-font-mono, ui-monospace, monospace);
	}
	.empty {
		color: var(--ca-text-secondary);
		font-size: 13px;
		padding: 12px 0;
	}
</style>
