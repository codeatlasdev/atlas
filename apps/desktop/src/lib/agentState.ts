import { m } from "#lib/i18n/m.ts";

/**
 * The agent states the hooks report (`working` | `idle` | …) as a person reads them; another one shows as sent.
 * Its own module: the tab strip and the sidebar draw it at startup and must not pull the Machine view's helpers in.
 */
export function agentStateLabel(state: string): string {
	switch (state) {
		case "working":
			return m.panel_machine_agent_working();
		case "idle":
			return m.panel_machine_agent_idle();
		case "blocked":
			return m.panel_machine_agent_blocked();
		case "waiting":
			return m.panel_machine_agent_waiting();
		case "done":
			return m.panel_machine_agent_done();
		default:
			return state;
	}
}
