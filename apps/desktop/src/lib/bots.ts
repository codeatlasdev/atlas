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
export type TaskSource = "mobile" | "desktop" | "telegram" | "routine";

export interface PendingApproval {
	req: string;
	tool_name: string;
	tool_input: unknown;
	pane_key: string;
}

export interface Bot {
	id: string;
	name: string;
	role: string;
	agent: string;
	approval: ApprovalMode;
	state: BotState;
	running_task?: BotTask;
	pending_approval?: PendingApproval;
}

export interface BotTask {
	id: string;
	ts: number;
	source: TaskSource;
	prompt: string;
	state: TaskState;
	pane?: string;
	result?: string;
	error?: string;
	finished_at?: number;
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
