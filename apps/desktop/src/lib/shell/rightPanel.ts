/**
 * What the right panel (Agents / Ports) shows, as pure functions of the stores' data: the agents running in the
 * worktree, and its ports joined with the services of its atlas.yaml. No IPC, no DOM.
 */
import type { ServiceDef } from "#lib/api.ts";
import type { AgentState } from "#lib/stores/agents.svelte.ts";
import { m } from "#lib/i18n/m.ts";

export interface AgentTab {
	id: string;
	worktreeId: string;
	agent?: string;
	pty?: string;
}

export interface AgentStatusLite {
	state: AgentState;
	prompt: string | null;
	last_message: string | null;
	state_started_at: number;
}

export interface LiveAgent {
	tabId: string;
	agent: string;
	state: AgentState;
	/** What the agent last said, else what it was asked; null when neither is known. */
	detail: string | null;
	/** Epoch ms the agent entered `state`; null when the hub has not reported it. */
	since: number | null;
}

/** Agent state words. Getters, so each read follows the app's language (`STATE_LABEL[state]` stays the call). */
export const STATE_LABEL: Record<AgentState, string> = {
	get blocked() {
		return m.shell_chrome_agent_state_needs_you();
	},
	get waiting() {
		return m.shell_chrome_agent_state_needs_you();
	},
	get working() {
		return m.shell_chrome_agent_state_working();
	},
	get done() {
		return m.common_action_done();
	},
	get idle() {
		return m.shell_chrome_agent_state_idle();
	},
};

const ORDER: AgentState[] = ["blocked", "waiting", "working", "done", "idle"];

/** The agent terminals of `worktreeId`, those that need the user first, then by how long they have been in that state. */
export function liveAgents(
	tabs: readonly AgentTab[],
	status: Record<string, AgentStatusLite | undefined>,
	worktreeId: string,
): LiveAgent[] {
	return tabs
		.filter((t) => t.worktreeId === worktreeId && t.agent && t.pty)
		.map((t): LiveAgent => {
			const st = status[t.id];
			const text = (st?.last_message ?? st?.prompt ?? "").replace(/\s+/g, " ").trim();
			return {
				tabId: t.id,
				agent: t.agent!,
				state: st?.state ?? "idle",
				detail: text ? text.slice(0, 160) : null,
				since: st?.state_started_at || null,
			};
		})
		.sort((a, b) => ORDER.indexOf(a.state) - ORDER.indexOf(b.state) || (b.since ?? 0) - (a.since ?? 0));
}

export interface ListeningPort {
	port: number;
	pid: number;
	command: string;
}

export interface PortRow {
	port: number;
	/** The atlas.yaml service that owns this port, if any. */
	service: string | null;
	/** `node (4242)` for a port found in a terminal's process tree; null for a service the runner started. */
	process: string | null;
}

/** A service state that means the process is up (runner `service_json`). */
const UP = new Set(["running"]);

/**
 * The ports to list: every port a terminal of the worktree listens on (named after the service that declares it in
 * atlas.yaml), plus the declared port of each service that is running but not seen in a terminal (`atlas dev` starts
 * services outside the terminals, so their ports are never in the process trees).
 */
export function portRows(
	listening: readonly ListeningPort[],
	defs: readonly ServiceDef[],
	running: readonly { name: string; state: string }[],
): PortRow[] {
	const owner = new Map<number, string>();
	for (const d of defs) if (d.port && (d.enabled || !owner.has(d.port))) owner.set(d.port, d.name);
	const rows: PortRow[] = listening.map((p) => ({
		port: p.port,
		service: owner.get(p.port) ?? null,
		process: p.command ? `${p.command} (${p.pid})` : null,
	}));
	const seen = new Set(rows.map((r) => r.port));
	const up = new Set(running.filter((s) => UP.has(s.state)).map((s) => s.name));
	for (const d of defs)
		if (d.port && up.has(d.name) && !seen.has(d.port)) {
			seen.add(d.port);
			rows.push({ port: d.port, service: d.name, process: null });
		}
	return rows.sort((a, b) => a.port - b.port);
}
