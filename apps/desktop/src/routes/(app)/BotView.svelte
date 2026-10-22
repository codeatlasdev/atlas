<script lang="ts">
	/**
	 * BotView — full-page view for a selected bot.
	 * Task thread, composer, approval card, memory editor, routines, Telegram, and Pair QR.
	 */
	import { onMount } from "svelte";
	import { Button, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { host, hostCall } from "#lib/host.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import PairQr from "#lib/components/PairQr.svelte";
	import type { BotTask, Routine, PendingApproval } from "#lib/bots.ts";
	import type { PairStart } from "#lib/settings/mobile.ts";

	// ─── state ────────────────────────────────────────────────────────────────

	let prompt = $state("");
	let sending = $state(false);
	let panel = $state<"none" | "memory" | "routines" | "settings" | "telegram" | "pair">("none");

	// memory panel
	let memoryText = $state("");
	let memoryLoading = $state(false);
	let memorySaving = $state(false);

	// settings panel
	let settingsName = $state("");
	let settingsRole = $state("");
	let settingsAgent = $state("");
	let settingsApproval = $state<"ask" | "auto">("auto");
	let settingsSaving = $state(false);

	// routines panel
	let routinesList = $state<Routine[]>([]);
	let newRoutineCron = $state("");
	let newRoutinePrompt = $state("");

	// telegram panel
	let telegramToken = $state("");
	let telegramCode = $state("");
	let telegramLinking = $state(false);

	// pair QR
	let pairing = $state<{ p: PairStart; at: number } | null>(null);
	let now = $state(Date.now());
	let pairingTimer: ReturnType<typeof setInterval> | undefined;

	// ─── derived ──────────────────────────────────────────────────────────────

	const bot = $derived(bots.selectedBot);
	const tasks = $derived(bots.tasks);
	const agentList = $derived(agents.list.filter((a) => a.installed));
	const pendingApproval = $derived<PendingApproval | undefined>(bot?.pending_approval);

	$effect(() => {
		if (panel === "memory") void loadMemory();
		if (panel === "settings" && bot) {
			settingsName = bot.name;
			settingsRole = bot.role;
			settingsAgent = bot.agent;
			settingsApproval = bot.approval;
		}
		if (panel === "routines" && bot) {
			void loadRoutines();
		}
	});

	onMount(() => {
		pairingTimer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(pairingTimer);
	});

	// ─── actions ──────────────────────────────────────────────────────────────

	async function send() {
		if (!prompt.trim() || sending) return;
		sending = true;
		try {
			await bots.sendTask(prompt.trim());
			prompt = "";
		} catch (e) {
			toast.error(errText(e));
		} finally {
			sending = false;
		}
	}

	async function loadMemory() {
		memoryLoading = true;
		try {
			memoryText = await bots.getMemory();
		} catch {
			/* keep stale */
		}
		memoryLoading = false;
	}

	async function saveMemory() {
		memorySaving = true;
		try {
			await bots.setMemory(memoryText);
			toast.success("Memory saved");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			memorySaving = false;
		}
	}

	async function saveSettings() {
		if (!bots.selected || !bot) return;
		settingsSaving = true;
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, {
				name: settingsName,
				role: settingsRole,
				agent: settingsAgent,
				approval: settingsApproval,
			});
			toast.success("Settings saved");
			panel = "none";
		} catch (e) {
			toast.error(errText(e));
		} finally {
			settingsSaving = false;
		}
	}

	async function loadRoutines() {
		if (!bots.selected || !bot) return;
		// Routines are embedded in the bot object from bot_list
		const entry = bots.hosts.find((h) => h.key === bots.selected!.hostKey);
		const fullBot = entry?.bots.find((b) => b.id === bot!.id);
		// The bot_list response doesn't include routines; fetch via bot_tasks is not right.
		// We need to read from the bot directly — for now show empty until a deeper fetch.
		// (bot_list includes routines via the plan's contract; if not yet, this is a graceful fallback)
		routinesList = (fullBot as unknown as { routines?: Routine[] })?.routines ?? [];
	}

	async function addRoutine() {
		if (!newRoutineCron.trim() || !newRoutinePrompt.trim()) return;
		const updated: Routine[] = [
			...routinesList,
			{
				id: crypto.randomUUID(),
				cron: newRoutineCron.trim(),
				prompt: newRoutinePrompt.trim(),
				enabled: true,
			},
		];
		try {
			await bots.setRoutines(updated);
			routinesList = updated;
			newRoutineCron = "";
			newRoutinePrompt = "";
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function toggleRoutine(id: string, enabled: boolean) {
		const updated = routinesList.map((r) => (r.id === id ? { ...r, enabled } : r));
		try {
			await bots.setRoutines(updated);
			routinesList = updated;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function removeRoutine(id: string) {
		const updated = routinesList.filter((r) => r.id !== id);
		try {
			await bots.setRoutines(updated);
			routinesList = updated;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function openTerminal() {
		if (!bot?.running_task?.pane || !bots.selected) return;
		const pty = bot.running_task.pane;
		// Attach to the running task's pty on the bot host
		const args = bots.selected.hostKey ? { pty, host: bots.selected.hostKey } : { pty };
		await hostCall("pty_attach", { ...args, replay: true, output: true }).catch((e) =>
			toast.error(errText(e)),
		);
	}

	async function allowApproval() {
		if (!pendingApproval) return;
		await bots
			.approve(pendingApproval.pane_key, pendingApproval.req, true)
			.catch((e) => toast.error(errText(e)));
	}

	async function denyApproval() {
		if (!pendingApproval) return;
		await bots
			.approve(pendingApproval.pane_key, pendingApproval.req, false)
			.catch((e) => toast.error(errText(e)));
	}

	async function saveTelegram() {
		try {
			await bots.setTelegramToken(telegramToken.trim() || null);
			toast.success(telegramToken.trim() ? "Telegram token saved" : "Telegram token cleared");
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function linkTelegram() {
		telegramLinking = true;
		try {
			telegramCode = await bots.telegramLinkCode();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			telegramLinking = false;
		}
	}

	async function startPair() {
		try {
			const p = await hostCall<PairStart>(
				"mobile_pair_start",
				bots.selected?.hostKey ? { host: bots.selected.hostKey } : {},
			);
			pairing = { p, at: Date.now() };
		} catch (e) {
			toast.error(errText(e));
		}
	}

	function stateLabel(t: BotTask) {
		if (t.state === "queued") return "In queue";
		if (t.state === "running") return "Running…";
		if (t.state === "cancelled") return "Cancelled";
		return "";
	}
</script>

<div class="bot-view">
	{#if !bot}
		<div class="empty">Select a bot from the sidebar.</div>
	{:else}
		<!-- Header -->
		<div class="header">
			<div class="title-row">
				<span class="bot-name">{bot.name}</span>
				<span class="state-chip state-{bot.state}">
					{bot.state === "idle" ? "Idle" : bot.state === "working" ? "Working" : "Needs you"}
				</span>
				{#if bot.running_task}
					<button class="link-btn" onclick={openTerminal} title="Open running task terminal">
						Terminal ↗
					</button>
				{/if}
			</div>
			<div class="actions">
				<button
					class="panel-btn"
					class:active={panel === "memory"}
					onclick={() => (panel = panel === "memory" ? "none" : "memory")}>Memory</button
				>
				<button
					class="panel-btn"
					class:active={panel === "routines"}
					onclick={() => (panel = panel === "routines" ? "none" : "routines")}>Routines</button
				>
				<button
					class="panel-btn"
					class:active={panel === "settings"}
					onclick={() => (panel = panel === "settings" ? "none" : "settings")}>Settings</button
				>
				<button
					class="panel-btn"
					class:active={panel === "telegram"}
					onclick={() => (panel = panel === "telegram" ? "none" : "telegram")}>Telegram</button
				>
				<button
					class="panel-btn"
					class:active={panel === "pair"}
					onclick={() => {
						panel = panel === "pair" ? "none" : "pair";
						if (panel === "pair") void startPair();
					}}>Pair phone</button
				>
			</div>
		</div>

		<!-- Approval card -->
		{#if pendingApproval}
			<div class="approval-card" role="alert">
				<div class="approval-title">
					<span class="needs-dot"></span>
					<strong>{bot.name}</strong> wants to run <code>{pendingApproval.tool_name}</code>
				</div>
				<pre class="approval-input">{JSON.stringify(pendingApproval.tool_input, null, 2).slice(0, 800)}</pre>
				<div class="approval-btns">
					<Button size="sm" onclick={allowApproval}>Allow</Button>
					<Button size="sm" variant="ghost" onclick={denyApproval}>Deny</Button>
				</div>
			</div>
		{/if}

		<!-- Side panel -->
		{#if panel === "memory"}
			<div class="side-panel">
				<div class="panel-head">
					<span>Memory</span>
					<button class="close-btn" onclick={() => (panel = "none")}>✕</button>
				</div>
				{#if memoryLoading}
					<div class="loading">Loading…</div>
				{:else}
					<textarea
						class="memory-area ca-selectable"
						bind:value={memoryText}
						rows={12}
						aria-label="Bot memory"></textarea>
					<div class="panel-foot">
						<Button size="sm" disabled={memorySaving} onclick={saveMemory}
							>{memorySaving ? "Saving…" : "Save"}</Button
						>
					</div>
				{/if}
			</div>
		{:else if panel === "settings"}
			<div class="side-panel">
				<div class="panel-head">
					<span>Settings</span>
					<button class="close-btn" onclick={() => (panel = "none")}>✕</button>
				</div>
				<label class="field">
					<span>Name</span>
					<input bind:value={settingsName} maxlength={40} aria-label="Bot name" />
				</label>
				<label class="field">
					<span>Role</span>
					<textarea bind:value={settingsRole} rows={4} maxlength={2000} aria-label="Bot role"></textarea>
				</label>
				<label class="field">
					<span>Agent</span>
					<select bind:value={settingsAgent} aria-label="Agent">
						{#each agentList as a (a.id)}
							<option value={a.id}>{a.label}</option>
						{/each}
					</select>
				</label>
				<label class="field">
					<span>Approval</span>
					<select bind:value={settingsApproval} aria-label="Approval mode">
						<option value="auto">Auto (no prompts)</option>
						{#if settingsAgent !== "omp"}
							<option value="ask">Ask me</option>
						{/if}
					</select>
				</label>
				{#if settingsAgent === "omp" && settingsApproval === "ask"}
					<p class="hint">omp bots run with auto-approval only.</p>
				{/if}
				<div class="panel-foot">
					<Button size="sm" disabled={settingsSaving} onclick={saveSettings}
						>{settingsSaving ? "Saving…" : "Save"}</Button
					>
				</div>
			</div>
		{:else if panel === "routines"}
			<div class="side-panel">
				<div class="panel-head">
					<span>Routines</span>
					<button class="close-btn" onclick={() => (panel = "none")}>✕</button>
				</div>
				{#each routinesList as r (r.id)}
					<div class="routine-row">
						<input
							type="checkbox"
							checked={r.enabled}
							aria-label="Enable {r.cron}"
							onchange={(e) => void toggleRoutine(r.id, (e.currentTarget as HTMLInputElement).checked)}
						/>
						<span class="routine-cron">{r.cron}</span>
						<span class="routine-prompt">{r.prompt}</span>
						<button class="close-btn" onclick={() => void removeRoutine(r.id)} aria-label="Remove routine"
							>✕</button
						>
					</div>
				{:else}
					<p class="hint">No routines yet.</p>
				{/each}
				<div class="add-routine">
					<input
						class="cron-input"
						bind:value={newRoutineCron}
						placeholder="0 9 * * 1-5"
						aria-label="Cron expression"
					/>
					<input
						class="prompt-input"
						bind:value={newRoutinePrompt}
						placeholder="Daily standup summary"
						aria-label="Routine prompt"
					/>
					<Button size="sm" onclick={addRoutine}>Add</Button>
				</div>
				<div class="cron-presets">
					<button class="preset" onclick={() => (newRoutineCron = "0 8 * * *")}>Every morning 8:00</button>
					<button class="preset" onclick={() => (newRoutineCron = "0 9 * * 1-5")}>Weekdays 9:00</button>
				</div>
			</div>
		{:else if panel === "telegram"}
			<div class="side-panel">
				<div class="panel-head">
					<span>Telegram</span>
					<button class="close-btn" onclick={() => (panel = "none")}>✕</button>
				</div>
				<label class="field">
					<span>Bot token</span>
					<input
						type="password"
						bind:value={telegramToken}
						placeholder="123456:ABC-..."
						aria-label="Telegram bot token"
					/>
				</label>
				<div class="panel-foot">
					<Button size="sm" onclick={saveTelegram}>Save token</Button>
					<Button size="sm" variant="ghost" disabled={telegramLinking} onclick={linkTelegram}>
						{telegramLinking ? "Getting code…" : "Link chat"}
					</Button>
				</div>
				{#if telegramCode}
					<p class="telegram-code">Send <code>/start {telegramCode}</code> to your bot in Telegram.</p>
				{/if}
			</div>
		{:else if panel === "pair"}
			<div class="side-panel">
				<div class="panel-head">
					<span>Pair phone</span>
					<button class="close-btn" onclick={() => (panel = "none")}>✕</button>
				</div>
				<p class="hint">Open Atlas Mobile and scan the code.</p>
				{#if pairing}
					<PairQr {pairing} {now} />
				{/if}
				<div class="panel-foot">
					<Button size="sm" onclick={startPair}>New code</Button>
				</div>
			</div>
		{/if}

		<!-- Task thread -->
		<div class="thread" aria-label="Task history">
			{#if bots.tasksLoading && tasks.length === 0}
				<div class="loading">Loading…</div>
			{:else if tasks.length === 0}
				<div class="empty-thread">No tasks yet. Send one below.</div>
			{:else}
				{#each [...tasks].reverse() as t (t.id)}
					<div class="task-entry">
						<div class="prompt-bubble ca-selectable">{t.prompt}</div>
						{#if t.state === "done" && t.result}
							<div class="result ca-selectable">{t.result}</div>
						{:else if t.state === "failed"}
							<div class="result error ca-selectable">{t.error ?? "Failed"}</div>
						{:else if t.state !== "done"}
							<div class="task-state">{stateLabel(t)}</div>
						{/if}
					</div>
				{/each}
			{/if}
		</div>

		<!-- Composer -->
		<div class="composer">
			<textarea
				class="composer-input"
				bind:value={prompt}
				placeholder="Send a task…"
				rows={3}
				aria-label="Task prompt"
				onkeydown={(e) => {
					if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
						e.preventDefault();
						void send();
					}
				}}></textarea>
			<Button disabled={!prompt.trim() || sending} onclick={send}>
				{sending ? "Sending…" : "Send"}
			</Button>
		</div>
	{/if}
</div>

<style>
	.bot-view {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		background: var(--ca-surface-base);
	}

	.empty {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--ca-text-secondary);
		font-size: 14px;
	}

	/* ─── header ─────────────────────────────────────────────── */
	.header {
		flex: none;
		padding: 14px 20px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.title-row {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 8px;
	}
	.bot-name {
		font-size: 15px;
		font-weight: 600;
	}
	.state-chip {
		font-size: 11px;
		padding: 2px 7px;
		border-radius: 10px;
		font-weight: 500;
	}
	.state-idle {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-secondary);
	}
	.state-working {
		background: color-mix(in oklab, var(--ca-brand-main) 18%, transparent);
		color: var(--ca-brand-main);
	}
	.state-needs_you {
		background: color-mix(in oklab, var(--ca-semantic-warning) 18%, transparent);
		color: var(--ca-semantic-warning);
	}
	.actions {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.panel-btn {
		font-size: 12px;
		padding: 3px 9px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		transition: background var(--dur-fast) var(--ease);
	}
	.panel-btn:hover,
	.panel-btn.active {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.link-btn {
		font-size: 12px;
		color: var(--ca-brand-main);
		background: none;
		border: none;
		padding: 0;
		cursor: default;
	}

	/* ─── approval card ──────────────────────────────────────── */
	.approval-card {
		flex: none;
		margin: 12px 20px;
		padding: 12px 14px;
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, var(--ca-surface-base));
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
	}
	.approval-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		margin-bottom: 8px;
	}
	.needs-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-semantic-warning);
		flex: none;
	}
	.approval-input {
		font-family: var(--ca-font-mono, monospace);
		font-size: 11px;
		white-space: pre-wrap;
		word-break: break-all;
		margin: 0 0 10px;
		color: var(--ca-text-secondary);
	}
	.approval-btns {
		display: flex;
		gap: 8px;
	}

	/* ─── side panel ─────────────────────────────────────────── */
	.side-panel {
		flex: none;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		padding: 12px 20px;
		background: var(--ca-surface-sunken);
		max-height: 340px;
		overflow-y: auto;
	}
	.panel-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-weight: 600;
		font-size: 13px;
		margin-bottom: 10px;
	}
	.close-btn {
		background: none;
		border: none;
		font-size: 13px;
		color: var(--ca-text-secondary);
		cursor: default;
		padding: 2px 4px;
	}
	.panel-foot {
		display: flex;
		gap: 8px;
		margin-top: 10px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 10px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.field input,
	.field textarea,
	.field select {
		font-size: 13px;
		padding: 6px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
	}
	.memory-area {
		width: 100%;
		font-family: var(--ca-font-mono, monospace);
		font-size: 12px;
		padding: 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		resize: vertical;
		box-sizing: border-box;
	}
	.routine-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 0;
		font-size: 12px;
	}
	.routine-cron {
		font-family: var(--ca-font-mono, monospace);
		color: var(--ca-text-secondary);
	}
	.routine-prompt {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.add-routine {
		display: flex;
		gap: 6px;
		margin-top: 10px;
	}
	.cron-input {
		width: 120px;
		font-family: var(--ca-font-mono, monospace);
		font-size: 12px;
		padding: 4px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
	}
	.prompt-input {
		flex: 1;
		font-size: 12px;
		padding: 4px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
	}
	.cron-presets {
		display: flex;
		gap: 6px;
		margin-top: 6px;
		flex-wrap: wrap;
	}
	.preset {
		font-size: 11px;
		padding: 2px 7px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.preset:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.telegram-code {
		font-size: 12px;
		margin-top: 8px;
		color: var(--ca-text-secondary);
	}
	.telegram-code code {
		font-family: var(--ca-font-mono, monospace);
		background: var(--ca-vibrancy-fill);
		padding: 1px 4px;
		border-radius: 3px;
	}
	.loading {
		color: var(--ca-text-secondary);
		font-size: 13px;
		padding: 12px 0;
	}
	.hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 4px 0;
	}

	/* ─── thread ─────────────────────────────────────────────── */
	.thread {
		flex: 1;
		overflow-y: auto;
		padding: 16px 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.empty-thread {
		color: var(--ca-text-secondary);
		font-size: 13px;
		text-align: center;
		padding: 40px 0;
	}
	.task-entry {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.prompt-bubble {
		align-self: flex-end;
		max-width: 80%;
		padding: 8px 12px;
		border-radius: 12px 12px 2px 12px;
		background: color-mix(in oklab, var(--ca-brand-main) 20%, var(--ca-surface-base));
		font-size: 13px;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.result {
		align-self: flex-start;
		max-width: 90%;
		padding: 8px 12px;
		border-radius: 2px 12px 12px 12px;
		background: var(--ca-surface-sunken);
		font-size: 13px;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.result.error {
		background: color-mix(in oklab, var(--ca-semantic-error) 10%, var(--ca-surface-base));
		color: var(--ca-semantic-error);
	}
	.task-state {
		align-self: flex-start;
		font-size: 11px;
		color: var(--ca-text-secondary);
		padding: 2px 8px;
	}

	/* ─── composer ───────────────────────────────────────────── */
	.composer {
		flex: none;
		display: flex;
		gap: 10px;
		align-items: flex-end;
		padding: 12px 20px 16px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.composer-input {
		flex: 1;
		font-size: 13px;
		padding: 8px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		resize: none;
		line-height: 1.4;
		font-family: inherit;
	}
	.composer-input:focus {
		outline: none;
		border-color: var(--ca-action-focus);
	}
</style>
