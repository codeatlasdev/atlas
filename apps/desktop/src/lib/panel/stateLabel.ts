import { m } from "#lib/i18n/m.ts";

/**
 * A service state as a person reads it. The runner sends the state as a stable English word (`running`, `failed`);
 * this is its translation at the display boundary, and a state this build does not know shows as sent.
 */
export function stateLabel(state: string): string {
	switch (state) {
		case "running":
			return m.panel_state_running();
		case "starting":
			return m.panel_state_starting();
		case "failed":
			return m.panel_state_failed();
		case "unhealthy":
			return m.panel_state_unhealthy();
		case "degraded":
			return m.panel_state_degraded();
		case "stopped":
			return m.panel_state_stopped();
		case "disabled":
			return m.panel_state_disabled();
		default:
			return state;
	}
}
