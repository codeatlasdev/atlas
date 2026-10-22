<script lang="ts">
	/**
	 * BotView — D2+D3: chat layout with ⋯ menu and profile side panel.
	 * Approval is inline in the thread; Telegram and Pair phone moved to D5 BotsSettings.
	 */
	import { onMount } from "svelte";
	import { Button, Spinner, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import type { BotTask, Routine } from "#lib/bots.ts";

	// ─── state ────────────────────────────────────────────────────────────────

	let prompt = $state("");
	let sending = $state(false);
	let panel = $state<"none" | "profile">("none");
	let menuOpen = $state(false);
	let now = $state(Date.now());

	// profile panel
	let settingsRole = $state("");
	let settingsAgent = $state("");
	let settingsApproval = $state<"ask" | "auto">("auto");
	let roleSaving = $state(false);
	let approvalSaving = $state(false);

	// memory (lazy-loaded when profile opens)
	let memoryText = $state("");
	let memoryLoading = $state(false);
	let memorySaving = $state(false);

	// routines
	let routinesList = $state<Routine[]>([]);
	let newRoutineCron = $state("");
	let newRoutinePrompt = $state("");

	// thread
	let stepsOpen = $state<Record<string, boolean>>({});
	let approvedReqs = $state<Set<string>>(new Set());
	/** Actual hostname of the local hostd, fetched on first selection of a local bot. */
	let localHostname = $state("");

	// ─── derived ──────────────────────────────────────────────────────────────

	const bot = $derived(bots.selectedBot);
	const tasks = $derived(bots.tasks);
	const agentList = $derived(agents.list.filter((a) => a.installed));
	const hostOnline = $derived(bots.selected ? bots.hostOk(bots.selected.hostKey) : true);
	const serverName = $derived(bots.selected?.hostName ?? "");
	/** Show actual hostname for local; server name label for remote. */
	const displayServerName = $derived(bots.selected?.hostKey ? serverName : localHostname || serverName);
	const queuedCount = $derived(tasks.filter((t) => t.state === "queued").length);

	/** Flat list of thread items: timestamp separators interleaved with tasks. Keyed by `key`. */
	const threadItems = $derived.by(() => {
		type Sep = { type: "sep"; key: string; ts: number };
		type TaskItem = { type: "task"; key: string; t: BotTask };
		const items: Array<Sep | TaskItem> = [];
		const reversed = [...tasks].reverse();
		let lastSepTs = -Infinity;
		for (const t of reversed) {
			if (t.ts - lastSepTs > 5 * 60 * 1000) {
				items.push({ type: "sep", key: `sep:${t.ts}:${t.id}`, ts: t.ts });
				lastSepTs = t.ts;
			}
			items.push({ type: "task", key: t.id, t });
		}
		return items;
	});

	const AGENT_LABELS: Record<string, string> = {
		omp: "omp",
		claude: "Claude Code",
		codex: "Codex",
		opencode: "OpenCode",
	};

	const agentLabel = $derived(
		bot ? (agents.list.find((a) => a.id === bot.agent)?.label ?? AGENT_LABELS[bot.agent] ?? bot.agent) : "",
	);

	// Reset approvedReqs when the selected bot changes
	$effect(() => {
		const _ = bots.selected;
		approvedReqs = new Set();
	});

	// Fetch actual hostname for local host so header shows e.g. "Mac" not "This computer"
	$effect(() => {
		if (bots.selected && !bots.selected.hostKey) {
			hostCall<{ hostname: string }>("info")
				.then((r) => {
					localHostname = r.hostname;
				})
				.catch(() => {});
		}
	});
	// Populate profile panel fields when opened
	$effect(() => {
		if (panel === "profile" && bot) {
			settingsRole = bot.role;
			settingsApproval = bot.approval;
			settingsAgent = bot.agent;
			void loadMemory();
			void loadRoutines();
		}
	});

	onMount(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// ─── helpers ──────────────────────────────────────────────────────────────

	function fmtTime(tsMs: number): string {
		return new Date(tsMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
	}

	function fmtElapsed(ms: number): string {
		const s = Math.floor(ms / 1000);
		if (s < 60) return `${s}s`;
		return `${Math.floor(s / 60)}m`;
	}

	function noop() {}

	/**
	 * Format tool_input for approval bubble: Bash → show command; Edit/Write → file path;
	 * else compact key: value lines.
	 */
	function formatApprovalInput(toolName: string, toolInput: unknown): string {
		if (!toolInput || typeof toolInput !== "object") return String(toolInput ?? "");
		const inp = toolInput as Record<string, unknown>;
		if (toolName === "Bash" && typeof inp.command === "string") return inp.command;
		if ((toolName === "Write" || toolName === "Edit") && typeof inp.file_path === "string")
			return inp.file_path;
		return Object.entries(inp)
			.slice(0, 6)
			.map(([k, v]) => `${k}: ${typeof v === "string" ? v : JSON.stringify(v)}`)
			.join("\n");
	}

	const AVATAR_COLORS = [
		"#6366f1",
		"#8b5cf6",
		"#ec4899",
		"#f43f5e",
		"#f97316",
		"#eab308",
		"#22c55e",
		"#14b8a6",
		"#3b82f6",
	];

	function avatarColor(name: string): string {
		let h = 0;
		for (let i = 0; i < name.length; i++) h = ((h * 31 + name.charCodeAt(i)) >>> 0) % AVATAR_COLORS.length;
		return AVATAR_COLORS[h];
	}

	function sourceLabel(source: string): string {
		if (source === "mobile") return "· mobile";
		if (source === "desktop") return "· desktop";
		if (source === "routine") return "· routine";
		return "";
	}

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

	async function saveRole() {
		if (!bots.selected || !bot) return;
		roleSaving = true;
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { role: settingsRole });
			toast.success("Role saved");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			roleSaving = false;
		}
	}

	async function saveApproval() {
		if (!bots.selected || !bot) return;
		approvalSaving = true;
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { approval: settingsApproval });
			toast.success("Approval saved");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			approvalSaving = false;
		}
	}

	/** Kept for compatibility — saves role, agent, and approval together. */
	async function saveSettings() {
		if (!bots.selected || !bot) return;
		await bots.updateBot(bots.selected.hostKey, bot.id, {
			role: settingsRole,
			agent: settingsAgent,
			approval: settingsApproval,
		});
	}

	async function loadRoutines() {
		if (!bots.selected || !bot) return;
		const entry = bots.hosts.find((h) => h.key === bots.selected!.hostKey);
		const fullBot = entry?.bots.find((b) => b.id === bot!.id);
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
		const args = bots.selected.hostKey ? { pty, host: bots.selected.hostKey } : { pty };
		await hostCall("pty_attach", { ...args, replay: true, output: true }).catch((e) =>
			toast.error(errText(e)),
		);
	}

	async function cancelRunningTask() {
		if (!bot?.running_task) return;
		menuOpen = false;
		await bots.cancelTask(bot.running_task.id).catch((e) => toast.error(errText(e)));
	}

	async function allowApproval(paneKey: string, req: string) {
		await bots
			.approve(paneKey, req, true)
			.then(() => {
				approvedReqs = new Set([...approvedReqs, req]);
			})
			.catch((e) => toast.error(errText(e)));
	}

	async function denyApproval(paneKey: string, req: string) {
		await bots
			.approve(paneKey, req, false)
			.then(() => {
				approvedReqs = new Set([...approvedReqs, req]);
			})
			.catch((e) => toast.error(errText(e)));
	}

	async function deleteBot() {
		if (!bots.selected || !bot) return;
		if (!confirm(`Delete "${bot.name}"? This cannot be undone.`)) return;
		try {
			await bots.deleteBot(bots.selected.hostKey, bot.id);
			panel = "none";
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

<div class="bot-view">
	{#if !bot}
		<div class="empty">Select a bot from the sidebar.</div>
	{:else}
		<!-- Header -->
		<div class="header">
			<div class="header-left">
				<div class="avatar" style:background={avatarColor(bot.name)} style:color={"#fff"} aria-hidden="true">
					{bot.name[0].toUpperCase()}
				</div>
				<div class="header-title">
					<span class="bot-name">{bot.name}</span>
					{#if agentLabel}
						<span class="header-sep">·</span>
						<span class="header-meta">{agentLabel}</span>
					{/if}
					{#if displayServerName}
						<span class="header-sep">·</span>
						<span class="header-meta">{displayServerName}</span>
					{/if}
				</div>
			</div>
			<div class="header-right">
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="menu-wrap" onmouseleave={() => (menuOpen = false)}>
					<button
						class="icon-btn"
						aria-label="More options"
						aria-expanded={menuOpen}
						aria-haspopup="menu"
						onclick={() => (menuOpen = !menuOpen)}>⋯</button
					>
					{#if menuOpen}
						<div class="dropdown" role="menu">
							<button
								class="menu-item"
								role="menuitem"
								onclick={() => {
									panel = "profile";
									menuOpen = false;
								}}>Profile</button
							>
							{#if bot.running_task?.pane}
								<button
									class="menu-item"
									role="menuitem"
									onclick={() => {
										void openTerminal();
										menuOpen = false;
									}}>Terminal</button
								>
							{/if}
							{#if bot.running_task}
								<button
									class="menu-item menu-item--danger"
									role="menuitem"
									onclick={() => void cancelRunningTask()}>Cancel task</button
								>
							{/if}
						</div>
					{/if}
				</div>
			</div>
		</div>

		<!-- Offline banner -->
		{#if !hostOnline}
			<div class="offline-banner" role="alert">
				⚠ {displayServerName || "Server"} is offline
			</div>
		{/if}

		<!-- Task thread -->
		<div class="thread" aria-label="Task history">
			{#if bots.tasksLoading && tasks.length === 0}
				<div class="loading">Loading…</div>
			{:else if tasks.length === 0}
				<div class="empty-thread">No tasks yet. Send one below.</div>
			{:else}
				{#each threadItems as item (item.key)}
					{#if item.type === "sep"}
						<div class="msg-ts">{fmtTime(item.ts)}</div>
					{:else}
						{@const t = item.t}
						<!-- User bubble (right) -->
						<div class="msg-out">
							<div class="bubble out ca-selectable">{t.prompt}</div>
							{#if t.source !== "desktop"}
								<div class="msg-meta">· {t.source}</div>
							{/if}
							{#if t.state === "queued"}
								<div class="msg-meta">Queued</div>
							{/if}
						</div>

						<!-- Bot response bubble (left) with avatar -->
						{#if t.state === "running"}
							{@const elapsedMs = t.started_at ? now - t.started_at * 1000 : 0}
							<div class="msg-in">
								<div
									class="bot-av"
									style:background={avatarColor(bot.name)}
									style:color={"#fff"}
									aria-hidden="true"
								>
									{bot.name[0].toUpperCase()}
								</div>
								<div class="msg-in-body">
									<div class="bubble in working">
										<Spinner size={12} />
										Working · {fmtElapsed(elapsedMs)}
									</div>
									{#if t.steps && t.steps.length > 0}
										<div class="steps-wrap">
											<button
												class="steps-btn"
												aria-expanded={!!stepsOpen[t.id]}
												onclick={() => (stepsOpen = { ...stepsOpen, [t.id]: !stepsOpen[t.id] })}
											>
												{stepsOpen[t.id] ? "▾" : "▸"}
												{t.steps.length}
												{t.steps.length === 1 ? "step" : "steps"}
											</button>
											{#if stepsOpen[t.id]}
												<div class="steps-list">
													{#each t.steps as step, si (`${t.id}-${si}`)}
														<div class="step-row">
															<span class="step-ts">{fmtTime(step.ts)}</span>
															<span>{step.text}</span>
														</div>
													{/each}
												</div>
											{/if}
										</div>
									{/if}
									{#if bot.running_task?.id === t.id && bot.pending_approval}
										{@const pa = bot.pending_approval}
										{#if approvedReqs.has(pa.req)}
											<div class="approval-answered">✓ Allowed by you</div>
										{:else}
											<div class="approval-bubble" role="alert">
												<div class="approval-tool">Wants to run <strong>{pa.tool_name}</strong></div>
												<pre class="approval-input">{formatApprovalInput(
														pa.tool_name ?? "",
														pa.tool_input,
													)}</pre>
												<div class="approval-btns">
													<Button
														size="sm"
														variant="ghost"
														onclick={() => void denyApproval(pa.pane_key, pa.req)}>Deny</Button
													>
													<Button size="sm" onclick={() => void allowApproval(pa.pane_key, pa.req)}
														>Allow</Button
													>
												</div>
											</div>
										{/if}
									{/if}
								</div>
							</div>
						{:else if t.state === "done"}
							<div class="msg-in">
								<div
									class="bot-av"
									style:background={avatarColor(bot.name)}
									style:color={"#fff"}
									aria-hidden="true"
								>
									{bot.name[0].toUpperCase()}
								</div>
								<div class="msg-in-body">
									<div class="bubble in ca-selectable">{t.result ?? "Done"}</div>
									{#if t.steps && t.steps.length > 0}
										<div class="steps-wrap">
											<button
												class="steps-btn"
												aria-expanded={!!stepsOpen[t.id]}
												onclick={() => (stepsOpen = { ...stepsOpen, [t.id]: !stepsOpen[t.id] })}
											>
												{stepsOpen[t.id] ? "▾" : "▸"}
												{t.steps.length}
												{t.steps.length === 1 ? "step" : "steps"}
											</button>
											{#if stepsOpen[t.id]}
												<div class="steps-list">
													{#each t.steps as step, si (`${t.id}-${si}`)}
														<div class="step-row">
															<span class="step-ts">{fmtTime(step.ts)}</span>
															<span>{step.text}</span>
														</div>
													{/each}
												</div>
											{/if}
										</div>
									{/if}
								</div>
							</div>
						{:else if t.state === "failed"}
							<div class="msg-in">
								<div
									class="bot-av"
									style:background={avatarColor(bot.name)}
									style:color={"#fff"}
									aria-hidden="true"
								>
									{bot.name[0].toUpperCase()}
								</div>
								<div class="msg-in-body">
									<div class="bubble in error ca-selectable">{t.error ?? "Failed"}</div>
								</div>
							</div>
						{:else if t.state === "cancelled"}
							<div class="msg-in">
								<div
									class="bot-av"
									style:background={avatarColor(bot.name)}
									style:color={"#fff"}
									aria-hidden="true"
								>
									{bot.name[0].toUpperCase()}
								</div>
								<div class="msg-in-body">
									<div class="bubble in muted">Cancelled</div>
								</div>
							</div>
						{/if}
					{/if}
				{/each}
			{/if}
		</div>

		<!-- Composer -->
		<div class="composer">
			<div class="pill" role="group">
				<button class="pill-add" aria-label="Attach" onclick={noop}>+</button>
				<input
					class="pill-input"
					bind:value={prompt}
					placeholder="Message {bot.name ?? 'bot'}…"
					aria-label="Task prompt"
					onkeydown={(e) => {
						if (e.key === "Enter" && !e.shiftKey) {
							e.preventDefault();
							void send();
						}
					}}
				/>
				<button
					class="pill-send"
					class:active={!!prompt.trim()}
					aria-label="Send"
					disabled={!prompt.trim() || sending}
					onclick={send}>↑</button
				>
			</div>
			{#if queuedCount > 0}
				<div class="queue-hint">{queuedCount} task{queuedCount === 1 ? "" : "s"} in queue</div>
			{/if}
		</div>

		<!-- D3 Profile panel (right overlay) -->
		{#if panel === "profile"}
			<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
			<div class="panel-overlay" role="presentation" onclick={() => (panel = "none")}></div>
			<div class="profile-panel" role="complementary" aria-label="Bot profile">
				<button class="close-btn" onclick={() => (panel = "none")} aria-label="Close panel">✕</button>

				<div class="profile-header">
					<div
						class="profile-avatar"
						style:background={avatarColor(bot.name)}
						style:color={"#fff"}
						aria-hidden="true"
					>
						{bot.name[0].toUpperCase()}
					</div>
					<div class="profile-name">{bot.name}</div>
					<div class="profile-sub">
						{agentLabel}{serverName ? ` · ${serverName}` : ""}
					</div>
				</div>

				<!-- What it does -->
				<div class="profile-section">
					<div class="profile-section-title">What it does</div>
					<textarea
						class="profile-textarea ca-selectable"
						bind:value={settingsRole}
						rows={5}
						maxlength={2000}
						aria-label="Bot role"></textarea>
					<div class="profile-foot">
						<Button size="sm" disabled={roleSaving} onclick={saveRole}>
							{roleSaving ? "Saving…" : "Save"}
						</Button>
					</div>
				</div>

				<!-- Approval -->
				<div class="profile-section">
					<div class="profile-section-title">Approval</div>
					<select class="profile-select" bind:value={settingsApproval} aria-label="Approval mode">
						<option value="ask">Ask before acting</option>
						<option value="auto">Automatic (no prompts)</option>
					</select>
					{#if settingsAgent === "omp" && settingsApproval === "ask"}
						<p class="hint">omp bots run with auto-approval only.</p>
					{/if}
					<div class="profile-foot">
						<Button size="sm" disabled={approvalSaving} onclick={saveApproval}>
							{approvalSaving ? "Saving…" : "Save"}
						</Button>
					</div>
				</div>

				<!-- Routines -->
				<div class="profile-section">
					<div class="profile-section-title">Routines</div>
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
						<button class="preset" onclick={() => (newRoutineCron = "0 8 * * *")}>
							Every morning 8:00
						</button>
						<button class="preset" onclick={() => (newRoutineCron = "0 9 * * 1-5")}> Weekdays 9:00 </button>
					</div>
				</div>

				<!-- Memory -->
				<div class="profile-section">
					<div class="profile-section-title">Memory</div>
					{#if memoryLoading}
						<div class="loading">Loading…</div>
					{:else}
						<textarea
							class="memory-area ca-selectable"
							bind:value={memoryText}
							rows={8}
							aria-label="Bot memory"></textarea>
						<div class="profile-foot">
							<Button size="sm" disabled={memorySaving} onclick={saveMemory}>
								{memorySaving ? "Saving…" : "Save memory"}
							</Button>
						</div>
					{/if}
				</div>

				<!-- Delete bot -->
				<div class="profile-delete-wrap">
					<button class="delete-btn" onclick={() => void deleteBot()}>Delete bot</button>
				</div>
			</div>
		{/if}
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
		position: relative;
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
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		gap: 10px;
	}
	.header-left {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}
	.avatar {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 13px;
		font-weight: 700;
		flex: none;
		letter-spacing: 0;
	}
	.header-title {
		display: flex;
		align-items: baseline;
		gap: 6px;
		min-width: 0;
		flex-wrap: wrap;
	}
	.bot-name {
		font-size: 14px;
		font-weight: 600;
		white-space: nowrap;
	}
	.header-sep {
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
		font-size: 13px;
	}
	.header-meta {
		font-size: 12px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.header-right {
		flex: none;
	}

	/* ─── ⋯ menu ──────────────────────────────────────────────── */
	.menu-wrap {
		position: relative;
	}
	.icon-btn {
		background: none;
		border: none;
		font-size: 18px;
		line-height: 1;
		color: var(--ca-text-secondary);
		padding: 4px 6px;
		border-radius: 6px;
		cursor: default;
		letter-spacing: 2px;
	}
	.icon-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.dropdown {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		min-width: 140px;
		background: var(--ca-surface-overlay, var(--ca-surface-base));
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
		z-index: 100;
		overflow: hidden;
		padding: 4px 0;
	}
	.menu-item {
		display: block;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		padding: 7px 14px;
		font-size: 13px;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.menu-item:hover {
		background: var(--ca-vibrancy-fill);
	}
	.menu-item--danger {
		color: var(--ca-semantic-error);
	}

	/* ─── offline banner ─────────────────────────────────────── */
	.offline-banner {
		flex: none;
		padding: 6px 16px;
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 12%, var(--ca-surface-base));
		color: color-mix(in oklab, var(--ca-semantic-warning) 80%, var(--ca-text-primary));
		border-bottom: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 25%, transparent);
	}

	/* ─── thread ─────────────────────────────────────────────── */
	.thread {
		flex: 1;
		overflow-y: auto;
		padding: 20px 24px 8px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.empty-thread {
		color: var(--ca-text-secondary);
		font-size: 13px;
		text-align: center;
		padding: 40px 0;
	}
	.loading {
		color: var(--ca-text-secondary);
		font-size: 13px;
		padding: 12px 0;
	}

	/* ─── message bubbles ────────────────────────────────────── */
	.msg-ts {
		text-align: center;
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin: 12px 0 8px;
	}
	.msg-out {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		margin-bottom: 4px;
	}
	.msg-in {
		display: flex;
		flex-direction: row;
		align-items: flex-start;
		gap: 6px;
		margin-bottom: 8px;
	}
	.bot-av {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 11px;
		font-weight: 700;
		margin-top: 2px;
	}
	.msg-in-body {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		min-width: 0;
	}
	.bubble {
		max-width: 72%;
		padding: 10px 14px;
		border-radius: 18px;
		font-size: 13.5px;
		line-height: 1.45;
		word-break: break-word;
		white-space: pre-wrap;
	}
	.bubble.out {
		background: color-mix(in oklab, var(--ca-brand-main) 22%, var(--ca-surface-base));
		border-bottom-right-radius: 4px;
	}
	.bubble.in {
		background: var(--ca-surface-sunken);
		border-bottom-left-radius: 4px;
		color: var(--ca-text-primary);
	}
	.bubble.in.working {
		color: var(--ca-text-secondary);
		font-style: italic;
	}
	.bubble.in.error {
		color: var(--ca-semantic-error);
	}
	.bubble.in.muted {
		color: var(--ca-text-secondary);
	}
	.msg-meta {
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin-top: 2px;
		padding: 0 4px;
	}

	/* steps */
	.steps-wrap {
		margin: 2px 0 4px 2px;
	}
	.steps-btn {
		background: none;
		border: none;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		cursor: default;
		padding: 2px 4px;
	}
	.steps-btn:hover {
		color: var(--ca-text-primary);
	}
	.steps-list {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		padding: 4px 0 0 12px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.step-row {
		display: flex;
		gap: 8px;
	}
	.step-ts {
		opacity: 0.6;
		flex: none;
	}

	/* approval bubble */
	.approval-bubble {
		max-width: 72%;
		padding: 10px 14px;
		border-radius: 18px;
		border-bottom-left-radius: 4px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, var(--ca-surface-sunken));
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
		font-size: 13px;
	}
	.approval-tool {
		font-weight: 600;
		margin-bottom: 6px;
	}
	.approval-input {
		font-family: var(--ca-font-mono, monospace);
		font-size: 11px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
		border-radius: 6px;
		padding: 6px 8px;
		margin: 6px 0;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.approval-btns {
		display: flex;
		gap: 8px;
		margin-top: 8px;
	}
	.approval-answered {
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 4px;
		margin-top: 2px;
	}

	/* ─── composer ───────────────────────────────────────────── */
	.composer {
		flex: none;
		padding: 10px 16px 14px;
	}
	.pill {
		display: flex;
		align-items: center;
		background: var(--ca-surface-sunken);
		border-radius: 24px;
		border: 0.5px solid var(--ca-outline-subtle);
		overflow: hidden;
	}
	.pill-add {
		flex: none;
		width: 36px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: none;
		font-size: 18px;
		font-weight: 300;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.pill-add:hover {
		color: var(--ca-text-primary);
	}
	.pill-input {
		flex: 1;
		background: none;
		border: none;
		outline: none;
		font-size: 13.5px;
		color: var(--ca-text-primary);
		padding: 0 4px;
		line-height: 36px;
	}
	.pill-input::placeholder {
		color: var(--ca-text-secondary);
	}
	.pill-send {
		flex: none;
		width: 32px;
		height: 32px;
		margin: 2px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in oklab, var(--ca-brand-main) 35%, transparent);
		border: none;
		color: var(--ca-text-secondary);
		font-size: 15px;
		cursor: default;
		transition:
			background 120ms,
			color 120ms;
	}
	.pill-send.active {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		cursor: default;
	}
	.queue-hint {
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: center;
		margin-top: 6px;
	}

	/* ─── profile panel overlay ──────────────────────────────── */
	.panel-overlay {
		position: absolute;
		inset: 0;
		z-index: 49;
	}
	.profile-panel {
		position: absolute;
		right: 0;
		top: 0;
		bottom: 0;
		width: 300px;
		z-index: 50;
		background: var(--ca-surface-base);
		border-left: 0.5px solid var(--ca-outline-subtle);
		overflow-y: auto;
		padding: 14px 16px 24px;
		display: flex;
		flex-direction: column;
		gap: 0;
	}
	.close-btn {
		background: none;
		border: none;
		font-size: 13px;
		color: var(--ca-text-secondary);
		cursor: default;
		padding: 2px 4px;
		align-self: flex-end;
		margin-bottom: 8px;
	}
	.close-btn:hover {
		color: var(--ca-text-primary);
	}
	.profile-header {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding-bottom: 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		margin-bottom: 4px;
	}
	.profile-avatar {
		width: 48px;
		height: 48px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 20px;
		font-weight: 700;
	}
	.profile-name {
		font-size: 15px;
		font-weight: 600;
		text-align: center;
	}
	.profile-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		text-align: center;
	}

	/* profile sections */
	.profile-section {
		padding: 14px 0;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.profile-section-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
	}
	.profile-textarea {
		width: 100%;
		font-size: 13px;
		padding: 7px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		resize: vertical;
		box-sizing: border-box;
		line-height: 1.4;
		font-family: inherit;
	}
	.profile-textarea:focus {
		outline: none;
		border-color: var(--ca-action-focus);
	}
	.profile-select {
		font-size: 13px;
		padding: 6px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		width: 100%;
	}
	.profile-foot {
		display: flex;
		gap: 8px;
	}
	.hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
	}

	/* routines (inside profile panel) */
	.routine-row {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
	}
	.routine-cron {
		font-family: var(--ca-font-mono, monospace);
		color: var(--ca-text-secondary);
		white-space: nowrap;
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
		align-items: center;
	}
	.cron-input {
		width: 100px;
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

	/* memory textarea */
	.memory-area {
		width: 100%;
		font-family: var(--ca-font-mono, monospace);
		font-size: 12px;
		padding: 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		resize: vertical;
		box-sizing: border-box;
	}
	.memory-area:focus {
		outline: none;
		border-color: var(--ca-action-focus);
	}

	/* delete bot */
	.profile-delete-wrap {
		padding-top: 20px;
		display: flex;
		justify-content: center;
	}
	.delete-btn {
		background: none;
		border: none;
		font-size: 13px;
		color: var(--ca-semantic-error);
		cursor: default;
		padding: 4px 8px;
		border-radius: 5px;
	}
	.delete-btn:hover {
		background: color-mix(in oklab, var(--ca-semantic-error) 10%, transparent);
	}
</style>
