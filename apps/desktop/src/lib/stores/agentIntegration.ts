/**
 * Agent integrations: the hooks (omp extension, Claude Code and Codex hooks) that let an agent
 * tell Atlas, and Atlas Mobile, when it works, waits for you or is done. hostd's
 * `agent_integrations` lists them (via `atlas setup <agent> --check`); `atlas setup <setup>
 * --apply` turns one on. Pure: no IPC.
 */

/** One installed agent, as hostd's `agent_integrations` reports it. */
export interface AgentIntegration {
	id: string;
	label: string;
	/** `atlas setup` name, e.g. `claude-code`. */
	setup: string;
	state: "ok" | "missing" | "unknown";
}

/** localStorage key that hides the prompt for `id` on this computer ("Not now"). */
export function dismissedKey(id: string): string {
	// omp's key predates the others; keeping it keeps an earlier "Not now".
	return id === "omp" ? "atlas.ompPromptDismissed" : `atlas.integrationPromptDismissed.${id}`;
}

/** The integration to offer in the workspace: the first missing one not dismissed, or `null`. */
export function promptFor(
	integrations: AgentIntegration[],
	dismissed: (id: string) => boolean,
): AgentIntegration | null {
	return integrations.find((a) => a.state === "missing" && !dismissed(a.id)) ?? null;
}
