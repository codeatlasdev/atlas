<script lang="ts">
	/**
	 * AutomationDialog — create or edit a bot automation (trigger + prompt).
	 * Follows the BotEditor.svelte pattern: kit Dialog + Button, CSS vars.
	 */
	import { Dialog, Button } from "@codeatlasdev/ui";
	import { Clock, Timer, Link, GitPullRequest, FileText, Bot as BotIcon, Settings2 } from "@lucide/svelte";
	import type { Component } from "svelte";
	import type { Trigger, TriggerKind } from "#lib/bots.ts";
	import { scheduleFromCron, cronFromSchedule, describeTrigger } from "#lib/bots/automations.ts";

	interface Props {
		open?: boolean;
		/** Populated in edit mode; null/undefined in create mode. */
		trigger?: Trigger | null;
		botName: string;
		/** IANA tz, e.g. "America/Sao_Paulo". Stored on the trigger and shown in the live sentence. */
		tz?: string;
		onsave?: (trigger: Trigger) => void;
		ondelete?: (id: string) => void;
	}

	let { open = $bindable(false), trigger = null, botName, tz, onsave, ondelete }: Props = $props();

	const isEdit = $derived(trigger != null);

	// ─── form state ──────────────────────────────────────────────────────────

	let selectedKind = $state<TriggerKind>("schedule");
	let prompt = $state("");

	// schedule builder
	let scheduleDays = $state<number[]>([1, 2, 3, 4, 5]);
	let scheduleHour = $state(9);
	let scheduleMinute = $state(0);
	let customCronMode = $state(false);
	let customCron = $state("");

	// interval
	let intervalValue = $state(30);
	let intervalUnit = $state<"minutes" | "hours" | "days">("minutes");

	// github
	let githubRepo = $state("");
	let githubEvents = $state<Set<string>>(new Set());

	// file
	let filePath = $state("");
	let fileGlob = $state("");

	// bot chain
	let botChainFrom = $state("");
	let botChainOn = $state<"done" | "failed">("done");

	// service
	let serviceProjectDir = $state("");
	let serviceName = $state("");
	let serviceOn = $state<"failed" | "recovered">("failed");

	// webhook
	let webhookToken = $state("");

	let saving = $state(false);

	// ─── reset on open ───────────────────────────────────────────────────────

	let prevOpen = false;
	$effect(() => {
		if (!open || prevOpen) {
			prevOpen = open;
			return;
		}
		prevOpen = open;

		if (trigger) {
			selectedKind = trigger.kind.type as TriggerKind;
			prompt = trigger.prompt;
			const k = trigger.kind;
			if (k.type === "schedule") {
				const spec = scheduleFromCron(k.cron);
				if (spec) {
					scheduleDays = spec.days;
					scheduleHour = spec.hour;
					scheduleMinute = spec.minute;
					customCronMode = false;
					customCron = k.cron;
				} else {
					customCronMode = true;
					customCron = k.cron;
					scheduleDays = [1, 2, 3, 4, 5];
					scheduleHour = 9;
					scheduleMinute = 0;
				}
			} else if (k.type === "interval") {
				const s = k.every_secs;
				if (s % 86400 === 0) {
					intervalValue = s / 86400;
					intervalUnit = "days";
				} else if (s % 3600 === 0) {
					intervalValue = s / 3600;
					intervalUnit = "hours";
				} else {
					intervalValue = Math.round(s / 60);
					intervalUnit = "minutes";
				}
			} else if (k.type === "github") {
				githubRepo = k.repo;
				githubEvents = new Set(k.events);
			} else if (k.type === "file") {
				filePath = k.path;
				fileGlob = k.glob ?? "";
			} else if (k.type === "bot") {
				botChainFrom = k.from_bot;
				botChainOn = k.on as "done" | "failed";
			} else if (k.type === "service") {
				serviceProjectDir = k.project_dir;
				serviceName = k.service;
				serviceOn = k.on as "failed" | "recovered";
			} else if (k.type === "webhook") {
				webhookToken = k.token;
			}
		} else {
			selectedKind = "schedule";
			prompt = "";
			scheduleDays = [1, 2, 3, 4, 5];
			scheduleHour = 9;
			scheduleMinute = 0;
			customCronMode = false;
			customCron = "";
			intervalValue = 30;
			intervalUnit = "minutes";
			githubRepo = "";
			githubEvents = new Set();
			filePath = "";
			fileGlob = "";
			botChainFrom = "";
			botChainOn = "done";
			serviceProjectDir = "";
			serviceName = "";
			serviceOn = "failed";
			webhookToken = "";
		}
	});

	// ─── derived ─────────────────────────────────────────────────────────────

	const scheduleCron = $derived(
		customCronMode
			? customCron
			: cronFromSchedule({ days: scheduleDays, hour: scheduleHour, minute: scheduleMinute }),
	);

	const liveSentence = $derived(
		selectedKind === "schedule" && scheduleCron
			? describeTrigger({ type: "schedule", cron: scheduleCron }, tz)
			: "",
	);

	const canSave = $derived(
		!saving &&
			!!prompt.trim() &&
			(selectedKind === "schedule"
				? !!scheduleCron
				: selectedKind === "interval"
					? intervalValue >= 1
					: selectedKind === "github"
						? !!githubRepo.trim()
						: selectedKind === "file"
							? !!(filePath.trim() || fileGlob.trim())
							: selectedKind === "bot"
								? !!botChainFrom.trim()
								: selectedKind === "service"
									? !!serviceName.trim()
									: true) /* webhook */,
	);

	// ─── actions ─────────────────────────────────────────────────────────────

	function toggleDay(d: number) {
		scheduleDays = scheduleDays.includes(d)
			? scheduleDays.length > 1
				? scheduleDays.filter((x) => x !== d)
				: scheduleDays
			: [...scheduleDays, d];
	}

	async function handleSave() {
		if (!canSave) return;
		let kind: Trigger["kind"];
		if (selectedKind === "schedule") {
			kind = { type: "schedule", cron: scheduleCron };
		} else if (selectedKind === "interval") {
			const mult = intervalUnit === "minutes" ? 60 : intervalUnit === "hours" ? 3600 : 86400;
			kind = { type: "interval", every_secs: intervalValue * mult };
		} else if (selectedKind === "github") {
			kind = { type: "github", repo: githubRepo.trim(), events: [...githubEvents] };
		} else if (selectedKind === "file") {
			kind = { type: "file", path: filePath.trim(), glob: fileGlob.trim() || undefined };
		} else if (selectedKind === "bot") {
			kind = { type: "bot", from_bot: botChainFrom.trim(), on: botChainOn };
		} else if (selectedKind === "service") {
			kind = {
				type: "service",
				project_dir: serviceProjectDir.trim(),
				service: serviceName.trim(),
				on: serviceOn,
			};
		} else {
			kind = { type: "webhook", token: webhookToken || crypto.randomUUID().replace(/-/g, "").slice(0, 24) };
		}

		const trig: Trigger = {
			id: trigger?.id ?? crypto.randomUUID(),
			kind,
			prompt: prompt.trim(),
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

	const KIND_ICONS: Record<TriggerKind, Component> = {
		schedule: Clock,
		interval: Timer,
		webhook: Link,
		github: GitPullRequest,
		file: FileText,
		bot: BotIcon,
		service: Settings2,
	};

	const KINDS: Array<{ kind: TriggerKind; label: string }> = [
		{ kind: "schedule", label: "Schedule" },
		{ kind: "interval", label: "Interval" },
		{ kind: "github", label: "GitHub" },
		{ kind: "file", label: "File" },
		{ kind: "bot", label: "After bot" },
		{ kind: "service", label: "Service" },
		{ kind: "webhook", label: "Webhook" },
	];

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

<Dialog bind:open title={isEdit ? "Edit automation" : "Add automation"} size="lg">
	<div class="body">
		<!-- Kind tabs -->
		<div class="kind-tabs" role="tablist" aria-label="Trigger kind">
			{#each KINDS as k (k.kind)}
				{@const Icon = KIND_ICONS[k.kind]}
				<button
					role="tab"
					aria-selected={selectedKind === k.kind}
					class:active={selectedKind === k.kind}
					onclick={() => (selectedKind = k.kind)}
				>
					<Icon size={13} aria-hidden="true" />
					{k.label}
				</button>
			{/each}
		</div>

		<!-- Kind-specific fields -->
		<div class="fields">
			{#if selectedKind === "schedule"}
				{#if !customCronMode}
					<!-- Day chips -->
					<div class="field-row">
						<span class="field-label">Days</span>
						<div class="day-chips" role="group" aria-label="Days of week">
							{#each DAY_LABELS as label, d}
								<button
									class="day-chip"
									class:on={scheduleDays.includes(d)}
									aria-pressed={scheduleDays.includes(d)}
									aria-label={DAY_FULL[d]}
									onclick={() => toggleDay(d)}
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
								class:active={scheduleDays.length === 7}
								onclick={() => (scheduleDays = [0, 1, 2, 3, 4, 5, 6])}
							>
								Every day
							</button>
							<button
								class="preset-btn"
								class:active={[...scheduleDays].sort().join() === "1,2,3,4,5"}
								onclick={() => (scheduleDays = [1, 2, 3, 4, 5])}
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
							value={`${String(scheduleHour).padStart(2, "0")}:${String(scheduleMinute).padStart(2, "0")}`}
							onchange={(e) => {
								const [h, m] = (e.currentTarget as HTMLInputElement).value.split(":").map(Number);
								if (!isNaN(h) && !isNaN(m)) {
									scheduleHour = h;
									scheduleMinute = m;
								}
							}}
						/>
					</div>

					<!-- Live sentence -->
					{#if liveSentence}
						<p class="live-sentence">{liveSentence}</p>
					{/if}
				{:else}
					<!-- Custom cron input -->
					<div class="field-row">
						<label class="field-label" for="custom-cron">Cron</label>
						<input
							id="custom-cron"
							class="cron-input"
							bind:value={customCron}
							placeholder="0 9 * * 1-5"
							aria-label="Cron expression"
						/>
					</div>
					{#if liveSentence}
						<p class="live-sentence">{liveSentence}</p>
					{/if}
				{/if}

				<button class="toggle-cron" onclick={() => (customCronMode = !customCronMode)}>
					{customCronMode ? "← Use builder" : "Custom cron →"}
				</button>
			{:else if selectedKind === "interval"}
				<div class="field-row">
					<label class="field-label" for="interval-val">Every</label>
					<div class="inline-row">
						<input
							id="interval-val"
							class="num-input"
							type="number"
							min="1"
							bind:value={intervalValue}
							aria-label="Interval amount"
						/>
						<select class="unit-select" bind:value={intervalUnit} aria-label="Interval unit">
							<option value="minutes">minutes</option>
							<option value="hours">hours</option>
							<option value="days">days</option>
						</select>
					</div>
				</div>
			{:else if selectedKind === "github"}
				<div class="field-row">
					<label class="field-label" for="gh-repo">Repo</label>
					<input
						id="gh-repo"
						class="text-input"
						bind:value={githubRepo}
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
									checked={githubEvents.has(ev.ev)}
									onchange={(e) => {
										const s = new Set(githubEvents);
										if ((e.currentTarget as HTMLInputElement).checked) s.add(ev.ev);
										else s.delete(ev.ev);
										githubEvents = s;
									}}
									aria-label={ev.label}
								/>
								{ev.label}
							</label>
						{/each}
					</div>
				</div>
			{:else if selectedKind === "file"}
				<div class="field-row">
					<label class="field-label" for="file-glob">Glob</label>
					<input
						id="file-glob"
						class="text-input"
						bind:value={fileGlob}
						placeholder="**/*.ts"
						aria-label="File glob"
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="file-path">Path</label>
					<input
						id="file-path"
						class="text-input"
						bind:value={filePath}
						placeholder="/path/to/dir"
						aria-label="Watch path"
					/>
				</div>
			{:else if selectedKind === "bot"}
				<div class="field-row">
					<label class="field-label" for="bot-from">Bot name</label>
					<input
						id="bot-from"
						class="text-input"
						bind:value={botChainFrom}
						placeholder="Bot name or ID"
						aria-label="Source bot"
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="bot-on">When</label>
					<select id="bot-on" class="unit-select" bind:value={botChainOn} aria-label="When to trigger">
						<option value="done">Finishes</option>
						<option value="failed">Fails</option>
					</select>
				</div>
			{:else if selectedKind === "service"}
				<div class="field-row">
					<label class="field-label" for="svc-name">Service</label>
					<input
						id="svc-name"
						class="text-input"
						bind:value={serviceName}
						placeholder="Service name"
						aria-label="Service name"
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="svc-dir">Project dir</label>
					<input
						id="svc-dir"
						class="text-input"
						bind:value={serviceProjectDir}
						placeholder="/path/to/project"
						aria-label="Project directory"
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="svc-on">When</label>
					<select id="svc-on" class="unit-select" bind:value={serviceOn} aria-label="When to trigger">
						<option value="failed">Fails</option>
						<option value="recovered">Recovers</option>
					</select>
				</div>
			{:else if selectedKind === "webhook"}
				<p class="hint">
					A unique token will be generated. Use it in your webhook URL:
					<code>/api/bots/webhook/&#123;token&#125;</code>
				</p>
				{#if trigger?.kind.type === "webhook"}
					<div class="field-row">
						<span class="field-label">Token</span>
						<code class="token-display">{trigger.kind.token}</code>
					</div>
				{/if}
			{/if}
		</div>

		<!-- Prompt (all kinds) -->
		<div class="prompt-section">
			<label class="field-label" for="auto-prompt">What should {botName || "the bot"} do?</label>
			<textarea
				id="auto-prompt"
				class="prompt-area"
				bind:value={prompt}
				rows={3}
				placeholder={selectedKind === "schedule"
					? "Review the open PRs and post a summary"
					: selectedKind === "github"
						? "A PR was opened: {{event}} — review it"
						: "Describe what the bot should do when this fires"}
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
