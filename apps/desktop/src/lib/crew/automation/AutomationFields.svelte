<!--
  The fields of the chosen trigger kind in the add / edit automation dialog (schedule builder or cron, interval,
  GitHub, file, after-crewmate, service, webhook). State is the AutomationForm the dialog owns.
-->
<script lang="ts">
	import type { Trigger } from "#lib/crew.ts";
	import type { AutomationForm } from "#lib/crew/automation/automationForm.svelte.ts";
	import { dateTime } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	let { form, trigger, tz }: { form: AutomationForm; trigger: Trigger | null; tz?: string } = $props();

	const sentence = $derived(form.liveSentence(tz));

	const GITHUB_EVENTS = $derived([
		{ ev: "pr_opened", label: m.crew_automation_fields_github_pr_opened() },
		{ ev: "pr_review_requested", label: m.crew_automation_fields_github_review_requested() },
		{ ev: "issue_opened", label: m.crew_automation_fields_github_issue_opened() },
		{ ev: "ci_failed", label: m.crew_automation_fields_github_ci_failed() },
		{ ev: "release", label: m.crew_automation_fields_github_release() },
		{ ev: "mention", label: m.crew_automation_fields_github_mention() },
		{ ev: "issue_assigned", label: m.crew_automation_fields_github_issue_assigned() },
		{ ev: "pr_review_comment", label: m.crew_automation_fields_github_review_comment() },
	]);

	// Sunday-first, like `scheduleDays`; 2024-01-07 is a Sunday. Weekday names come from Intl, never typed.
	const dayLabels = $derived(
		Array.from({ length: 7 }, (_, d) => ({
			short: dateTime(new Date(2024, 0, 7 + d), { weekday: "narrow" }),
			full: dateTime(new Date(2024, 0, 7 + d), { weekday: "short" }),
		})),
	);
</script>

<div class="fields">
	{#if form.kind === "schedule"}
		{#if !form.customCronMode}
			<!-- Day chips -->
			<div class="field-row">
				<span class="field-label">{m.crew_automation_fields_days()}</span>
				<div class="day-chips" role="group" aria-label={m.crew_automation_fields_days_of_week()}>
					{#each dayLabels as day, d}
						<button
							class="day-chip"
							class:on={form.scheduleDays.includes(d)}
							aria-pressed={form.scheduleDays.includes(d)}
							aria-label={day.full}
							onclick={() => form.toggleDay(d)}
						>
							{day.short}
						</button>
					{/each}
				</div>
			</div>

			<!-- Presets -->
			<div class="field-row">
				<span class="field-label"></span>
				<div class="presets">
					<button
						class="preset-btn"
						class:active={form.scheduleDays.length === 7}
						onclick={() => (form.scheduleDays = [0, 1, 2, 3, 4, 5, 6])}
					>
						{m.crew_automation_fields_every_day()}
					</button>
					<button
						class="preset-btn"
						class:active={[...form.scheduleDays].sort().join() === "1,2,3,4,5"}
						onclick={() => (form.scheduleDays = [1, 2, 3, 4, 5])}
					>
						{m.crew_automation_fields_weekdays()}
					</button>
				</div>
			</div>

			<!-- Time -->
			<div class="field-row">
				<label class="field-label" for="schedule-time">{m.crew_automation_fields_time()}</label>
				<input
					id="schedule-time"
					class="time-input"
					type="time"
					value={`${String(form.scheduleHour).padStart(2, "0")}:${String(form.scheduleMinute).padStart(2, "0")}`}
					onchange={(e) => {
						const [h, min] = (e.currentTarget as HTMLInputElement).value.split(":").map(Number);
						if (!isNaN(h) && !isNaN(min)) {
							form.scheduleHour = h;
							form.scheduleMinute = min;
						}
					}}
				/>
			</div>

			<!-- Live sentence -->
			{#if sentence}
				<p class="live-sentence">{sentence}</p>
			{/if}
		{:else}
			<!-- Custom cron input -->
			<div class="field-row">
				<label class="field-label" for="custom-cron">{m.crew_automation_fields_cron()}</label>
				<input
					id="custom-cron"
					class="cron-input"
					bind:value={form.customCron}
					placeholder="0 9 * * 1-5"
					aria-label={m.crew_automation_fields_cron_expression()}
				/>
			</div>
			{#if sentence}
				<p class="live-sentence">{sentence}</p>
			{/if}
		{/if}

		<button class="toggle-cron" onclick={() => (form.customCronMode = !form.customCronMode)}>
			{#if form.customCronMode}← {m.crew_automation_fields_use_builder()}{:else}{m.crew_automation_fields_custom_cron()}
				→{/if}
		</button>
	{:else if form.kind === "interval"}
		<div class="field-row">
			<label class="field-label" for="interval-val">{m.crew_automation_fields_every()}</label>
			<div class="inline-row">
				<input
					id="interval-val"
					class="num-input"
					type="number"
					min="1"
					bind:value={form.intervalValue}
					aria-label={m.crew_automation_fields_interval_amount()}
				/>
				<select
					class="unit-select"
					bind:value={form.intervalUnit}
					aria-label={m.crew_automation_fields_interval_unit()}
				>
					<option value="minutes">{m.crew_automation_fields_unit_minutes()}</option>
					<option value="hours">{m.crew_automation_fields_unit_hours()}</option>
					<option value="days">{m.crew_automation_fields_unit_days()}</option>
				</select>
			</div>
		</div>
	{:else if form.kind === "github"}
		<div class="field-row">
			<label class="field-label" for="gh-repo">{m.crew_automation_fields_repo()}</label>
			<!-- i18n-ignore: an example owner/repo name, not prose -->
			<input
				id="gh-repo"
				class="text-input"
				bind:value={form.githubRepo}
				placeholder="owner/repo"
				aria-label={m.crew_automation_fields_github_repo()}
			/>
		</div>
		<div class="field-row">
			<span class="field-label">{m.crew_automation_fields_events()}</span>
			<div class="check-list">
				{#each GITHUB_EVENTS as ev}
					<label class="check-item">
						<input
							type="checkbox"
							checked={form.githubEvents.has(ev.ev)}
							onchange={(e) => form.toggleGithubEvent(ev.ev, (e.currentTarget as HTMLInputElement).checked)}
							aria-label={ev.label}
						/>
						{ev.label}
					</label>
				{/each}
			</div>
		</div>
	{:else if form.kind === "file"}
		<div class="field-row">
			<label class="field-label" for="file-glob">{m.crew_automation_fields_glob()}</label>
			<!-- i18n-ignore: an example glob pattern, not prose -->
			<input
				id="file-glob"
				class="text-input"
				bind:value={form.fileGlob}
				placeholder="**/*.ts"
				aria-label={m.crew_automation_fields_file_glob()}
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="file-path">{m.crew_automation_fields_path()}</label>
			<!-- i18n-ignore: an example path, not prose -->
			<input
				id="file-path"
				class="text-input"
				bind:value={form.filePath}
				placeholder="/path/to/dir"
				aria-label={m.crew_automation_fields_watch_path()}
			/>
		</div>
	{:else if form.kind === "crewmate"}
		<div class="field-row">
			<label class="field-label" for="crewmate-from">{m.crew_automation_fields_crewmate_name()}</label>
			<input
				id="crewmate-from"
				class="text-input"
				bind:value={form.crewmateChainFrom}
				placeholder={m.crew_automation_fields_crewmate_name_or_id()}
				aria-label={m.crew_automation_fields_source_crewmate()}
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="crewmate-on">{m.crew_automation_fields_when()}</label>
			<select
				id="crewmate-on"
				class="unit-select"
				bind:value={form.crewmateChainOn}
				aria-label={m.crew_automation_fields_when_to_trigger()}
			>
				<option value="done">{m.crew_automation_fields_finishes()}</option>
				<option value="failed">{m.crew_automation_fields_fails()}</option>
			</select>
		</div>
	{:else if form.kind === "service"}
		<div class="field-row">
			<label class="field-label" for="svc-name">{m.crew_automation_fields_service()}</label>
			<input
				id="svc-name"
				class="text-input"
				bind:value={form.serviceName}
				placeholder={m.crew_automation_fields_service_name()}
				aria-label={m.crew_automation_fields_service_name()}
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="svc-dir">{m.crew_automation_fields_project_dir()}</label>
			<!-- i18n-ignore: an example path, not prose -->
			<input
				id="svc-dir"
				class="text-input"
				bind:value={form.serviceProjectDir}
				placeholder="/path/to/project"
				aria-label={m.crew_automation_fields_project_directory()}
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="svc-on">{m.crew_automation_fields_when()}</label>
			<select
				id="svc-on"
				class="unit-select"
				bind:value={form.serviceOn}
				aria-label={m.crew_automation_fields_when_to_trigger()}
			>
				<option value="failed">{m.crew_automation_fields_fails()}</option>
				<option value="recovered">{m.crew_automation_fields_recovers()}</option>
			</select>
		</div>
	{:else if form.kind === "webhook"}
		<p class="hint">
			{m.crew_automation_fields_webhook_hint()}
			<code>/crew/&#123;crewmate-id&#125;/hook/&#123;token&#125;</code>
		</p>
		{#if trigger?.kind.type === "webhook"}
			<div class="field-row">
				<span class="field-label">{m.crew_automation_fields_token()}</span>
				<code class="token-display">{trigger.kind.token}</code>
			</div>
		{/if}
	{/if}
</div>

<style>
	/* ── Fields ─────────────────────────────────────────────────────────────── */
	.fields {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.field-row {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.field-label {
		width: 80px;
		flex: none;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}

	/* ── Day chips ───────────────────────────────────────────────────────────── */
	.day-chips {
		display: flex;
		gap: 4px;
	}

	.day-chip {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition:
			background 80ms,
			color 80ms,
			border-color 80ms;
	}

	.day-chip.on {
		background: var(--ca-brand-main);
		border-color: var(--ca-brand-main);
		color: var(--ca-surface-base);
	}

	/* ── Presets ─────────────────────────────────────────────────────────────── */
	.presets {
		display: flex;
		gap: 6px;
	}

	.preset-btn {
		font: inherit;
		font-size: 11px;
		padding: 3px 9px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
		transition:
			background 80ms,
			border-color 80ms,
			color 80ms;
	}

	.preset-btn.active,
	.preset-btn:hover {
		background: color-mix(in oklab, var(--ca-brand-main) 10%, transparent);
		border-color: color-mix(in oklab, var(--ca-brand-main) 35%, transparent);
		color: var(--ca-text-primary);
	}

	/* ── Live sentence ────────────────────────────────────────────────────────── */
	.live-sentence {
		font-size: 13px;
		color: var(--ca-text-primary);
		margin: 0;
		padding: 6px 10px;
		border-radius: 6px;
		background: color-mix(in oklab, var(--ca-brand-main) 6%, var(--ca-surface-sunken));
		border: 0.5px solid color-mix(in oklab, var(--ca-brand-main) 20%, transparent);
	}

	/* ── Custom cron toggle ──────────────────────────────────────────────────── */
	.toggle-cron {
		background: none;
		border: none;
		font: inherit;
		font-size: 11px;
		color: var(--ca-text-secondary);
		cursor: pointer;
		padding: 0;
		text-align: left;
		width: fit-content;
	}

	.toggle-cron:hover {
		color: var(--ca-text-primary);
		text-decoration: underline;
	}

	/* ── Inputs ─────────────────────────────────────────────────────────────── */
	.time-input,
	.cron-input,
	.num-input,
	.text-input,
	.unit-select {
		height: 32px;
		padding: 0 8px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 13px;
		outline: none;
	}

	.time-input:focus,
	.cron-input:focus,
	.num-input:focus,
	.text-input:focus,
	.unit-select:focus {
		border-color: var(--ca-action-focus);
	}

	.time-input {
		width: 120px;
	}

	.cron-input {
		width: 160px;
		font-family: var(--ca-font-mono, monospace);
		font-size: 12px;
	}

	.num-input {
		width: 72px;
	}

	.text-input {
		flex: 1;
		min-width: 0;
		width: 100%;
	}

	.unit-select {
		appearance: none;
		padding-right: 28px;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%239aa3b5' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 8px center;
		cursor: pointer;
	}

	.inline-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	/* ── GitHub events ───────────────────────────────────────────────────────── */
	.check-list {
		display: flex;
		flex-wrap: wrap;
		gap: 6px 14px;
	}

	.check-item {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		color: var(--ca-text-primary);
		cursor: pointer;
	}

	/* ── Hint / token ────────────────────────────────────────────────────────── */
	.hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	.token-display {
		font-family: var(--ca-font-mono, monospace);
		font-size: 12px;
		color: var(--ca-text-primary);
		background: var(--ca-surface-sunken);
		padding: 2px 6px;
		border-radius: 4px;
	}
</style>
