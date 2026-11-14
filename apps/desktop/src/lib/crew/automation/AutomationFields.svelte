<!--
  The fields of the chosen trigger kind in the add / edit automation dialog (schedule builder or cron, interval,
  GitHub, file, after-crewmate, service, webhook). State is the AutomationForm the dialog owns.
-->
<script lang="ts">
	import type { Trigger } from "#lib/crew.ts";
	import type { AutomationForm } from "#lib/crew/automation/automationForm.svelte.ts";

	let { form, trigger, tz }: { form: AutomationForm; trigger: Trigger | null; tz?: string } = $props();

	const sentence = $derived(form.liveSentence(tz));

	const GITHUB_EVENTS = [
		{ ev: "pr_opened", label: "PR opened" },
		{ ev: "pr_review_requested", label: "Review requested" },
		{ ev: "issue_opened", label: "Issue opened" },
		{ ev: "ci_failed", label: "CI failed" },
		{ ev: "release", label: "Release" },
	];

	const DAY_LABELS = ["S", "M", "T", "W", "T", "F", "S"] as const;
	const DAY_FULL = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;
</script>

<div class="fields">
	{#if form.kind === "schedule"}
		{#if !form.customCronMode}
			<!-- Day chips -->
			<div class="field-row">
				<span class="field-label">Days</span>
				<div class="day-chips" role="group" aria-label="Days of week">
					{#each DAY_LABELS as label, d}
						<button
							class="day-chip"
							class:on={form.scheduleDays.includes(d)}
							aria-pressed={form.scheduleDays.includes(d)}
							aria-label={DAY_FULL[d]}
							onclick={() => form.toggleDay(d)}
						>
							{label}
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
						Every day
					</button>
					<button
						class="preset-btn"
						class:active={[...form.scheduleDays].sort().join() === "1,2,3,4,5"}
						onclick={() => (form.scheduleDays = [1, 2, 3, 4, 5])}
					>
						Weekdays
					</button>
				</div>
			</div>

			<!-- Time -->
			<div class="field-row">
				<label class="field-label" for="schedule-time">Time</label>
				<input
					id="schedule-time"
					class="time-input"
					type="time"
					value={`${String(form.scheduleHour).padStart(2, "0")}:${String(form.scheduleMinute).padStart(2, "0")}`}
					onchange={(e) => {
						const [h, m] = (e.currentTarget as HTMLInputElement).value.split(":").map(Number);
						if (!isNaN(h) && !isNaN(m)) {
							form.scheduleHour = h;
							form.scheduleMinute = m;
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
				<label class="field-label" for="custom-cron">Cron</label>
				<input
					id="custom-cron"
					class="cron-input"
					bind:value={form.customCron}
					placeholder="0 9 * * 1-5"
					aria-label="Cron expression"
				/>
			</div>
			{#if sentence}
				<p class="live-sentence">{sentence}</p>
			{/if}
		{/if}

		<button class="toggle-cron" onclick={() => (form.customCronMode = !form.customCronMode)}>
			{form.customCronMode ? "← Use builder" : "Custom cron →"}
		</button>
	{:else if form.kind === "interval"}
		<div class="field-row">
			<label class="field-label" for="interval-val">Every</label>
			<div class="inline-row">
				<input
					id="interval-val"
					class="num-input"
					type="number"
					min="1"
					bind:value={form.intervalValue}
					aria-label="Interval amount"
				/>
				<select class="unit-select" bind:value={form.intervalUnit} aria-label="Interval unit">
					<option value="minutes">minutes</option>
					<option value="hours">hours</option>
					<option value="days">days</option>
				</select>
			</div>
		</div>
	{:else if form.kind === "github"}
		<div class="field-row">
			<label class="field-label" for="gh-repo">Repo</label>
			<input
				id="gh-repo"
				class="text-input"
				bind:value={form.githubRepo}
				placeholder="owner/repo"
				aria-label="GitHub repo"
			/>
		</div>
		<div class="field-row">
			<span class="field-label">Events</span>
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
			<label class="field-label" for="file-glob">Glob</label>
			<input
				id="file-glob"
				class="text-input"
				bind:value={form.fileGlob}
				placeholder="**/*.ts"
				aria-label="File glob"
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="file-path">Path</label>
			<input
				id="file-path"
				class="text-input"
				bind:value={form.filePath}
				placeholder="/path/to/dir"
				aria-label="Watch path"
			/>
		</div>
	{:else if form.kind === "crewmate"}
		<div class="field-row">
			<label class="field-label" for="crewmate-from">Crewmate name</label>
			<input
				id="crewmate-from"
				class="text-input"
				bind:value={form.crewmateChainFrom}
				placeholder="Crewmate name or ID"
				aria-label="Source crewmate"
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="crewmate-on">When</label>
			<select
				id="crewmate-on"
				class="unit-select"
				bind:value={form.crewmateChainOn}
				aria-label="When to trigger"
			>
				<option value="done">Finishes</option>
				<option value="failed">Fails</option>
			</select>
		</div>
	{:else if form.kind === "service"}
		<div class="field-row">
			<label class="field-label" for="svc-name">Service</label>
			<input
				id="svc-name"
				class="text-input"
				bind:value={form.serviceName}
				placeholder="Service name"
				aria-label="Service name"
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="svc-dir">Project dir</label>
			<input
				id="svc-dir"
				class="text-input"
				bind:value={form.serviceProjectDir}
				placeholder="/path/to/project"
				aria-label="Project directory"
			/>
		</div>
		<div class="field-row">
			<label class="field-label" for="svc-on">When</label>
			<select id="svc-on" class="unit-select" bind:value={form.serviceOn} aria-label="When to trigger">
				<option value="failed">Fails</option>
				<option value="recovered">Recovers</option>
			</select>
		</div>
	{:else if form.kind === "webhook"}
		<p class="hint">
			A unique token will be generated. Use it in your webhook URL:
			<code>/crew/&#123;crewmate-id&#125;/hook/&#123;token&#125;</code>
		</p>
		{#if trigger?.kind.type === "webhook"}
			<div class="field-row">
				<span class="field-label">Token</span>
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
