<script lang="ts">
	/**
	 * BotView — D2+D3: chat layout with context menu and profile side panel.
	 * Approval is inline in the thread; Telegram and Pair phone moved to D5 BotsSettings.
	 */
	import { onMount } from "svelte";
	import type { Component } from "svelte";
	import { Button, Spinner, toast } from "@codeatlasdev/ui";
	import {
		ArrowDown,
		ArrowUp,
		Check,
		ChevronDown,
		ChevronRight,
		ChevronUp,
		Clock,
		Copy,
		Ellipsis,
		FileText,
		Folder,
		GitBranch,
		GitPullRequest,
		Link,
		Bot as BotIcon,
		RotateCcw,
		Settings2,
		Terminal,
		Timer,
		TriangleAlert,
		X,
		Zap,
	} from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import type { BotProject, BotTask, ScheduleTrigger, Trigger, TriggerKind } from "#lib/bots.ts";
	import BotEditor from "#lib/bots/BotEditor.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { normalizeLook, type BotLook } from "#lib/bots/look.ts";
	import TypingIndicator from "#lib/bots/TypingIndicator.svelte";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";
	import { groupTasks, firstUnreadIndex, roleTag } from "#lib/bots/botsLogic.ts";
	import { msgIn, msgOut, embedIn, optionIn } from "#lib/bots/motion.ts";
	import UsageSparkline from "#lib/bots/UsageSparkline.svelte";

	// ─── ask card ─────────────────────────────────────────────────────────────

	interface AskPayload {
		id: string;
		question: string;
		options: Array<{ key: string; title: string; subtitle?: string }>;
		allow_text?: boolean;
	}

	function parseAsk(result?: string): AskPayload | null {
		if (!result?.trimStart().startsWith("{")) return null;
		try {
			const v = JSON.parse(result);
			if (v && typeof v === "object" && "ask" in v) return v.ask as AskPayload;
		} catch {
			/* not JSON */
		}
		return null;
	}

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

	// usage (lazy-loaded when profile opens)
	let usageData = $state<number[]>([]);
	let usageTotals = $state({ today: 0, week: 0, month: 0 });

	// routines
	let routinesList = $state<ScheduleTrigger[]>([]);
	let newRoutineCron = $state("");
	let newRoutinePrompt = $state("");

	// triggers (new trigger form)
	let newTriggerKind = $state<TriggerKind | null>(null);
	let newTriggerForm = $state<Record<string, string | boolean>>({});

	// thread
	let stepsOpen = $state<Record<string, boolean>>({});
	let approvedReqs = $state<Set<string>>(new Set());
	/** Actual hostname of the local hostd, fetched on first selection of a local bot. */
	let localHostname = $state("");

	// markdown collapse
	const COLLAPSE_LINES = 12;
	let expandedResults = $state<Set<string>>(new Set());

	// hover toolbar (CSS-driven)
	let hoveredTask = $state<string | null>(null);

	// jump to present
	let threadEl = $state<HTMLElement | null>(null);
	let atBottom = $state(true);
	let jumpVisible = $state(false);

	// ─── derived ──────────────────────────────────────────────────────────────

	const bot = $derived(bots.selectedBot);
	const tasks = $derived(bots.tasks);
	const agentList = $derived(agents.list.filter((a) => a.installed));
	const hostOnline = $derived(bots.selected ? bots.hostOk(bots.selected.hostKey) : true);
	const serverName = $derived(bots.selected?.hostName ?? "");
	/** Show actual hostname for local; server name label for remote. */
	const displayServerName = $derived(bots.selected?.hostKey ? serverName : localHostname || serverName);
	const queuedCount = $derived(tasks.filter((t) => t.state === "queued").length);
	const faceState = $derived<FaceState>(
		bot?.state === "needs_you" ? "needs_you" : bot?.state === "working" ? "working" : "idle",
	);

	const orderedTasks = $derived([...tasks].reverse()); // oldest first
	const groupedTasks = $derived(groupTasks(orderedTasks));
	const lastSeenTs = $derived(bots.selectedLastSeen);
	const unreadIdx = $derived(firstUnreadIndex(orderedTasks, lastSeenTs));
	const allTriggers = $derived(bot?.triggers ?? []);

	const AGENT_LABELS: Record<string, string> = {
		omp: "omp",
		claude: "Claude Code",
		codex: "Codex",
		opencode: "OpenCode",
	};

	const agentLabel = $derived(
		bot ? (agents.list.find((a) => a.id === bot.agent)?.label ?? AGENT_LABELS[bot.agent] ?? bot.agent) : "",
	);

	// IDE projects for project linking
	const ideProjects = $derived(
		ws.doc.worktrees.map((w) => ({ id: w.id, name: w.name ?? w.id, branch: w.branch ?? "" })),
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
			void bots
				.getUsage()
				.then(({ records }) => {
					const now2 = Date.now();
					const todayStart = new Date();
					todayStart.setHours(0, 0, 0, 0);
					const weekStart = now2 - 7 * 24 * 60 * 60 * 1000;
					const monthStart = now2 - 30 * 24 * 60 * 60 * 1000;
					const buckets = new Array(30).fill(0);
					for (const rec of records) {
						const tsMs = rec.ts * 1000;
						const dayIdx = Math.floor((now2 - tsMs) / (24 * 60 * 60 * 1000));
						if (dayIdx >= 0 && dayIdx < 30)
							buckets[29 - dayIdx] += (rec.input_tokens ?? 0) + (rec.output_tokens ?? 0);
					}
					usageData = buckets;
					usageTotals = {
						today: records
							.filter((r) => r.ts * 1000 >= todayStart.getTime())
							.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
						week: records
							.filter((r) => r.ts * 1000 >= weekStart)
							.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
						month: records
							.filter((r) => r.ts * 1000 >= monthStart)
							.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
					};
				})
				.catch(() => {});
		}
	});

	onMount(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// Auto-scroll to bottom when new tasks arrive, if already at bottom
	$effect(() => {
		const _ = orderedTasks.length;
		if (atBottom && threadEl) {
			setTimeout(() => threadEl?.scrollTo({ top: threadEl.scrollHeight }), 0);
		}
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

	function fmtTokens(n: number): string {
		if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
		return String(n);
	}

	function fmtDate(tsMs: number): string {
		const d = new Date(tsMs);
		const today = new Date();
		if (d.toDateString() === today.toDateString()) return "Today";
		const yesterday = new Date(today);
		yesterday.setDate(today.getDate() - 1);
		if (d.toDateString() === yesterday.toDateString()) return "Yesterday";
		return d.toLocaleDateString([], { month: "long", day: "numeric" });
	}

	function shouldCollapse(text: string): boolean {
		return text.split("\n").length > COLLAPSE_LINES;
	}

	function resultHtml(text: string): string {
		return renderMarkdown(text, "", false);
	}

	function onThreadScroll() {
		if (!threadEl) return;
		const { scrollTop, scrollHeight, clientHeight } = threadEl;
		atBottom = scrollHeight - scrollTop - clientHeight < 60;
		jumpVisible = !atBottom;
	}

	function jumpToBottom() {
		threadEl?.scrollTo({ top: threadEl.scrollHeight, behavior: "smooth" });
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

	let editingLook = $state<BotLook | null>(null);
	async function saveLook() {
		if (!bots.selected || !bot || !editingLook) return;
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { look: editingLook });
			editingLook = null;
		} catch (e) {
			toast.error(errText(e));
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
		routinesList = allTriggers.filter((t): t is ScheduleTrigger => t.kind.type === "schedule");
	}

	// ─── trigger helpers ──────────────────────────────────────────────────────

	const TRIGGER_ICONS: Record<TriggerKind, Component> = {
		schedule: Clock,
		interval: Timer,
		webhook: Link,
		github: GitPullRequest,
		file: FileText,
		bot: BotIcon,
		service: Settings2,
	};

	const TRIGGER_KINDS: Array<{ kind: TriggerKind; label: string }> = [
		{ kind: "schedule", label: "Schedule" },
		{ kind: "interval", label: "Interval" },
		{ kind: "webhook", label: "Webhook" },
		{ kind: "github", label: "GitHub" },
		{ kind: "file", label: "File" },
		{ kind: "bot", label: "Bot chain" },
		{ kind: "service", label: "Service" },
	];
	const GITHUB_EVENTS: Array<{ ev: string; label: string }> = [
		{ ev: "pr_opened", label: "PR opened" },
		{ ev: "pr_review_requested", label: "Review requested" },
		{ ev: "issue_opened", label: "Issue opened" },
		{ ev: "ci_failed", label: "CI failed" },
		{ ev: "release", label: "Release" },
	];

	function triggerIconComponent(kind: TriggerKind | string): Component {
		return (TRIGGER_ICONS as Record<string, Component>)[kind] ?? Zap;
	}

	function triggerSummary(t: Trigger): string {
		const k = t.kind;
		switch (k.type) {
			case "schedule":
				return k.cron;
			case "interval":
				return `Every ${k.every_secs}s`;
			case "webhook":
				return `webhook (token: ${k.token.slice(0, 6)}…)`;
			case "github":
				return `${k.repo} · ${k.events.length} event${k.events.length !== 1 ? "s" : ""}`;
			case "file":
				return k.glob ?? k.path;
			case "bot":
				return `${k.from_bot} · ${k.on}`;
			case "service":
				return `${k.service} · ${k.on}`;
			default:
				return "";
		}
	}

	async function toggleTrigger(id: string, enabled: boolean) {
		const updated = allTriggers.map((t) => (t.id === id ? { ...t, enabled } : t));
		try {
			await bots.setTriggers(updated);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function removeTrigger(id: string) {
		const updated = allTriggers.filter((t) => t.id !== id);
		try {
			await bots.setTriggers(updated);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function addTrigger() {
		if (!newTriggerKind) return;
		const id = crypto.randomUUID();
		let trig: Trigger;
		const k = newTriggerKind;
		const f = newTriggerForm;

		if (k === "schedule") {
			if (!newRoutineCron.trim() || !newRoutinePrompt.trim()) return;
			trig = {
				id,
				kind: { type: "schedule", cron: newRoutineCron.trim() },
				prompt: newRoutinePrompt.trim(),
				enabled: true,
			};
		} else if (k === "interval") {
			const secs = parseInt(f.every_secs as string);
			if (!secs || secs < 60) return;
			trig = {
				id,
				kind: { type: "interval", every_secs: secs },
				prompt: (f.prompt as string) ?? "",
				enabled: true,
			};
		} else if (k === "webhook") {
			trig = {
				id,
				kind: { type: "webhook", token: crypto.randomUUID().replace(/-/g, "").slice(0, 24) },
				prompt: (f.prompt as string) ?? "Webhook event: {{event}}",
				enabled: true,
			};
		} else if (k === "github") {
			if (!f.repo) return;
			const events = GITHUB_EVENTS.map((e) => e.ev).filter((ev) => !!f[ev]);
			trig = {
				id,
				kind: { type: "github", repo: f.repo as string, events },
				prompt: (f.prompt as string) ?? "GitHub: {{event}}",
				enabled: true,
			};
		} else if (k === "file") {
			if (!f.path && !f.glob) return;
			trig = {
				id,
				kind: { type: "file", path: (f.path as string) ?? "", glob: (f.glob as string) || undefined },
				prompt: (f.prompt as string) ?? "File changed: {{event}}",
				enabled: true,
			};
		} else if (k === "bot") {
			if (!f.from_bot) return;
			trig = {
				id,
				kind: {
					type: "bot",
					from_bot: f.from_bot as string,
					on: ((f.on as string) || "done") as "done" | "failed" | "both",
				},
				prompt: (f.prompt as string) ?? "Bot finished: {{event}}",
				enabled: true,
			};
		} else {
			if (!f.service) return;
			trig = {
				id,
				kind: {
					type: "service",
					project_dir: (f.project_dir as string) ?? "",
					service: f.service as string,
					on: ((f.on as string) || "failed") as "failed" | "recovered" | "both",
				},
				prompt: (f.prompt as string) ?? "Service {{event}}",
				enabled: true,
			};
		}

		try {
			await bots.setTriggers([...allTriggers, trig]);
			newTriggerKind = null;
			newTriggerForm = {};
			newRoutineCron = "";
			newRoutinePrompt = "";
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

	async function linkProject(p: { id: string; name: string; branch: string }) {
		if (!bots.selected || !bot) return;
		const project: BotProject = { name: p.name, kind: "path", branch: p.branch || undefined };
		await bots.updateBot(bots.selected.hostKey, bot.id, { project }).catch((e) => toast.error(errText(e)));
	}

	async function unlinkProject() {
		if (!bots.selected || !bot) return;
		await bots
			.updateBot(bots.selected.hostKey, bot.id, { project: null })
			.catch((e) => toast.error(errText(e)));
	}
</script>

<div class="bot-view">
	{#if !bot}
		<div class="empty">Select a bot from the sidebar.</div>
	{:else}
		<!-- Header -->
		<div class="header">
			<div class="header-left">
				<BotFace seed={bot.id} look={bot.look} state={faceState} size={30} />
				<div class="header-title">
					<span class="bot-name">{bot.name}</span>
					{#if roleTag(bot.role)}
						<span class="header-role-tag">{roleTag(bot.role)}</span>
					{/if}
					{#if bot.project}
						<span class="header-project-chip">
							{#if bot.project.kind === "git"}
								<GitBranch size={12} aria-hidden="true" />
							{:else}
								<Folder size={12} aria-hidden="true" />
							{/if}
							{bot.project.name}
							{#if bot.project.branch}
								<span class="header-project-branch">{bot.project.branch}</span>
							{/if}
						</span>
					{/if}
					{#if bot.state === "working" && bot.running_task}
						<span class="header-presence"
							>working · {fmtElapsed(
								bot.running_task
									? now - (bots.tasks.find((t) => t.id === bot.running_task!.id)?.started_at || now)
									: 0,
							)}</span
						>
					{:else if agentLabel}
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
						onclick={() => (menuOpen = !menuOpen)}><Ellipsis size={16} /></button
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
				<TriangleAlert size={14} aria-hidden="true" />
				{displayServerName || "Server"} is offline
			</div>
		{/if}

		<!-- Task thread -->
		<div class="thread" bind:this={threadEl} onscroll={onThreadScroll} aria-label="Task history">
			{#if bots.tasksLoading && tasks.length === 0}
				<div class="loading">Loading…</div>
			{:else if tasks.length === 0}
				<div class="empty-thread">No tasks yet. Send one below.</div>
			{:else}
				{#each orderedTasks as t, i (t.id)}
					<!-- Unread divider -->
					{#if i === unreadIdx}
						<div class="unread-divider" role="separator">
							<span class="unread-label">NEW</span>
						</div>
					{/if}
					<!-- Date separator on group start + day change -->
					{#if groupedTasks[i]?.groupStart && i > 0}
						{@const prev = orderedTasks[i - 1]}
						{#if new Date(t.ts).toDateString() !== new Date(prev.ts).toDateString()}
							<div class="msg-ts">{fmtDate(t.ts)}</div>
						{/if}
					{/if}
					<!-- Row: user prompt + bot response -->
					<div
						class="msg-row"
						class:group-start={groupedTasks[i]?.groupStart}
						onmouseenter={() => (hoveredTask = t.id)}
						onmouseleave={() => (hoveredTask = null)}
					>
						<!-- User bubble -->
						{#if t.trigger_event}
							{@const TrigIcon = triggerIconComponent(t.trigger_event.kind)}
							<div class="trigger-event" role="note">
								<span class="trigger-icon" aria-hidden="true"><TrigIcon size={14} /></span>
								<span class="trigger-summary">{t.trigger_event.summary}</span>
							</div>
						{/if}
						<div class="msg-out" in:msgOut>
							<div class="bubble out ca-selectable">{t.prompt}</div>
							{#if t.source !== "desktop"}
								<div class="msg-meta">· {t.source}</div>
							{/if}
							{#if t.state === "queued"}
								<div class="msg-meta muted">Queued</div>
							{/if}
						</div>

						<!-- Bot response: running -->
						{#if t.state === "running"}
							{@const elapsedMs = t.started_at ? now - t.started_at : 0}
							<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
								{#if groupedTasks[i]?.groupStart}
									<div class="msg-avatar">
										<BotFace seed={bot.id} look={bot.look} state="working" size={40} />
									</div>
								{:else}
									<div class="msg-avatar-gap"></div>
								{/if}
								<div class="msg-in-body">
									{#if groupedTasks[i]?.groupStart}
										<div class="msg-author">
											<span class="author-name">{bot.name}</span>
											<span class="author-time">{fmtTime(t.ts)}</span>
										</div>
									{/if}
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
												{#if stepsOpen[t.id]}
													<ChevronDown size={12} aria-hidden="true" />
												{:else}
													<ChevronRight size={12} aria-hidden="true" />
												{/if}
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
											<div class="approval-answered" in:msgIn>
												<Check size={12} aria-hidden="true" /> Allowed by you · {fmtTime(Date.now())}
											</div>
										{:else}
											<div class="approval-bubble" role="alert" in:embedIn>
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
							<!-- Bot response: done -->
						{:else if t.state === "done"}
							{@const raw = t.result ?? "Done"}
							{@const collapsed = shouldCollapse(raw) && !expandedResults.has(t.id)}
							<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
								{#if groupedTasks[i]?.groupStart}
									<div class="msg-avatar">
										<BotFace seed={bot.id} look={bot.look} state={faceState} size={40} />
									</div>
								{:else}
									<div class="msg-avatar-gap"></div>
								{/if}
								<div class="msg-in-body">
									{#if groupedTasks[i]?.groupStart}
										<div class="msg-author">
											<span class="author-name">{bot.name}</span>
											<span class="author-time">{fmtTime(t.ts)}</span>
										</div>
									{/if}
									{#each [parseAsk(t.result)] as ask}
										{#if ask && !expandedResults.has(t.id + ":dismissed")}
											<!-- Ask card -->
											<div class="ask-card">
												<div class="ask-question">
													<span>{ask.question}</span>
													<button
														class="ask-dismiss"
														aria-label="Dismiss"
														onclick={() => {
															const s = new Set(expandedResults);
															s.add(t.id + ":dismissed");
															expandedResults = s;
														}}>×</button
													>
												</div>
												{#each ask.options as opt, oi (opt.key)}
													<button
														class="ask-option"
														onclick={() => {
															prompt = opt.title;
															void send();
														}}
													>
														<span class="ask-key">{String.fromCharCode(65 + oi)}</span>
														<div class="ask-opt-body">
															<span class="ask-opt-title">{opt.title}</span>
															{#if opt.subtitle}<span class="ask-opt-sub">{opt.subtitle}</span>{/if}
														</div>
													</button>
												{/each}
												{#if ask.allow_text !== false}
													<input
														class="ask-text"
														placeholder="Type your own answer"
														onkeydown={(e) => {
															if (e.key === "Enter" && (e.currentTarget as HTMLInputElement).value.trim()) {
																prompt = (e.currentTarget as HTMLInputElement).value.trim();
																void send();
															}
														}}
													/>
												{/if}
											</div>
										{:else if !expandedResults.has(t.id + ":dismissed")}
											<!-- Normal markdown result -->
											<div class="md-result ca-selectable" class:collapsed>
												{@html resultHtml(
													collapsed ? raw.split("\n").slice(0, COLLAPSE_LINES).join("\n") : raw,
												)}
											</div>
											{#if shouldCollapse(raw)}
												<button
													class="show-more"
													onclick={() => {
														const s = new Set(expandedResults);
														if (collapsed) s.add(t.id);
														else s.delete(t.id);
														expandedResults = s;
													}}
												>
													{collapsed ? "Show more" : "Show less"}
													{#if collapsed}<ChevronDown size={12} aria-hidden="true" />{:else}<ChevronUp
															size={12}
															aria-hidden="true"
														/>{/if}
												</button>
											{/if}
											{#if t.steps && t.steps.length > 0}
												<div class="steps-wrap">
													<button
														class="steps-btn"
														aria-expanded={!!stepsOpen[t.id]}
														onclick={() => (stepsOpen = { ...stepsOpen, [t.id]: !stepsOpen[t.id] })}
													>
														{#if stepsOpen[t.id]}
															<ChevronDown size={12} aria-hidden="true" />
														{:else}
															<ChevronRight size={12} aria-hidden="true" />
														{/if}
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
										{/if}
									{/each}
									{#if t.usage}
										<div class="usage-footer">
											{#if t.usage.input_tokens || t.usage.output_tokens}
												<span
													>{fmtTokens(t.usage.input_tokens ?? 0)}in · {fmtTokens(
														t.usage.output_tokens ?? 0,
													)}out</span
												>
											{/if}
											{#if t.usage.cost_usd}
												<span>${t.usage.cost_usd.toFixed(4)}</span>
											{/if}
											{#if t.usage.duration_ms}
												<span>{fmtElapsed(t.usage.duration_ms)}</span>
											{/if}
											{#if t.usage.model}
												<span class="usage-model">{t.usage.model}</span>
											{/if}
										</div>
									{/if}
								</div>
							</div>
							<!-- Bot response: failed -->
						{:else if t.state === "failed"}
							<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
								{#if groupedTasks[i]?.groupStart}
									<div class="msg-avatar">
										<BotFace seed={bot.id} look={bot.look} state="offline" size={40} />
									</div>
								{:else}
									<div class="msg-avatar-gap"></div>
								{/if}
								<div class="msg-in-body">
									{#if groupedTasks[i]?.groupStart}
										<div class="msg-author">
											<span class="author-name">{bot.name}</span>
											<span class="author-time">{fmtTime(t.ts)}</span>
										</div>
									{/if}
									<div class="bubble in error ca-selectable">{t.error ?? "Failed"}</div>
								</div>
							</div>
							<!-- Bot response: cancelled -->
						{:else if t.state === "cancelled"}
							<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
								{#if groupedTasks[i]?.groupStart}
									<div class="msg-avatar">
										<BotFace seed={bot.id} look={bot.look} state="idle" size={40} />
									</div>
								{:else}
									<div class="msg-avatar-gap"></div>
								{/if}
								<div class="msg-in-body">
									{#if groupedTasks[i]?.groupStart}
										<div class="msg-author">
											<span class="author-name">{bot.name}</span>
											<span class="author-time">{fmtTime(t.ts)}</span>
										</div>
									{/if}
									<div class="bubble in muted">Cancelled</div>
								</div>
							</div>
						{/if}

						<!-- Hover toolbar (shown via CSS .msg-row:hover) -->
						<div class="msg-toolbar">
							<button
								class="tb-btn"
								title="Copy"
								onclick={() =>
									navigator.clipboard.writeText(t.state === "done" ? (t.result ?? "") : t.prompt)}
								><Copy size={14} /></button
							>
							{#if t.state === "failed" || t.state === "cancelled"}
								<button
									class="tb-btn"
									title="Retry"
									onclick={() => {
										prompt = t.prompt;
										void send();
									}}><RotateCcw size={14} /></button
								>
							{/if}
							{#if t.state === "running" && bot.running_task?.pane}
								<button class="tb-btn" title="Open terminal" onclick={openTerminal}
									><Terminal size={14} /></button
								>
							{/if}
						</div>
					</div>
				{/each}
			{/if}
		</div>

		{#if jumpVisible}
			<div class="jump-present" role="status">
				<button class="jump-btn" onclick={jumpToBottom}
					>Jump to present <ArrowDown size={12} aria-hidden="true" /></button
				>
			</div>
		{/if}

		{#if bot.state === "working"}
			<TypingIndicator bots={[{ id: bot.id, name: bot.name }]} />
		{/if}

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
					onclick={send}><ArrowUp size={14} /></button
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
				<button class="close-btn" onclick={() => (panel = "none")} aria-label="Close panel"
					><X size={14} /></button
				>

				<div class="profile-header">
					<button
						class="profile-avatar"
						aria-label="Customize look"
						onclick={() => (editingLook = normalizeLook(bot.look, bot.id))}
					>
						<BotFace seed={bot.id} look={bot.look} state={faceState} size={72} track />
					</button>
					<div class="profile-name">{bot.name}</div>
					<div class="profile-sub">
						{agentLabel}{serverName ? ` · ${serverName}` : ""}
					</div>
				</div>

				{#if editingLook}
					<div class="profile-section">
						<BotEditor bind:look={editingLook} seed={bot.id} />
						<div class="profile-foot">
							<Button size="sm" onclick={() => (editingLook = null)}>Cancel</Button>
							<Button size="sm" onclick={saveLook}>Save look</Button>
						</div>
					</div>
				{/if}

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
					<select
						class="profile-select"
						bind:value={settingsApproval}
						disabled={bot.approval_supported === false}
						aria-label="Approval mode"
					>
						<option value="ask">Ask before acting</option>
						<option value="auto">Automatic (no prompts)</option>
					</select>
					{#if bot.approval_supported === false}
						<p class="hint">OpenCode bots run without approval prompts.</p>
					{:else}
						<div class="profile-foot">
							<Button size="sm" disabled={approvalSaving} onclick={saveApproval}>
								{approvalSaving ? "Saving…" : "Save"}
							</Button>
						</div>
					{/if}
				</div>

				<!-- Project -->
				<div class="profile-section">
					<div class="profile-section-title">Project</div>
					{#if bot.project}
						<div class="project-linked">
							<span class="project-kind-icon">
								{#if bot.project.kind === "git"}
									<GitBranch size={13} aria-hidden="true" />
								{:else}
									<Folder size={13} aria-hidden="true" />
								{/if}
							</span>
							<div class="project-details">
								<span class="project-name">{bot.project.name}</span>
								{#if bot.project.url}<span class="project-url">{bot.project.url}</span>{/if}
								{#if bot.project.branch}<span class="project-branch">@ {bot.project.branch}</span>{/if}
							</div>
							<button class="close-btn" onclick={() => void unlinkProject()} aria-label="Unlink project">
								<X size={14} /></button
							>
						</div>
					{:else}
						<div class="project-link-options">
							{#each ideProjects as p (p.id)}
								<button class="project-option" onclick={() => void linkProject(p)}>
									<GitBranch size={12} aria-hidden="true" />
									{p.name}
									{#if p.branch}<span class="muted">{p.branch}</span>{/if}
								</button>
							{/each}
							{#if ideProjects.length === 0}
								<p class="hint">No IDE projects found. Open a project in the IDE first.</p>
							{/if}
						</div>
					{/if}
				</div>

				<!-- Triggers -->
				<div class="profile-section">
					<div class="profile-section-title">Triggers</div>
					{#each allTriggers as t, i (t.id)}
						{@const TrigIcon = triggerIconComponent(t.kind.type as TriggerKind)}
						<div class="trigger-row" in:optionIn={{ index: i }}>
							<span class="trigger-row-icon" aria-hidden="true"><TrigIcon size={14} /></span>
							<span class="trigger-row-summary">{triggerSummary(t)}</span>
							<input
								type="checkbox"
								checked={t.enabled}
								aria-label="Enable trigger"
								onchange={(e) => void toggleTrigger(t.id, (e.currentTarget as HTMLInputElement).checked)}
							/>
							<button class="close-btn" onclick={() => void removeTrigger(t.id)} aria-label="Remove trigger">
								<X size={14} /></button
							>
						</div>
					{:else}
						<p class="hint">No triggers yet.</p>
					{/each}

					{#if newTriggerKind === null}
						<div class="add-trigger-picker">
							<span class="hint">Add trigger:</span>
							{#each TRIGGER_KINDS as tk}
								{@const TkIcon = triggerIconComponent(tk.kind)}
								<button
									class="trigger-kind-btn"
									onclick={() => {
										newTriggerKind = tk.kind;
										newTriggerForm = {};
									}}><TkIcon size={13} aria-hidden="true" /> {tk.label}</button
								>
							{/each}
						</div>
					{:else}
						<div class="add-trigger-form">
							{#if newTriggerKind === "schedule"}
								<input
									class="cron-input"
									bind:value={newRoutineCron}
									placeholder="0 9 * * 1-5"
									aria-label="Cron expression"
								/>
								<textarea
									class="profile-textarea ca-selectable"
									bind:value={newRoutinePrompt}
									rows={3}
									placeholder="Daily standup summary"
									aria-label="Prompt"></textarea>
								<div class="cron-presets">
									<button class="preset" onclick={() => (newRoutineCron = "0 8 * * *")}
										>Every morning 8:00</button
									>
									<button class="preset" onclick={() => (newRoutineCron = "0 9 * * 1-5")}
										>Weekdays 9:00</button
									>
								</div>
							{:else if newTriggerKind === "interval"}
								<div class="add-routine">
									<input
										class="cron-input"
										type="number"
										min="1"
										value={newTriggerForm.every ?? ""}
										oninput={(e) => {
											newTriggerForm = {
												...newTriggerForm,
												every: (e.currentTarget as HTMLInputElement).value,
											};
										}}
										placeholder="30"
										aria-label="Interval amount"
									/>
									<select
										class="profile-select"
										value={newTriggerForm.unit ?? "minutes"}
										onchange={(e) => {
											newTriggerForm = {
												...newTriggerForm,
												unit: (e.currentTarget as HTMLSelectElement).value,
											};
										}}
										aria-label="Interval unit"
									>
										<option value="minutes">minutes</option>
										<option value="hours">hours</option>
										<option value="days">days</option>
									</select>
								</div>
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt"
									aria-label="Prompt"></textarea>
							{:else if newTriggerKind === "webhook"}
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt_template ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt_template: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt template"
									aria-label="Prompt template"></textarea>
							{:else if newTriggerKind === "github"}
								<input
									class="prompt-input"
									value={newTriggerForm.repo ?? ""}
									oninput={(e) => {
										newTriggerForm = { ...newTriggerForm, repo: (e.currentTarget as HTMLInputElement).value };
									}}
									placeholder="owner/repo"
									aria-label="GitHub repo"
								/>
								<div class="github-events">
									{#each GITHUB_EVENTS as ev}
										<label class="event-check">
											<input
												type="checkbox"
												checked={!!newTriggerForm[ev.ev]}
												onchange={(e) => {
													newTriggerForm = {
														...newTriggerForm,
														[ev.ev]: (e.currentTarget as HTMLInputElement).checked,
													};
												}}
												aria-label={ev.label}
											/>
											{ev.label}
										</label>
									{/each}
								</div>
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt_template ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt_template: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt template"
									aria-label="Prompt template"></textarea>
							{:else if newTriggerKind === "file"}
								<input
									class="prompt-input"
									value={newTriggerForm.glob ?? ""}
									oninput={(e) => {
										newTriggerForm = { ...newTriggerForm, glob: (e.currentTarget as HTMLInputElement).value };
									}}
									placeholder="**/*.ts"
									aria-label="File glob"
								/>
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt_template ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt_template: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt template"
									aria-label="Prompt template"></textarea>
							{:else if newTriggerKind === "bot"}
								<select
									class="profile-select"
									value={newTriggerForm.bot_id ?? ""}
									onchange={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											bot_id: (e.currentTarget as HTMLSelectElement).value,
										};
									}}
									aria-label="Bot"
								>
									<option value="">Select bot…</option>
									{#each bots.allBots as { bot: b }}
										<option value={b.id}>{b.name}</option>
									{/each}
								</select>
								<select
									class="profile-select"
									value={newTriggerForm.on ?? "done"}
									onchange={(e) => {
										newTriggerForm = { ...newTriggerForm, on: (e.currentTarget as HTMLSelectElement).value };
									}}
									aria-label="On event"
								>
									<option value="done">Done</option>
									<option value="failed">Failed</option>
									<option value="both">Both</option>
								</select>
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt_template ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt_template: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt template"
									aria-label="Prompt template"></textarea>
							{:else if newTriggerKind === "service"}
								<input
									class="prompt-input"
									value={newTriggerForm.service ?? ""}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											service: (e.currentTarget as HTMLInputElement).value,
										};
									}}
									placeholder="Service name"
									aria-label="Service name"
								/>
								<select
									class="profile-select"
									value={newTriggerForm.on ?? "failed"}
									onchange={(e) => {
										newTriggerForm = { ...newTriggerForm, on: (e.currentTarget as HTMLSelectElement).value };
									}}
									aria-label="On event"
								>
									<option value="failed">Failed</option>
									<option value="recovered">Recovered</option>
									<option value="both">Both</option>
								</select>
								<textarea
									class="profile-textarea ca-selectable"
									value={String(newTriggerForm.prompt_template ?? "")}
									oninput={(e) => {
										newTriggerForm = {
											...newTriggerForm,
											prompt_template: (e.currentTarget as HTMLTextAreaElement).value,
										};
									}}
									rows={3}
									placeholder="Prompt template"
									aria-label="Prompt template"></textarea>
							{/if}
							<div class="profile-foot">
								<Button
									size="sm"
									variant="ghost"
									onclick={() => {
										newTriggerKind = null;
										newTriggerForm = {};
									}}>Cancel</Button
								>
								<Button size="sm" onclick={addTrigger}>Add</Button>
							</div>
						</div>
					{/if}
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

				<!-- Usage -->
				<div class="profile-section">
					<div class="profile-section-title">Usage</div>
					<UsageSparkline data={usageData} />
					<div class="usage-totals">
						<span class="usage-stat">Today: {fmtTokens(usageTotals.today)}</span>
						<span class="usage-stat">7d: {fmtTokens(usageTotals.week)}</span>
						<span class="usage-stat">30d: {fmtTokens(usageTotals.month)}</span>
					</div>
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
	.header-role-tag {
		font-size: 10.5px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-outline-subtle) 60%, transparent);
		border-radius: 10px;
		padding: 2px 7px;
		margin-left: 4px;
	}
	.header-presence {
		font-size: 12px;
		color: var(--ca-semantic-warning, #e67700);
		margin-left: 4px;
	}
	.header-right {
		flex: none;
	}

	/* context menu */
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
		margin: 12px 0;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.msg-ts::before,
	.msg-ts::after {
		content: "";
		flex: 1;
		height: 0.5px;
		background: var(--ca-outline-subtle);
	}
	.msg-out {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		margin-bottom: 2px;
		padding: 1px 16px;
	}
	.msg-in {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 1px 16px;
		position: relative;
	}
	.msg-in.group-start {
		margin-top: 8px;
		padding-top: 1px;
	}
	.msg-avatar {
		width: 40px;
		flex: none;
	}
	.msg-avatar-gap {
		width: 40px;
		flex: none;
	}
	.msg-in-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
	}
	.msg-author {
		display: flex;
		align-items: baseline;
		gap: 6px;
		margin-bottom: 2px;
	}
	.author-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.author-time {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	/* msg-row wraps user + bot bubbles for a single task */
	.msg-row {
		position: relative;
		display: flex;
		flex-direction: column;
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
		display: flex;
		align-items: center;
		gap: 6px;
		white-space: nowrap;
		word-break: normal;
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

	/* ─── markdown result ────────────────────────────────────── */
	.md-result {
		font-size: 13px;
		line-height: 1.6;
		color: var(--ca-text-primary);
		word-break: break-word;
		max-width: 72%;
	}
	.md-result :global(h1),
	.md-result :global(h2),
	.md-result :global(h3) {
		font-size: 14px;
		font-weight: 600;
		margin: 0.8em 0 0.4em;
	}
	.md-result :global(p) {
		margin: 0.4em 0;
	}
	.md-result :global(ul),
	.md-result :global(ol) {
		padding-left: 1.4em;
		margin: 0.4em 0;
	}
	.md-result :global(li) {
		margin: 0.2em 0;
	}
	.md-result :global(pre.code) {
		position: relative;
		background: color-mix(in oklab, var(--ca-surface-sunken) 80%, transparent);
		border-radius: 6px;
		padding: 10px 12px;
		overflow-x: auto;
		font-size: 12px;
		margin: 0.6em 0;
	}
	.md-result :global(code) {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		border-radius: 3px;
		padding: 1px 4px;
	}
	.md-result :global(pre.code code) {
		background: none;
		padding: 0;
	}
	.md-result :global(table) {
		border-collapse: collapse;
		width: 100%;
		font-size: 12px;
		margin: 0.6em 0;
	}
	.md-result :global(th),
	.md-result :global(td) {
		border: 0.5px solid var(--ca-outline-subtle);
		padding: 4px 8px;
		text-align: left;
	}
	.md-result :global(th) {
		background: color-mix(in oklab, var(--ca-surface-sunken) 60%, transparent);
		font-weight: 600;
	}
	.show-more {
		font-size: 11.5px;
		color: var(--ca-brand-main);
		background: none;
		border: none;
		padding: 2px 0;
		cursor: default;
		margin-top: 2px;
	}

	/* ─── unread divider ─────────────────────────────────────── */
	.unread-divider {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 12px 16px;
		color: var(--ca-semantic-error, #e03131);
	}
	.unread-divider::before,
	.unread-divider::after {
		content: "";
		flex: 1;
		height: 1px;
		background: var(--ca-semantic-error, #e03131);
		opacity: 0.5;
	}
	.unread-label {
		font-size: 10.5px;
		font-weight: 700;
		letter-spacing: 0.05em;
	}

	/* ─── hover toolbar ──────────────────────────────────────── */
	.msg-toolbar {
		position: absolute;
		top: -10px;
		right: 8px;
		display: none;
		background: var(--ca-surface-overlay, #2a2a2a);
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 2px 4px;
		gap: 2px;
		flex-direction: row;
		align-items: center;
		z-index: 10;
		box-shadow: 0 2px 8px rgb(0 0 0 / 0.2);
	}
	.msg-row:hover .msg-toolbar {
		display: flex;
	}
	.tb-btn {
		width: 26px;
		height: 26px;
		border: none;
		background: none;
		border-radius: 5px;
		color: var(--ca-text-secondary);
		cursor: default;
		font-size: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.tb-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
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

	/* ─── jump to present ────────────────────────────────────── */
	.jump-present {
		position: absolute;
		bottom: 120px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 20;
		pointer-events: none;
	}
	.jump-btn {
		pointer-events: all;
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		border: none;
		border-radius: 20px;
		padding: 5px 14px;
		font-size: 12px;
		cursor: default;
		box-shadow: 0 2px 8px rgb(0 0 0 / 0.3);
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
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
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

	/* routines (inside profile panel) — add-routine/cron-input/prompt-input kept for schedule form */
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

	/* ─── ask card ───────────────────────────────────────────── */
	.ask-card {
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 12px;
		overflow: hidden;
		margin: 4px 0;
		background: color-mix(in oklab, var(--ca-surface-sunken) 60%, transparent);
	}
	.ask-question {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px 8px;
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.ask-dismiss {
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		cursor: default;
		font-size: 16px;
		line-height: 1;
		padding: 0 2px;
		flex: none;
	}
	.ask-dismiss:hover {
		color: var(--ca-text-primary);
	}
	.ask-option {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 14px;
		background: none;
		border: none;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		text-align: left;
		cursor: default;
		transition: background 80ms;
	}
	.ask-option:last-of-type {
		border-bottom: none;
	}
	.ask-option:hover {
		background: var(--ca-vibrancy-fill);
	}
	.ask-key {
		width: 22px;
		height: 22px;
		border-radius: 6px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-secondary);
		font-size: 11px;
		font-weight: 600;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
	}
	.ask-opt-body {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.ask-opt-title {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}
	.ask-opt-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.ask-text {
		display: block;
		width: 100%;
		padding: 10px 14px;
		background: none;
		border: none;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 13px;
		color: var(--ca-text-secondary);
		outline: none;
		box-sizing: border-box;
	}
	.ask-text::placeholder {
		color: var(--ca-text-secondary);
		opacity: 0.6;
	}
	.ask-text:focus {
		color: var(--ca-text-primary);
	}
	.usage-footer {
		display: none;
		align-items: center;
		gap: 8px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		padding: 2px 0;
		flex-wrap: wrap;
	}
	.msg-in:hover .usage-footer {
		display: flex;
	}
	.usage-model {
		font-style: italic;
	}
	.usage-totals {
		display: flex;
		gap: 12px;
		padding: 4px 0;
	}
	.usage-stat {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	/* ── Project chip in header ─────────────────────────────────── */
	.header-project-chip {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 10.5px;
		font-weight: 500;
		background: color-mix(in oklab, var(--ca-outline-subtle) 60%, transparent);
		border-radius: 10px;
		padding: 1px 7px;
		color: var(--ca-text-secondary);
		margin-left: 6px;
		cursor: default;
	}
	.header-project-branch {
		color: var(--ca-text-secondary);
		opacity: 0.7;
		font-size: 10px;
	}
	/* ── Project section in profile panel ───────────────────────── */
	.project-linked {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 0;
	}
	.project-kind-icon {
		font-size: 16px;
		flex: none;
	}
	.project-details {
		flex: 1;
		min-width: 0;
	}
	.project-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		display: block;
	}
	.project-url {
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.project-branch {
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}
	.project-link-options {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.project-option {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		border: none;
		background: none;
		border-radius: 6px;
		text-align: left;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.project-option:hover {
		background: var(--ca-vibrancy-fill);
	}
	.muted {
		color: var(--ca-text-secondary);
		font-size: 11px;
	}

	/* ─── trigger event line (in thread) ────────────────────── */
	.trigger-event {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 16px 3px 68px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin-top: 8px;
	}
	.trigger-icon {
		font-size: 12px;
		flex: none;
	}
	.trigger-summary {
		font-style: italic;
	}

	/* ─── trigger rows (inside profile panel) ───────────────── */
	.trigger-row {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		padding: 2px 0;
	}
	.trigger-row-icon {
		font-size: 14px;
		flex: none;
	}
	.trigger-row-summary {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}

	/* ─── add trigger picker ─────────────────────────────────── */
	.add-trigger-picker {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		align-items: center;
	}
	.trigger-kind-btn {
		font-size: 11px;
		padding: 2px 7px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.trigger-kind-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	/* ─── add trigger form ───────────────────────────────────── */
	.add-trigger-form {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.github-events {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.event-check {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-primary);
		cursor: default;
	}
</style>
