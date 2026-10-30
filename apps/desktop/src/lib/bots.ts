/**
 * Types for the Atlas Bots feature: bot hosts, bots, tasks, routines, and approvals.
 * All bot commands go through `hostCall` with an optional `host` arg that routes to
 * the correct bot computer (e.g. `{ host: "ssh:atlas-bot@server" }`).
 */

/** An entry from `~/.atlas/desktop/bot-hosts.json`. */
export interface BotHost {
	/** SSH target, e.g. `atlas-bot@server`. */
	host: string;
	/** Human-readable label. */
	name: string;
}

export type BotState = "idle" | "working" | "needs_you";
export type ApprovalMode = "ask" | "auto";
export type TaskState = "queued" | "running" | "done" | "failed" | "cancelled" | "waiting_action";
export type TaskSource = "mobile" | "desktop" | "telegram" | "routine" | "trigger";

// ─── Task actions ─────────────────────────────────────────────────────────────

export type TaskAction =
	| { kind: "connect_github"; reason: string }
	| { kind: "provide_secret"; name: string; label: string; reason: string }
	| { kind: "input"; label: string; reason: string; placeholder?: string }
	| { kind: "choose"; question: string; options: string[] }
	| { kind: "sign_in_engine"; engine: string }
	| { kind: "add_trigger"; trigger: Trigger["kind"]; prompt: string; reason: string };

// ─── Triggers ────────────────────────────────────────────────────────────────

export type TriggerKind = "schedule" | "interval" | "webhook" | "github" | "file" | "bot" | "service";

/** Wire shape: kind is a nested object with a `type` discriminant. */
export interface Trigger {
	id: string;
	/** Nested kind object — e.g. `{ type: "schedule", cron: "0 9 * * 1-5" }` */
	kind:
		| { type: "schedule"; cron: string }
		| { type: "interval"; every_secs: number }
		| { type: "webhook"; token: string }
		| { type: "github"; repo: string; events: string[] }
		| { type: "file"; path: string; glob?: string }
		| { type: "bot"; from_bot: string; on: "done" | "failed" }
		| { type: "service"; project_dir: string; service: string; on: "failed" | "recovered" };
	/** Top-level prompt (supports `{{event}}` substitution). */
	prompt: string;
	enabled: boolean;
	last_fired?: number;
	/** Unix seconds of next scheduled/interval fire; computed by hostd, never sent on input. */
	next_fire?: number;
	/** IANA timezone for evaluating cron (e.g. "America/Sao_Paulo"). None = UTC. */
	tz?: string;
}
/** Convenience alias — kind discriminant for UI switch statements. */
export type ScheduleTrigger = Trigger & { kind: { type: "schedule" } };
export type IntervalTrigger = Trigger & { kind: { type: "interval" } };
export type WebhookTrigger = Trigger & { kind: { type: "webhook" } };
export type GithubTrigger = Trigger & { kind: { type: "github" } };
export type FileTrigger = Trigger & { kind: { type: "file" } };
export type BotChainTrigger = Trigger & { kind: { type: "bot" } };
export type ServiceTrigger = Trigger & { kind: { type: "service" } };

// ─── Project linking ──────────────────────────────────────────────────────────

export interface BotProject {
	name: string;
	kind: "git" | "path";
	/** Git remote URL (when kind === 'git'). */
	url?: string;
	/** Server-side path (when kind === 'path'). */
	path?: string;
	branch?: string;
}

/** One choice in an ask card. */
export interface AskOption {
	key: string;
	title: string;
	subtitle?: string;
}

/** Interactive ask card attached to a task by the bot engine. */
export interface AskCard {
	id: string;
	question: string;
	options: AskOption[];
	/** Whether to show a free-text input. Default true. */
	allow_text?: boolean;
	/** Set by hostd once the user answers. */
	answered?: { kind: "key" | "text"; value: string } | null;
}

/** Engine configuration for a bot. */
export interface BotEngine {
	kind: string;
	provider?: string;
	base_url?: string;
	model?: string;
}

export interface PendingApproval {
	req: string;
	tool_name: string;
	tool_input: unknown;
	pane_key: string;
}

export interface BotStep {
	ts: number;
	text: string;
	/** The agent's title when the step ran; the chat groups steps under it. */
	group?: string;
}

export interface BotLast {
	text: string;
	state: TaskState;
	ts: number;
}

export interface Bot {
	id: string;
	name: string;
	role: string;
	agent: string;
	approval: ApprovalMode;
	state: BotState;
	queued: number;
	last?: BotLast;
	running_task?: { id: string; pane?: string };
	pending_approval?: PendingApproval;
	/** Unvalidated wire value; render through `normalizeLook`. */
	look?: unknown;
	/** Engine config (set by the onboarding conversation). */
	engine?: BotEngine;
	/** Linked project (set during onboarding or later in profile). */
	project?: BotProject;
	/** Triggers configured for this bot. */
	triggers?: Trigger[];
	/**
	 * Whether this bot's engine supports approval prompts.
	 * Present only when `engine` is set. False for opencode; true for claude/codex/omp.
	 * When false: approval must be "auto"; UI shows a hint and disables the selector.
	 */
	approval_supported?: boolean;
	/**
	 * Current setup conversation step. Present while the bot hasn't finished its
	 * initial setup chat; cleared (absent) once setup completes.
	 */
	setup_step?: string;
	/** Per-bot short replies toggle. Absent = use crew default (on). */
	short_replies?: boolean;
}

export interface BotTask {
	id: string;
	ts: number;
	source: TaskSource;
	prompt: string;
	state: TaskState;
	pane?: string;
	/** Bot's markdown reply (replaces plain `result` for engine bots). */
	reply?: string;
	result?: string;
	error?: string;
	started_at?: number;
	finished_at?: number;
	steps?: BotStep[];
	/** Interactive ask card; present while bot waits for user input. */
	ask?: AskCard;
	/** ID of the trigger that fired this task (links to bot.triggers[i].id). */
	trigger_id?: string;
	/** Resolved trigger event line for the chat thread (UI-computed). */
	trigger_event?: { kind: TriggerKind; summary: string };
	/** Per-turn usage data from the engine. */
	usage?: {
		model?: string;
		input_tokens?: number;
		output_tokens?: number;
		cache_read?: number;
		cache_write?: number;
		cost_usd?: number;
		duration_ms?: number;
	};
	/** Crew action blocking task progress; present when state is "waiting_action". */
	action?: TaskAction;
	/** Original full reply before summarization; present only when summarized is true. */
	reply_full?: string;
	/** True when result is a summary and reply_full holds the original. */
	summarized?: boolean;
	/** What the user picked/typed for the task's choose/input action; `action` keeps the question. */
	answer?: string;
	/** What the agent says it is working on (omp's session title), shown on the live card while it runs. */
	title?: string;
	/** IANA timezone sent with the task; hostd uses it for cron evaluation and agent context. */
	tz?: string;
	/** Set while the task waits for the bot's project to download ("codeatlasdev/ui"); gone once it can start. */
	preparing?: string;
}

export interface BotCrewSettings {
	short_replies: boolean;
	short_replies_model?: string;
}

/** One per-turn row from the BotUsage response. */
export interface BotUsageRecord {
	bot_id: string;
	engine: string;
	ts: number;
	model?: string;
	input_tokens?: number;
	output_tokens?: number;
	cache_read?: number;
	cache_write?: number;
	cost_usd?: number;
	duration_ms?: number;
}

export interface Routine {
	id: string;
	cron: string;
	prompt: string;
	enabled: boolean;
	last_run?: number;
}

/** Identifies a selected bot by its hostd routing key and bot id. */
export interface BotSelection {
	/** `""` for local, `"ssh:atlas-bot@server"` for remote. */
	hostKey: string;
	/** Display name of the host. */
	hostName: string;
	botId: string;
}

/** Key name of a secret stored for a bot (value never returned by any command). */
export interface BotSecretKey {
	key: string;
}
