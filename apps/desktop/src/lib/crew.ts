/**
 * Wire types of the Atlas Crew feature: crew servers, crewmates, tasks, triggers and approvals.
 * All crewmate/crew commands go through `hostCall` with an optional `host` arg that routes to
 * the right crew server (e.g. `{ host: "ssh:atlas-crew@server" }`).
 */

/** An entry from `~/.atlas/desktop/crew-servers.json`. */
export interface CrewServer {
	/** SSH target, e.g. `atlas-crew@server`. */
	host: string;
	/** Human-readable label. */
	name: string;
}

export type CrewmateState = "idle" | "working" | "needs_you";
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

export type TriggerKind = "schedule" | "interval" | "webhook" | "github" | "file" | "crewmate" | "service";

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
		| { type: "crewmate"; from_crewmate: string; on: "done" | "failed" }
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
export type CrewmateChainTrigger = Trigger & { kind: { type: "crewmate" } };
export type ServiceTrigger = Trigger & { kind: { type: "service" } };

// ─── Project linking ──────────────────────────────────────────────────────────

export interface CrewmateProject {
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

/** Interactive ask card attached to a task by the crewmate engine. */
export interface AskCard {
	id: string;
	question: string;
	options: AskOption[];
	/** Whether to show a free-text input. Default true. */
	allow_text?: boolean;
	/** Set by hostd once the user answers. */
	answered?: { kind: "key" | "text"; value: string } | null;
}

/** Engine configuration for a crewmate. */
export interface CrewmateEngine {
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

export interface CrewmateStep {
	ts: number;
	text: string;
	/** The agent's title when the step ran; the chat groups steps under it. */
	group?: string;
}

export interface CrewmateLast {
	text: string;
	state: TaskState;
	ts: number;
}

export interface Crewmate {
	id: string;
	name: string;
	role: string;
	agent: string;
	approval: ApprovalMode;
	state: CrewmateState;
	queued: number;
	last?: CrewmateLast;
	running_task?: { id: string; pane?: string };
	pending_approval?: PendingApproval;
	/** Unvalidated wire value; render through `normalizeLook`. */
	look?: unknown;
	/** Engine config (set by the onboarding conversation). */
	engine?: CrewmateEngine;
	/** Linked project (set during onboarding or later in profile). */
	project?: CrewmateProject;
	/** Triggers configured for this crewmate. */
	triggers?: Trigger[];
	/**
	 * Whether this crewmate's engine supports approval prompts.
	 * Present only when `engine` is set. False for opencode; true for claude/codex/omp.
	 * When false: approval must be "auto"; UI shows a hint and disables the selector.
	 */
	approval_supported?: boolean;
	/**
	 * Current setup conversation step. Present while the crewmate hasn't finished its
	 * initial setup chat; cleared (absent) once setup completes.
	 */
	setup_step?: string;
	/** Per-crewmate short replies toggle. Absent = use crew default (on). */
	short_replies?: boolean;
}

export interface CrewmateTask {
	id: string;
	ts: number;
	source: TaskSource;
	prompt: string;
	state: TaskState;
	pane?: string;
	/** Crewmate's markdown reply (replaces plain `result` for engine crewmates). */
	reply?: string;
	result?: string;
	error?: string;
	started_at?: number;
	finished_at?: number;
	steps?: CrewmateStep[];
	/** Interactive ask card; present while crewmate waits for user input. */
	ask?: AskCard;
	/** ID of the trigger that fired this task (links to crewmate.triggers[i].id). */
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
	/** Set while the task waits for the crewmate's project to download ("codeatlasdev/ui"); gone once it can start. */
	preparing?: string;
}

export interface CrewSettingsData {
	short_replies: boolean;
	short_replies_model?: string;
}

/** One per-turn row from the CrewmateUsage response. */
export interface CrewmateUsageRecord {
	crewmate_id: string;
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

/** Identifies a selected crewmate by its hostd routing key and crewmate id. */
export interface CrewmateSelection {
	/** `""` for local, `"ssh:atlas-crew@server"` for remote. */
	hostKey: string;
	/** Display name of the host. */
	hostName: string;
	crewmateId: string;
}

/** Key name of a secret stored for a crewmate (value never returned by any command). */
export interface CrewmateSecretKey {
	key: string;
}
