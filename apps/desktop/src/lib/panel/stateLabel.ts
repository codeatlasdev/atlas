import { m } from "#lib/i18n/m.ts";

/**
 * Every state the runner sends for a service or a tunnel (`ServiceState` in `crates/atlas-tui/src/runtime/service.rs`,
 * serialized lowercase). The wire values stay English; this is their translation at the display boundary.
 * `stateLabel.test.ts` reads the Rust enum, so a new variant fails there until it has a message.
 */
export const STATE_MESSAGES: Record<string, () => string> = {
	running: () => m.panel_state_running(),
	starting: () => m.panel_state_starting(),
	failed: () => m.panel_state_failed(),
	unhealthy: () => m.panel_state_unhealthy(),
	degraded: () => m.panel_state_degraded(),
	stopped: () => m.panel_state_stopped(),
	disabled: () => m.panel_state_disabled(),
	suspended: () => m.panel_state_suspended(),
};

/** A service or tunnel state as a person reads it; a state this build does not know shows as sent. */
export function stateLabel(state: string): string {
	return Object.hasOwn(STATE_MESSAGES, state) ? STATE_MESSAGES[state]() : state;
}
