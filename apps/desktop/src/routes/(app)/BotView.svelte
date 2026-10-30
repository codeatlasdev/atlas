<script lang="ts">
	/**
	 * BotView — D2+D3: chat layout with context menu and profile side panel.
	 * Approval is inline in the thread; Telegram and Pair phone moved to D5 BotsSettings.
	 */
	import { onMount } from "svelte";
	import type { Component } from "svelte";
	import { Button, toast } from "@codeatlasdev/ui";
	import {
		ArrowDown,
		ArrowUp,
		Check,
		ChevronDown,
		ChevronUp,
		Clock,
		Copy,
		FileText,
		GitPullRequest,
		Link,
		Bot as BotIcon,
		Plus,
		RotateCcw,
		Settings2,
		Terminal,
		Timer,
		TriangleAlert,
		X,
		Zap,
	} from "@lucide/svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import type { TriggerKind } from "#lib/bots.ts";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { normalizeLook } from "#lib/bots/look.ts";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";
	import { groupTasks, firstUnreadIndex, roleTag } from "#lib/bots/botsLogic.ts";
	import { msgIn, msgOut, embedIn, optionIn } from "#lib/bots/motion.ts";
	import BotTyping from "#lib/bots/BotTyping.svelte";
	import { projectForShip } from "#lib/bots/projectRef.ts";
	import { workingLabel } from "#lib/bots/stepPhrase.ts";
	import TaskWork from "#lib/bots/TaskWork.svelte";
	import { replyParts } from "#lib/bots/replyParts.ts";
	import {
		getQuestion,
		optionAnswer,
		nextStep as setupNextStep,
		roleKeyToText,
		approvalKeyToMode,
		summaryText,
		typingDelay,
		RESTART_STEP,
		type SetupStep,
		type SummaryState,
		type SummaryPart,
	} from "#lib/bots/crewSetup.ts";
	import { githubCrew } from "#lib/bots/githubCrew.svelte.ts";
	import BotSettingsDialog from "#lib/bots/BotSettingsDialog.svelte";
	import { describeTrigger } from "#lib/bots/automations.ts";

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
	let settingsOpen = $state(false);
	let now = $state(Date.now());

	// thread
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
	let composerFocused = $state(false);

	// ─── action card state ────────────────────────────────────────────────────
	let actionInputs = $state<Record<string, string>>({}); // taskId → current input
	let actionSecrets = $state<Record<string, string>>({}); // taskId → secret value (never logged)
	let actionSubmitting = $state<Set<string>>(new Set()); // taskIds currently submitting

	// ─── derived ──────────────────────────────────────────────────────────────

	const bot = $derived(bots.selectedBot);
	const tasks = $derived(bots.tasks);
	const hostOnline = $derived(bots.selected ? bots.hostOk(bots.selected.hostKey) : true);
	const serverName = $derived(bots.selected?.hostName ?? "");
	/** Show actual hostname for local; server name label for remote. */
	const displayServerName = $derived(bots.selected?.hostKey ? serverName : localHostname || serverName);
	const queuedCount = $derived(tasks.filter((t) => t.state === "queued").length);
	/** True when the most recent task has an unanswered ask card. */
	const hasPendingAsk = $derived(
		tasks.length > 0 && parseAsk(tasks[0].result) !== null && !approvedReqs.has(tasks[0].id),
	);
	const faceState = $derived<FaceState>(
		bot?.state === "needs_you" ? "needs_you" : bot?.state === "working" ? "working" : "idle",
	);

	const orderedTasks = $derived([...tasks].reverse()); // oldest first
	const groupedTasks = $derived(groupTasks(orderedTasks));
	const lastSeenTs = $derived(bots.selectedLastSeen);
	const unreadIdx = $derived(firstUnreadIndex(orderedTasks, lastSeenTs));

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
		ws.doc.worktrees.map((w) => ({ id: w.id, name: w.name, path: w.path, branch: w.branch ?? "" })),
	);

	// ─── setup conversation ───────────────────────────────────────────────────

	interface SetupMessage {
		kind: "bot" | "user";
		text: string;
		parts?: SummaryPart[];
	}

	/** Messages already rendered (so resume shows them without typing delay). */
	let setupMessages = $state<SetupMessage[]>([]);
	/** Whether the bot is "typing" its next question. */
	let setupTyping = $state(false);
	/** The live question currently shown to the user (null while typing). */
	let setupCurrentQuestion = $state<ReturnType<typeof getQuestion> | null>(null);
	/** Free-text answer the user is composing during setup. */
	let setupInput = $state("");
	/** Accumulated answers; used to build the summary. */
	let setupState = $state<{
		name: string;
		roleKey: string;
		roleText: string;
		project: string;
		approvalKey: string;
	}>({ name: "", roleKey: "", roleText: "", project: "", approvalKey: "A" });
	/** Whether a bot_update call is in flight during setup. */
	let setupSaving = $state(false);
	/** The active setup step derived from the bot record. */
	const setupStep = $derived(bot?.setup_step as SetupStep | undefined);
	/** IDE project names for the project question chips. */
	const ideProjectNames = $derived(ideProjects.map((p) => p.name));

	/**
	 * (Re)initialise the setup conversation when a bot with setup_step is selected,
	 * or when setup_step changes. Replays already-answered steps without delay,
	 * then presents the current question with the typing indicator.
	 */
	$effect(() => {
		const step = setupStep;
		const b = bot;
		if (!step || !b) return;

		// Pre-fill collected answers from the existing bot record so resuming works.
		const resumed: typeof setupState = {
			name: b.name !== "New crewmate" ? b.name : "",
			roleKey: "",
			roleText: b.role ?? "",
			project: b.project?.name ?? b.project?.path ?? b.project?.url ?? "",
			approvalKey: b.approval === "auto" ? "C" : "A",
		};
		setupState = resumed;

		// Build the replay messages: all steps BEFORE the current one are already answered.
		const { STEPS } = (() => {
			const s = ["name", "role", "project", "approval", "summary"] as const;
			return { STEPS: s };
		})();
		const currentIdx = STEPS.indexOf(step as (typeof STEPS)[number]);
		const replay: SetupMessage[] = [];
		for (let i = 0; i < currentIdx; i++) {
			const s = STEPS[i] as SetupStep;
			const q = getQuestion(s, { ideProjects: ideProjectNames });
			replay.push({ kind: "bot", text: q.text });
			let answer = "";
			if (s === "name") answer = resumed.name || b.name;
			// Only what was really saved: an empty role used to replay as the user saying "Something else".
			else if (s === "role") answer = resumed.roleText;
			else if (s === "project") answer = resumed.project || "Skip";
			else if (s === "approval")
				answer = getQuestion("approval").options?.find((o) => o.key === resumed.approvalKey)?.title ?? "";
			if (answer) replay.push({ kind: "user", text: answer });
		}
		setupMessages = replay;
		setupCurrentQuestion = null;
		setupTyping = false;

		// Now show typing indicator, then the current question.
		setupTyping = true;
		const delay = replay.length === 0 ? 600 : typingDelay();
		const tid = setTimeout(() => {
			setupTyping = false;
			if (step === "summary") {
				setupCurrentQuestion = {
					...getQuestion("summary"),
					...(() => {
						const parts = summaryText({
							name: resumed.name || b.name,
							roleText: resumed.roleText || "do tasks",
							project: resumed.project || undefined,
							approvalKey: resumed.approvalKey,
						} satisfies SummaryState);
						return { parts, text: parts.map((p) => p.text).join("") };
					})(),
				};
			} else {
				setupCurrentQuestion = getQuestion(step as SetupStep, { ideProjects: ideProjectNames });
			}
		}, delay);
		return () => clearTimeout(tid);
	});

	async function setupAnswer(key?: string, text?: string) {
		const step = setupStep;
		const b = bot;
		if (!step || !b || !bots.selected || setupSaving) return;

		const option = key != null ? setupCurrentQuestion?.options?.find((o) => o.key === key) : undefined;
		// "Something else" asks for text: put the c‍ursor in the box and wait, never save the option's own title.
		if (option && "write" in optionAnswer(option)) {
			document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Setup answer"]')?.focus();
			return;
		}
		const answerText = option ? option.title : (text ?? "").trim();
		if (!answerText && step !== "project") return;

		setupMessages = [
			...setupMessages,
			{ kind: "bot", text: setupCurrentQuestion?.text ?? "", parts: setupCurrentQuestion?.parts },
		];
		setupCurrentQuestion = null;

		// Map answer to bot fields
		const patch: Record<string, unknown> = {};
		let nextStep_: SetupStep | "done" = "done";
		const newState = { ...setupState };

		if (step === "name") {
			const name = answerText.trim() || "New crewmate";
			newState.name = name;
			patch.name = name;
			nextStep_ = setupNextStep("name");
		} else if (step === "role") {
			if (!key) {
				// typed by the user
				newState.roleText = answerText;
			} else {
				newState.roleKey = key;
				newState.roleText = roleKeyToText(key);
			}
			patch.role = newState.roleText;
			nextStep_ = setupNextStep("role");
		} else if (step === "project") {
			const projectName = answerText.trim();
			newState.project = projectName;
			if (projectName) {
				// One of the user's projects: resolve its git origin BEFORE saving (it used to save the Mac path and
				// race a later overwrite, so the ship could keep a path it cannot open).
				const matched = ideProjects.find((p) => p.name === projectName);
				if (matched) {
					const origin = await invoke<{ url: string | null }>("project_git_info", { path: matched.path })
						.then((i) => i.url)
						.catch(() => null);
					const r = projectForShip(matched, origin, bots.selected.hostKey);
					if (!r.ok) {
						toast.error(r.error);
						return;
					}
					patch.project = r.project;
				} else {
					// Free text / git URL
					const isGit = /^(https?|git|ssh):\/\//.test(projectName) || projectName.endsWith(".git");
					patch.project = isGit
						? { kind: "git", url: projectName, name: projectName }
						: { kind: "path", path: projectName, name: projectName };
				}
			}
			nextStep_ = setupNextStep("project");
		} else if (step === "approval") {
			newState.approvalKey = key ?? "A";
			patch.approval = approvalKeyToMode(key ?? "A");
			nextStep_ = setupNextStep("approval");
		} else if (step === "summary") {
			if (key === "B") {
				// Restart: go back to first step
				setupMessages = [];
				patch.setup_step = RESTART_STEP;
				nextStep_ = RESTART_STEP;
			} else {
				// Done
				patch.setup_step = null;
				nextStep_ = "done";
			}
		}

		setupState = newState;
		if (step !== "summary" || key === "A") {
			setupMessages = [...setupMessages, { kind: "user", text: answerText }];
		}

		// Persist to bot record
		setupSaving = true;
		try {
			const updatePatch: Parameters<typeof bots.updateBot>[2] = {
				...("name" in patch ? { name: patch.name as string } : {}),
				...("role" in patch ? { role: patch.role as string } : {}),
				...("approval" in patch ? { approval: patch.approval as string } : {}),
				...("project" in patch
					? { project: patch.project as Parameters<typeof bots.updateBot>[2]["project"] }
					: {}),
				...(step !== "summary" || key !== "B"
					? { setup_step: nextStep_ === "done" ? null : (nextStep_ as string) }
					: { setup_step: RESTART_STEP }),
			};
			await bots.updateBot(bots.selected.hostKey, b.id, updatePatch);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			setupSaving = false;
		}

		setupInput = "";

		// If done, the effect above won't re-run (setup_step cleared), so just clear state.
		if (nextStep_ === "done") {
			setupMessages = [];
			setupCurrentQuestion = null;
		}
	}

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

	onMount(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// Auto-scroll to bottom when new tasks arrive, if already at bottom
	$effect(() => {
		const _ = orderedTasks.length;
		// A Board card asked for one task: that wins over sticking to the bottom.
		if (bots.focusTask) return;
		if (atBottom && threadEl) {
			setTimeout(() => threadEl?.scrollTo({ top: threadEl.scrollHeight }), 0);
		}
	});

	// Opened from a Board card: scroll to that task once it is rendered, and flash it so the eye finds it.
	let flashTask = $state<string | null>(null);
	$effect(() => {
		const id = bots.focusTask;
		if (!id || !threadEl) return;
		if (!orderedTasks.some((t) => t.id === id)) {
			// Loaded and still not there (older than the chat's window): give up, or the chat would stop following new
			// messages, since the stick-to-bottom effect waits while a focus is pending.
			if (!bots.tasksLoading && orderedTasks.length > 0) bots.focusTask = null;
			return;
		}
		// Not "at the bottom" any more: clearing focusTask re-runs the stick-to-bottom effect above, which put the
		// thread straight back at the end.
		atBottom = false;
		requestAnimationFrame(() => {
			threadEl?.querySelector(`[data-task-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "center" });
			flashTask = id;
			bots.focusTask = null;
			setTimeout(() => flashTask === id && (flashTask = null), 1600);
		});
	});

	// ─── helpers ──────────────────────────────────────────────────────────────

	function fmtTime(tsMs: number): string {
		return new Date(tsMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
	}

	function fmtElapsed(ms: number): string {
		const s = Math.floor(ms / 1000);
		if (s < 60) return `${s}s`;
		const m = Math.floor(s / 60);
		if (m < 60) return `${m}m`;
		const h = Math.floor(m / 60);
		return h < 24 ? `${h}h ${m % 60}m` : `${Math.floor(h / 24)}d ${h % 24}h`;
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

	/** Fit the composer to its text on every change of `prompt`, typed or set by code. Growing only on `input`
	 *  left the box two lines tall after a send cleared it, and after a pick or "edit" filled it. */
	function autoGrow(el: HTMLTextAreaElement) {
		void [prompt, setupInput];
		el.style.height = "auto";
		const maxH = parseInt(getComputedStyle(el).getPropertyValue("--composer-max-h") || "192");
		el.style.height = Math.min(el.scrollHeight, maxH) + "px";
	}

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
	// ─── trigger helpers (still used by thread trigger_event display) ─────────

	const TRIGGER_ICONS: Record<TriggerKind, Component> = {
		schedule: Clock,
		interval: Timer,
		webhook: Link,
		github: GitPullRequest,
		file: FileText,
		bot: BotIcon,
		service: Settings2,
	};

	function triggerIconComponent(kind: TriggerKind | string): Component {
		return (TRIGGER_ICONS as Record<string, Component>)[kind] ?? Zap;
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

	// ─── action card helpers ──────────────────────────────────────────────────

	async function answerAction(taskId: string, botId: string, value: string): Promise<void> {
		const hostKey = bots.selected?.hostKey ?? "";
		const hostArgs = hostKey ? { host: hostKey } : {};
		actionSubmitting = new Set([...actionSubmitting, taskId]);
		try {
			await hostCall("bot_action_answer", { ...hostArgs, id: botId, task: taskId, value });
			const i = { ...actionInputs };
			delete i[taskId];
			actionInputs = i;
			const s = { ...actionSecrets };
			delete s[taskId];
			actionSecrets = s;
		} finally {
			const sub = new Set(actionSubmitting);
			sub.delete(taskId);
			actionSubmitting = sub;
		}
	}

	async function cancelAction(taskId: string, botId: string): Promise<void> {
		const hostKey = bots.selected?.hostKey ?? "";
		const hostArgs = hostKey ? { host: hostKey } : {};
		await hostCall("bot_cancel", { ...hostArgs, id: botId, task: taskId });
	}

	// Auto-resume: when GitHub connects while a waiting_action connect_github task is parked
	$effect(() => {
		const hostKey = bots.selected?.hostKey ?? "";
		if (!hostKey) return;
		const status = githubCrew.statuses.get(hostKey);
		if (!status?.login) return;
		const needsResume = tasks.some(
			(t) => t.state === "waiting_action" && t.action?.kind === "connect_github",
		);
		if (needsResume) {
			const ha = hostKey ? { host: hostKey } : {};
			void hostCall("bot_github_connect_resume", ha);
		}
	});
</script>

<div class="bot-view">
	{#if !bot}
		<div class="empty">Select a bot from the sidebar.</div>
	{:else}
		<!-- Header -->
		<div class="header">
			<!-- Who you are talking to and what it is doing; clicking opens its settings. -->
			<button class="header-pill" onclick={() => (settingsOpen = true)} aria-label="Bot settings: {bot.name}">
				<BotFace seed={bot.id} look={bot.look} state={faceState} size={30} />
				<span class="pill-text">
					<span class="pill-name">{bot.name}</span>
					<span class="pill-status" data-state={bot.state}>
						<span class="pill-dot" aria-hidden="true"></span>
						{#if bot.state === "working"}
							<!-- Only with a real start: a task waiting for the user has none, and `now - 0` showed the minutes
							     since 1970 ("29886587m"). -->
							{@const runningStart = bot.running_task
								? bots.tasks.find((t) => t.id === bot.running_task!.id)?.started_at
								: undefined}
							Working{runningStart ? ` · ${fmtElapsed(now - runningStart)}` : ""}
						{:else if bot.state === "needs_you"}
							Needs you
						{:else}
							{agentLabel}
						{/if}
					</span>
				</span>
				<ChevronDown size={14} class="pill-chevron" aria-hidden="true" />
			</button>
			<!-- What used to hide behind "⋯" (whose only item when idle was the settings the name already opens): shown
			     only while there is a task to watch or stop. -->
			{#if bot.running_task}
				<div class="header-right">
					{#if bot.running_task.pane}
						<button class="head-btn" onclick={() => void openTerminal()} title="Watch in a terminal">
							<Terminal size={14} aria-hidden="true" /> Terminal
						</button>
					{/if}
					<button
						class="head-btn danger"
						onclick={() => void cancelRunningTask()}
						title="Stop the current task"
					>
						<X size={14} aria-hidden="true" /> Stop
					</button>
				</div>
			{/if}
		</div>

		<!-- Offline banner -->
		{#if !hostOnline}
			<div class="offline-banner" role="alert">
				<TriangleAlert size={14} aria-hidden="true" />
				{displayServerName || "Server"} is offline
			</div>
		{/if}

		<!-- Task thread -->
		<!-- Task thread or setup conversation -->
		<!-- --crew: this crewmate's own colour, so your bubbles and the composer match the bot you're talking to. -->
		<div class="chat-col" style:--crew={normalizeLook(bot.look, bot.id).color}>
			{#if setupStep}
				<!-- ── Setup conversation ──────────────────────────────────── -->
				<div class="thread setup-thread" aria-label="Setup conversation" aria-live="polite">
					{#each setupMessages as msg, i (i)}
						{#if msg.kind === "bot"}
							<div class="msg-in" in:msgIn>
								<div class="msg-in-body">
									<div class="bubble in">
										{#if msg.parts}{#each msg.parts as p, pi (pi)}{#if p.bold}<strong>{p.text}</strong
													>{:else}{p.text}{/if}{/each}{:else}{msg.text}{/if}
									</div>
								</div>
							</div>
						{:else}
							<div class="msg-out" in:msgOut>
								<div class="bubble out">{msg.text}</div>
							</div>
						{/if}
					{/each}

					{#if setupTyping}
						<div class="msg-in setup-typing" in:msgIn>
							<BotTyping
								seed={bot.id}
								look={bot.look as import("#lib/bots/look.ts").BotLook | undefined}
								label="Thinking"
							/>
						</div>
					{/if}

					{#if setupCurrentQuestion}
						{@const q = setupCurrentQuestion}
						<div class="msg-in" in:msgIn>
							<div class="msg-in-body">
								<div class="bubble in">
									{#if q.parts}{#each q.parts as p, pi (pi)}{#if p.bold}<strong>{p.text}</strong
												>{:else}{p.text}{/if}{/each}{:else}{q.text}{/if}
								</div>
								{#if q.options}
									<div class="ask-card setup-ask">
										{#each q.options as opt, oi (opt.key)}
											<button
												class="ask-option"
												in:optionIn={{ index: oi }}
												onclick={() => void setupAnswer(opt.key)}
												disabled={setupSaving}
											>
												<span class="ask-key">{opt.key}</span>
												<div class="ask-opt-body">
													<span class="ask-opt-title">{opt.title}</span>
													{#if opt.subtitle}<span class="ask-opt-sub">{opt.subtitle}</span>{/if}
												</div>
											</button>
										{/each}
									</div>
								{/if}
								{#if q.nameChips?.length}
									<div class="name-chips">
										{#each q.nameChips as chip (chip)}
											<button
												class="name-chip"
												onclick={() => void setupAnswer(undefined, chip)}
												disabled={setupSaving}>{chip}</button
											>
										{/each}
									</div>
								{/if}
							</div>
						</div>
					{/if}
				</div>

				{#if setupCurrentQuestion?.allowText}
					<div class="composer">
						<div class="pill" class:focused={composerFocused} role="group">
							<textarea
								class="pill-input"
								bind:value={setupInput}
								rows={1}
								placeholder={setupStep === "name"
									? "Type a name…"
									: setupStep === "project"
										? "Paste a git URL or server path, or skip"
										: "Type your answer…"}
								aria-label="Setup answer"
								onkeydown={(e) => {
									if (e.key === "Enter" && !e.shiftKey && setupInput.trim()) {
										e.preventDefault();
										void setupAnswer(undefined, setupInput);
									}
								}}
								{@attach autoGrow}
								onfocus={() => (composerFocused = true)}
								onblur={() => (composerFocused = false)}></textarea>
							<button
								class="pill-send"
								class:active={!!setupInput.trim()}
								aria-label="Send"
								disabled={!setupInput.trim() || setupSaving}
								onclick={() => void setupAnswer(undefined, setupInput)}><ArrowUp size={15} /></button
							>
						</div>
					</div>
				{/if}
			{:else}
				<!-- Normal task thread -->
				<div class="thread" bind:this={threadEl} onscroll={onThreadScroll} aria-label="Task history">
					{#if bots.tasksLoading && tasks.length === 0}
						<div class="loading">Loading…</div>
					{:else if tasks.length === 0}
						<div class="empty-thread">No tasks yet. Send one below.</div>
					{:else}
						{#each orderedTasks as t, i (t.id)}
							{#if i === unreadIdx}
								<div class="unread-divider" role="separator">
									<span class="unread-label">NEW</span>
								</div>
							{/if}
							{#if groupedTasks[i]?.groupStart && i > 0}
								{@const prev = orderedTasks[i - 1]}
								{#if new Date(t.ts).toDateString() !== new Date(prev.ts).toDateString()}
									<div class="msg-ts">{fmtDate(t.ts)}</div>
								{/if}
							{/if}
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="msg-row"
								data-task-id={t.id}
								class:flash={flashTask === t.id}
								class:group-start={groupedTasks[i]?.groupStart}
								onmouseenter={() => (hoveredTask = t.id)}
								onmouseleave={() => (hoveredTask = null)}
							>
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
									{#if t.preparing}
										<!-- A first task on a big repo waited over a minute as a bare "Queued". -->
										<div class="msg-meta muted preparing">
											<span class="prep-dot" aria-hidden="true"></span>Downloading {t.preparing}…
										</div>
									{:else if t.state === "queued"}
										<div class="msg-meta muted">Queued</div>
									{/if}
								</div>

								<!-- A choice/answer the user gave mid-task stays in the conversation: the crewmate's question, then
								     the user's pick as their own message. -->
								{#if t.answer && t.state !== "waiting_action" && t.action?.kind !== "add_trigger"}
									{#if t.action?.kind === "choose"}
										<div class="msg-in" in:msgIn>
											<div class="msg-in-body">
												<div class="bubble in">{t.action.question}</div>
											</div>
										</div>
									{:else if t.action?.kind === "input"}
										<div class="msg-in" in:msgIn>
											<div class="msg-in-body">
												<div class="bubble in">{t.action.label}</div>
											</div>
										</div>
									{/if}
									<div class="msg-out" in:msgOut>
										<div class="bubble out ca-selectable">{t.answer}</div>
									</div>
								{/if}

								{#if t.state === "running"}
									{@const elapsedMs = t.started_at ? now - t.started_at : 0}
									<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
										<div class="msg-in-body">
											<!-- Same "dots gather, crewmate hops" as the onboarding. The line says what it is doing: its
											     latest step while fresh, then "Thinking", "Planning"… changing every few seconds. -->
											<BotTyping
												seed={bot.id}
												look={bot.look as import("#lib/bots/look.ts").BotLook | undefined}
												label={workingLabel(t.steps?.at(-1)?.text, t.steps?.at(-1)?.ts, now)}
												detail={fmtElapsed(elapsedMs)}
											/>
											<!-- While it works, what it is doing stays on screen, like the onboarding story: a section per
											     title the agent set itself, its steps ticking off, finished sections folded. -->
											<TaskWork steps={t.steps ?? []} title={t.title} running />
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
								{:else if t.state === "done"}
									{@const raw = t.result ?? "Done"}
									{@const collapsed = shouldCollapse(raw) && !expandedResults.has(t.id)}
									{@const shownFull = expandedResults.has(t.id + ":full")}
									<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
										<div class="msg-in-body">
											<!-- The work before the reply, folded per section; it stays in the conversation. -->
											<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
											{#if t.action?.kind === "add_trigger" && t.answer}
												<div class="task-note">
													{#if t.answer === "add"}
														<Check size={12} aria-hidden="true" /> Automation added
													{:else}
														Not added
													{/if}
												</div>
											{/if}
											{#each [parseAsk(t.result)] as ask}
												{#if ask && !expandedResults.has(t.id + ":dismissed")}
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
																	if (
																		e.key === "Enter" &&
																		(e.currentTarget as HTMLInputElement).value.trim()
																	) {
																		prompt = (e.currentTarget as HTMLInputElement).value.trim();
																		void send();
																	}
																}}
															/>
														{/if}
													</div>
												{:else if !expandedResults.has(t.id + ":dismissed")}
													{#if t.summarized && !shownFull}
														<div class="md-result ca-selectable">{@html resultHtml(raw)}</div>
														<button
															class="show-more"
															onclick={() => {
																const s = new Set(expandedResults);
																s.add(t.id + ":full");
																expandedResults = s;
															}}>See full reply</button
														>
													{:else if t.summarized && shownFull}
														<div class="md-result ca-selectable">{@html resultHtml(t.reply_full ?? raw)}</div>
														<button
															class="show-more"
															onclick={() => {
																const s = new Set(expandedResults);
																s.delete(t.id + ":full");
																expandedResults = s;
															}}>Show summary</button
														>
													{:else}
														{@const parts = replyParts(raw)}
														{#if parts.length > 1}
															<!-- A long reply as a few chat messages, one per section, instead of one long document. -->
															{#each parts as part, pi (pi)}
																<div class="md-result ca-selectable reply-part" in:msgIn={{ delay: pi * 90 }}>
																	{@html resultHtml(part)}
																</div>
															{/each}
														{:else}
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
														{/if}
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
													{#if t.usage.cost_usd}<span>${t.usage.cost_usd.toFixed(4)}</span>{/if}
													{#if t.usage.duration_ms}<span>{fmtElapsed(t.usage.duration_ms)}</span>{/if}
													{#if t.usage.model}<span class="usage-model">{t.usage.model}</span>{/if}
												</div>
											{/if}
										</div>
									</div>
								{:else if t.state === "failed"}
									<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
										<div class="msg-in-body">
											<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
											<div class="bubble in error ca-selectable">{t.error ?? "Failed"}</div>
										</div>
									</div>
								{:else if t.state === "cancelled"}
									<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
										<div class="msg-in-body">
											<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
											<div class="task-note"><X size={12} aria-hidden="true" /> Cancelled</div>
										</div>
									</div>
								{:else if t.state === "waiting_action" && t.action}
									{@const action = t.action}
									{@const hostKey = bots.selected?.hostKey ?? ""}
									{@const flow = githubCrew.flows.get(hostKey)}
									{@const polling = githubCrew.isPolling(hostKey)}
									{@const status = githubCrew.statuses.get(hostKey)}
									<div class="msg-in" class:group-start={groupedTasks[i]?.groupStart} in:msgIn>
										<div class="msg-in-body">
											<div class="action-card" role="alert">
												{#if action.kind === "connect_github"}
													<div class="action-title">Connect GitHub to continue</div>
													<div class="action-reason">{action.reason}</div>
													{#if status?.login}
														<div class="action-connected">Connected as @{status.login}</div>
													{:else if flow}
														<div class="gh-flow-block">
															<p class="gh-flow-label">
																Enter this code at <a
																	href={flow.verification_uri}
																	target="_blank"
																	rel="noreferrer"
																	class="text-link">{flow.verification_uri}</a
																>
															</p>
															<div class="gh-code-row">
																<code class="gh-code">{flow.user_code}</code>
																<button
																	class="gh-copy-btn"
																	onclick={() => {
																		void navigator.clipboard.writeText(flow.user_code);
																	}}
																	aria-label="Copy code">Copy</button
																>
															</div>
															{#if polling}
																<p class="gh-flow-waiting" role="status" aria-live="polite">
																	Waiting for authorization…
																</p>
															{/if}
															<Button
																variant="ghost"
																size="sm"
																onclick={() => githubCrew.stopPolling(hostKey)}>Cancel</Button
															>
														</div>
													{:else}
														<div class="action-btns">
															<Button size="sm" onclick={() => void githubCrew.startLogin(hostKey)}
																>Connect</Button
															>
															<Button
																variant="ghost"
																size="sm"
																onclick={() => void cancelAction(t.id, bot.id)}>Cancel task</Button
															>
														</div>
													{/if}
												{:else if action.kind === "provide_secret"}
													<div class="action-title">{action.label}</div>
													<div class="action-reason">{action.reason}</div>
													<input
														type="password"
														class="action-input"
														placeholder="Enter value"
														value={actionSecrets[t.id] ?? ""}
														oninput={(e) => {
															actionSecrets = {
																...actionSecrets,
																[t.id]: (e.currentTarget as HTMLInputElement).value,
															};
														}}
														aria-label={action.label}
													/>
													<div class="action-btns">
														<Button
															size="sm"
															disabled={!actionSecrets[t.id]?.trim() || actionSubmitting.has(t.id)}
															onclick={() => {
																const v = actionSecrets[t.id]?.trim() ?? "";
																if (v) void answerAction(t.id, bot.id, v);
															}}>Submit</Button
														>
														<Button variant="ghost" size="sm" onclick={() => void cancelAction(t.id, bot.id)}
															>Cancel</Button
														>
													</div>
												{:else if action.kind === "input"}
													<div class="action-title">{action.label}</div>
													{#if action.reason}
														<div class="action-reason">{action.reason}</div>
													{/if}
													<input
														type="text"
														class="action-input"
														placeholder={action.placeholder ?? ""}
														value={actionInputs[t.id] ?? ""}
														oninput={(e) => {
															actionInputs = {
																...actionInputs,
																[t.id]: (e.currentTarget as HTMLInputElement).value,
															};
														}}
														onkeydown={(e) => {
															if (e.key === "Enter") {
																const v = actionInputs[t.id]?.trim() ?? "";
																if (v) void answerAction(t.id, bot.id, v);
															}
														}}
														aria-label={action.label}
													/>
													<div class="action-btns">
														<Button
															size="sm"
															disabled={!actionInputs[t.id]?.trim() || actionSubmitting.has(t.id)}
															onclick={() => {
																const v = actionInputs[t.id]?.trim() ?? "";
																if (v) void answerAction(t.id, bot.id, v);
															}}>Submit</Button
														>
														<Button variant="ghost" size="sm" onclick={() => void cancelAction(t.id, bot.id)}
															>Cancel</Button
														>
													</div>
												{:else if action.kind === "choose"}
													<div class="action-title">{action.question}</div>
													<div class="action-options">
														{#each action.options as opt, oi (opt)}
															<button
																class="ask-option"
																disabled={actionSubmitting.has(t.id)}
																onclick={() => void answerAction(t.id, bot.id, opt)}
															>
																<span class="ask-key">{String.fromCharCode(65 + oi)}</span>
																<div class="ask-opt-body">
																	<span class="ask-opt-title">{opt}</span>
																</div>
															</button>
														{/each}
													</div>
												{:else if action.kind === "sign_in_engine"}
													<div class="action-title">Sign in to {action.engine}</div>
													<div class="action-reason">
														Sign in to {action.engine} in Settings, then click Done.
													</div>
													<div class="action-btns">
														<Button
															size="sm"
															disabled={actionSubmitting.has(t.id)}
															onclick={() => void answerAction(t.id, bot.id, "done")}>Done</Button
														>
														<Button variant="ghost" size="sm" onclick={() => void cancelAction(t.id, bot.id)}
															>Cancel</Button
														>
													</div>
												{:else if action.kind === "add_trigger"}
													<div class="action-title">{bot.name} wants to add an automation</div>
													<div class="action-reason">{action.reason}</div>
													<div class="action-detail">
														{describeTrigger(action.trigger)}
													</div>
													{#if action.prompt}
														<div class="action-reason">{action.prompt}</div>
													{/if}
													<div class="action-btns">
														<Button
															size="sm"
															disabled={actionSubmitting.has(t.id)}
															onclick={() => void answerAction(t.id, bot.id, "add")}>Add</Button
														>
														<Button
															variant="ghost"
															size="sm"
															disabled={actionSubmitting.has(t.id)}
															onclick={() => void answerAction(t.id, bot.id, "skip")}>Not now</Button
														>
													</div>
												{:else}
													<div class="action-title">
														{(action as { label?: string }).label ?? "Response needed"}
													</div>
													<input
														type="text"
														class="action-input"
														value={actionInputs[t.id] ?? ""}
														oninput={(e) => {
															actionInputs = {
																...actionInputs,
																[t.id]: (e.currentTarget as HTMLInputElement).value,
															};
														}}
														aria-label="Response"
													/>
													<div class="action-btns">
														<Button
															size="sm"
															disabled={!actionInputs[t.id]?.trim() || actionSubmitting.has(t.id)}
															onclick={() => {
																const v = actionInputs[t.id]?.trim() ?? "";
																if (v) void answerAction(t.id, bot.id, v);
															}}>Submit</Button
														>
														<Button variant="ghost" size="sm" onclick={() => void cancelAction(t.id, bot.id)}
															>Cancel</Button
														>
													</div>
												{/if}
											</div>
										</div>
									</div>
								{/if}

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

				<div class="composer">
					<div class="pill" class:focused={composerFocused} role="group">
						<button class="pill-add" aria-label="Attach" onclick={noop}><Plus size={16} /></button>
						<textarea
							class="pill-input"
							bind:value={prompt}
							rows={1}
							placeholder={hasPendingAsk
								? "Type your answer or talk about something else"
								: `Message ${bot.name ?? "bot"}…`}
							aria-label="Task prompt"
							onkeydown={(e) => {
								if (e.key === "Enter" && !e.shiftKey) {
									e.preventDefault();
									void send();
								}
							}}
							{@attach autoGrow}
							onfocus={() => (composerFocused = true)}
							onblur={() => (composerFocused = false)}></textarea>
						<button
							class="pill-send"
							class:active={!!prompt.trim()}
							aria-label="Send"
							disabled={!prompt.trim() || sending}
							onclick={send}><ArrowUp size={15} /></button
						>
					</div>
					{#if queuedCount > 0}
						<div class="queue-hint">{queuedCount} task{queuedCount === 1 ? "" : "s"} in queue</div>
					{/if}
				</div>
			{/if}
		</div>

		<BotSettingsDialog bind:open={settingsOpen} />
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
		justify-content: center;
		padding: 0 12px;
		height: 44px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		position: relative;
		flex-shrink: 0;
	}
	.header-pill {
		display: flex;
		align-items: center;
		gap: 10px;
		background: none;
		border: none;
		border-radius: 12px;
		padding: 4px 10px 4px 4px;
		color: var(--ca-text-primary);
		cursor: default;
		transition: background 120ms;
		text-align: left;
	}
	.header-pill:hover,
	.header-pill:focus-visible {
		background: var(--ca-vibrancy-fill);
		outline: none;
	}
	.pill-text {
		display: flex;
		flex-direction: column;
		line-height: 1.2;
		min-width: 0;
	}
	.pill-name {
		font-size: 14px;
		font-weight: 600;
	}
	.pill-status {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.pill-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-tertiary, var(--ca-text-secondary));
		opacity: 0.6;
	}
	.pill-status[data-state="working"] .pill-dot {
		background: var(--ca-semantic-success);
		opacity: 1;
		animation: pill-pulse 1.6s ease-in-out infinite;
	}
	.pill-status[data-state="needs_you"] {
		color: var(--ca-semantic-warning);
	}
	.pill-status[data-state="needs_you"] .pill-dot {
		background: var(--ca-semantic-warning);
		opacity: 1;
	}
	@keyframes pill-pulse {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pill-status[data-state="working"] .pill-dot {
			animation: none;
		}
	}
	.header-pill :global(.pill-chevron) {
		color: var(--ca-text-secondary);
		opacity: 0;
		transition: opacity 120ms;
	}
	.header-pill:hover :global(.pill-chevron),
	.header-pill:focus-visible :global(.pill-chevron) {
		opacity: 1;
	}
	.header-right {
		position: absolute;
		right: 12px;
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		gap: 6px;
	}
	.head-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font: inherit;
		font-size: 12px;
		padding: 5px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.head-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.head-btn.danger:hover {
		color: var(--ca-semantic-error);
		border-color: color-mix(in oklab, var(--ca-semantic-error) 40%, transparent);
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
	.chat-col {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		/* Like Grok: the thread uses the whole pane on a normal window and only centres (capped) once the pane
		   is much wider than a comfortable reading line, i.e. full screen. */
		max-width: 1120px;
		width: 100%;
		margin: 0 auto;
		position: relative;
	}
	/* A chat never scrolls sideways: with only overflow-y set, overflow-x computed to auto and one wide bubble (a long
	   unbroken path in an error, a code line) dragged the whole conversation left. Rows may shrink, long words break,
	   code blocks scroll inside themselves. */
	.thread {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 20px clamp(24px, 5%, 64px) 8px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.thread > :global(*) {
		min-width: 0;
	}
	.thread :global(.msg-in-body) {
		min-width: 0;
		max-width: 100%;
	}
	.thread :global(.bubble),
	.thread :global(.md-result) {
		overflow-wrap: anywhere;
	}
	.thread :global(.md-result pre) {
		max-width: 100%;
		overflow-x: auto;
	}
	.empty-thread {
		color: var(--ca-text-secondary);
		font-size: 14px;
		text-align: center;
		padding: 40px 0;
	}
	.loading {
		color: var(--ca-text-secondary);
		font-size: 14px;
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
	/* Turns: a clear gap whenever the speaker changes (you ↔ the crewmate), tight spacing between consecutive bubbles of
	   the same speaker. Before, both sides used 2px and the reply touched the question it answered. */
	.msg-out {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		margin-top: 20px;
		margin-bottom: 14px;
		padding: 1px 16px;
	}
	.msg-out + .msg-out {
		margin-top: -10px;
	}
	.msg-in {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 1px 16px;
		margin-top: 4px;
		position: relative;
	}
	/* The body takes the row and the bubble hugs its text: shrink-wrapped, the body's width depended on its own
	   content and a bubble's max-width (72%) of it could fall under one word ("Canc/elled"). */
	.msg-in-body {
		flex: 1;
		min-width: 0;
	}
	.msg-in-body > .bubble {
		width: fit-content;
	}
	/* A task's end that is not a reply (cancelled): a quiet line, not a bubble. */
	.task-note {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 4px;
	}
	.msg-in.group-start {
		margin-top: 8px;
		padding-top: 1px;
	}
	/* msg-row wraps user + bot bubbles for a single task */
	.msg-row {
		position: relative;
		display: flex;
		flex-direction: column;
	}
	.msg-row.flash {
		animation: task-flash 1.6s ease-out;
		border-radius: 12px;
	}
	@keyframes task-flash {
		0%,
		30% {
			background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
		}
		100% {
			background: transparent;
		}
	}
	.bubble {
		max-width: 72%;
		padding: 12px 16px;
		border-radius: 18px;
		font-size: 15px;
		line-height: 1.45;
		word-break: break-word;
		white-space: pre-wrap;
	}
	.bubble.out {
		background: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 24%, var(--ca-surface-base));
		border-bottom-right-radius: 4px;
	}
	.bubble.in {
		background: var(--ca-surface-sunken);
		border-bottom-left-radius: 4px;
		color: var(--ca-text-primary);
	}
	.bubble.in.error {
		color: var(--ca-semantic-error);
	}
	.msg-meta {
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin-top: 2px;
		padding: 0 4px;
	}
	.msg-meta.preparing {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.prep-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-brand-main);
		animation: prep-pulse 1.2s ease-in-out infinite;
	}
	@keyframes prep-pulse {
		50% {
			opacity: 0.3;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.prep-dot {
			animation: none;
		}
	}

	/* ─── markdown result ────────────────────────────────────── */
	/* Consecutive messages of one reply sit close together, like a burst of chat messages. */
	.reply-part + .reply-part {
		margin-top: 6px;
	}
	/* The bot's reply sits in the same bubble as every other message (loose text on the page read as a document,
	   not a conversation). One size throughout: the markdown renderer's paragraphs came out smaller than lists. */
	.md-result {
		font-size: 15px;
		line-height: 1.55;
		color: var(--ca-text-primary);
		word-break: break-word;
		max-width: 72%;
		width: fit-content;
		padding: 12px 16px;
		border-radius: 18px;
		border-bottom-left-radius: 4px;
		background: var(--ca-surface-sunken);
	}
	.md-result :global(:first-child) {
		margin-top: 0;
	}
	.md-result :global(:last-child) {
		margin-bottom: 0;
	}
	.md-result :global(p),
	.md-result :global(li) {
		font-size: inherit;
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
		bottom: 130px;
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
		/* Same side inset as the thread, so the pill lines up with the bubbles above it. */
		padding: 8px clamp(24px, 5%, 64px) 20px;
	}
	.pill {
		display: flex;
		align-items: flex-end;
		/* Raised, like Grok's composer: a sunken (darker than the page) pill read as a hole with faded edges. */
		background: color-mix(in oklab, var(--ca-text-primary) 6%, var(--ca-surface-raised, var(--ca-surface)));
		/* Half the one-line height: a pill with one line, a rounded box once the text wraps. 999px kept the pill shape
		   at any height and its round ends cut into the second line. */
		border-radius: 26px;
		border: 1px solid color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
		min-height: 52px;
		padding: 8px 8px 8px 6px;
		gap: 4px;
		transition: border-color 120ms;
		--composer-max-h: 192;
	}
	.pill.focused {
		border-color: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
	}
	.pill-add {
		flex: none;
		width: 36px;
		height: 36px;
		min-height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		border: none;
		border-radius: 50%;
		color: var(--ca-text-secondary);
		cursor: default;
		align-self: flex-end;
		flex-shrink: 0;
	}
	.pill-add:hover {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-primary);
	}
	.pill-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		outline: none;
		font-size: 15px;
		color: var(--ca-text-primary);
		padding: 6px 4px;
		line-height: 1.5;
		resize: none;
		overflow-y: auto;
		min-height: 36px;
		max-height: 192px;
		align-self: flex-end;
		font-family: inherit;
		field-sizing: content;
	}
	/* Without the "+" before it (the setup composer), the text starts at the pill's own edge: inset it so it clears the
	   round end like the "+" does in the chat composer. */
	.pill-input:first-child {
		padding-left: 14px;
	}
	.pill-input::placeholder {
		color: var(--ca-text-secondary);
	}
	.pill-send {
		flex: none;
		width: 36px;
		height: 36px;
		min-height: 36px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 25%, transparent);
		border: none;
		color: var(--ca-text-disabled, #888);
		cursor: default;
		transition:
			background 120ms,
			color 120ms;
		align-self: flex-end;
		flex-shrink: 0;
	}
	.pill-send.active {
		background: var(--crew, var(--ca-brand-main));
		color: var(--ca-surface-base);
	}
	.pill-send:disabled:not(.active) {
		opacity: 0.5;
	}
	.queue-hint {
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: center;
		margin-top: 6px;
	}

	/* ─── ask card ───────────────────────────────────────────── */
	.ask-card {
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 16px;
		overflow: hidden;
		margin: 6px 0;
		max-width: 480px;
		background: color-mix(in oklab, var(--ca-surface-sunken) 60%, transparent);
	}
	.ask-question {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 18px 12px;
		font-size: 15px;
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
		gap: 12px;
		width: 100%;
		padding: 14px 18px;
		min-height: 52px;
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
		width: 26px;
		height: 26px;
		border-radius: 4px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-secondary);
		font-size: 12px;
		font-weight: 600;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		border: 0.5px solid var(--ca-outline-subtle);
	}
	.ask-opt-body {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.ask-opt-title {
		font-size: 15px;
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
		padding: 12px 18px;
		background: none;
		border: none;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 14px;
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

	/* ─── trigger event line (in thread) ────────────────────── */
	.trigger-event {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 16px 3px 28px;
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

	/* ─── setup conversation ─────────────────────────────────── */
	.setup-thread {
		padding-top: 24px;
	}
	.setup-typing {
		padding: 4px 16px;
	}
	.setup-ask {
		margin-top: 8px;
	}
	.name-chips {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 10px;
		padding: 0 2px;
	}
	.name-chip {
		padding: 6px 16px;
		border-radius: 999px;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		font-size: 14px;
		cursor: default;
		transition: background 80ms;
	}
	.name-chip:hover:not(:disabled) {
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-sunken));
		border-color: color-mix(in oklab, var(--ca-brand-main) 40%, transparent);
	}
	.name-chip:disabled {
		opacity: 0.5;
	}

	/* ── Action card (waiting_action state) ─────────────────────────── */
	/* A comfortable fixed width instead of shrink-to-fit: the card used to collapse to its title's width and the
	   "Cancel task" button spilled out of it. */
	.action-card {
		box-sizing: border-box;
		width: min(380px, 100%);
		max-width: 100%;
		padding: 14px 16px;
		border-radius: 18px;
		border-top-left-radius: 4px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, transparent);
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 35%, transparent);
		display: flex;
		flex-direction: column;
		gap: 10px;
		overflow-wrap: anywhere;
	}
	.action-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.action-reason {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.action-connected {
		font-size: 12px;
		color: var(--ca-semantic-success);
	}
	.action-input {
		background: var(--ca-surface-base);
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 6px 10px;
		font-size: 13px;
		color: var(--ca-text-primary);
		outline: none;
		width: 100%;
		box-sizing: border-box;
	}
	.action-input:focus {
		border-color: var(--ca-outline-primary);
	}
	.action-btns {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.action-btns :global(button) {
		white-space: nowrap;
	}
	.action-options {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.gh-flow-block {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.gh-flow-label {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.gh-code-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.gh-code {
		font-size: 18px;
		font-weight: 700;
		letter-spacing: 0.15em;
		font-family: var(--ca-font-mono, monospace);
		color: var(--ca-text-primary);
	}
	.gh-copy-btn {
		font-size: 11px;
		padding: 2px 8px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.gh-copy-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.gh-flow-waiting {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.text-link {
		color: var(--ca-accent, #4f8ef7);
	}
</style>
