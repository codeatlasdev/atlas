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
export type TaskState = "queued" | "running" | "done" | "failed" | "cancelled";
export type TaskSource = "mobile" | "desktop" | "telegram" | "routine" | "trigger";

// ─── Triggers ────────────────────────────────────────────────────────────────

export type TriggerKind = "schedule" | "interval" | "webhook" | "github" | "file" | "bot" | "service";

interface TriggerBase {
	id: string;
	kind: TriggerKind;
	enabled: boolean;
	last_fired?: number;
	next_fire?: number;
}
export interface ScheduleTrigger extends TriggerBase {
	kind: "schedule";
	cron: string;
	prompt: string;
}
export interface IntervalTrigger extends TriggerBase {
	kind: "interval";
	every: number;
	unit: "minutes" | "hours" | "days";
	prompt: string;
}
export interface WebhookTrigger extends TriggerBase {
	kind: "webhook";
	/** Hostd-generated URL; read-only. */
	url: string;
	/** Opaque token embedded in the URL; can be regenerated. */
	token: string;
	prompt_template: string;
}
export interface GithubTrigger extends TriggerBase {
	kind: "github";
	repo: string;
	events: Array<"pr_opened" | "review_requested" | "issue_opened" | "ci_failed" | "release">;
	prompt_template: string;
}
export interface FileTrigger extends TriggerBase {
	kind: "file";
	glob: string;
	prompt_template: string;
}
export interface BotChainTrigger extends TriggerBase {
	kind: "bot";
	bot_id: string;
	on: "done" | "failed" | "both";
	prompt_template: string;
}
export interface ServiceTrigger extends TriggerBase {
	kind: "service";
	service: string;
	on: "failed" | "recovered" | "both";
	prompt_template: string;
}
export type Trigger =
	| ScheduleTrigger
	| IntervalTrigger
	| WebhookTrigger
	| GithubTrigger
	| FileTrigger
	| BotChainTrigger
	| ServiceTrigger;

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
	answered?: boolean;
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
	/** Interactive ask card, present when the bot is waiting for user input. */
	ask?: AskCard;
	/** Set when the task was fired by a trigger; rendered as a system event line. */
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
