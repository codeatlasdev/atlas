/**
 * Pure parser for `atlas crew install` stdout markers, and the checklist those phases make.
 * The CLI emits `step: <key>` lines (mapped to UI phases) and one `user: <name>` line naming the service user.
 * Imports only the message catalog, so it stays bun-testable without Svelte/Tauri.
 */
import { m } from "#lib/i18n/m.ts";

/** `signin` is Atlas Desktop's own first step (reaching the server, with the password once if need be); the CLI never prints it. */
export type InstallPhase = "signin" | "check" | "install" | "user" | "service" | "ssh" | "browsers" | "done";

/** The phases the CLI prints a `step: <key>` marker for. */
const PHASES: InstallPhase[] = ["check", "install", "user", "service", "ssh", "browsers", "done"];

/**
 * If `line` is a step-marker line (`step: <key>`), return the phase key.
 * Returns null for all other lines (progress output, errors, etc.).
 */
export function parseInstallLine(line: string): InstallPhase | null {
	const hit = line.match(/^step: (\w+)/);
	if (!hit) return null;
	return PHASES.includes(hit[1] as InstallPhase) ? (hit[1] as InstallPhase) : null;
}

/** The `user: <name>` marker: the service user hostd runs as (an existing `atlas-bot` is reused). Null for other lines. */
export function parseInstallUser(line: string): string | null {
	const hit = line.match(/^user: ([a-z_][a-z0-9_-]*)\s*$/);
	return hit ? hit[1] : null;
}

export type InstallStep = {
	phase: InstallPhase;
	label: string;
	/** What the step does, shown under the one that is running. */
	detail: string;
};

/** The name and the one-line detail of a phase, in the app's language. */
function describePhase(phase: InstallPhase): InstallStep {
	switch (phase) {
		case "signin":
			return {
				phase,
				label: m.crew_onboarding_install_signin_label(),
				detail: m.crew_onboarding_install_signin_detail(),
			};
		case "check":
			return {
				phase,
				label: m.crew_onboarding_install_check_label(),
				detail: m.crew_onboarding_install_check_detail(),
			};
		case "install":
			return {
				phase,
				label: m.crew_onboarding_install_install_label(),
				detail: m.crew_onboarding_install_install_detail(),
			};
		case "user":
			return {
				phase,
				label: m.crew_onboarding_install_user_label(),
				detail: m.crew_onboarding_install_user_detail(),
			};
		case "service":
			return {
				phase,
				label: m.crew_onboarding_install_service_label(),
				detail: m.crew_onboarding_install_service_detail(),
			};
		case "ssh":
			return {
				phase,
				label: m.crew_onboarding_install_ssh_label(),
				detail: m.crew_onboarding_install_ssh_detail(),
			};
		case "browsers":
			return {
				phase,
				label: m.crew_onboarding_install_browsers_label(),
				detail: m.crew_onboarding_install_browsers_detail(),
			};
		case "done":
			return {
				phase,
				label: m.crew_onboarding_install_done_label(),
				detail: m.crew_onboarding_install_done_detail(),
			};
	}
}

/** The phases of an install in order; `browsers` (the capture browsers, `--browsers`) is a step only when it was asked for. */
export function installPhases(withBrowsers: boolean): InstallPhase[] {
	return withBrowsers
		? ["signin", "check", "install", "user", "service", "ssh", "browsers", "done"]
		: ["signin", "check", "install", "user", "service", "ssh", "done"];
}

/** The checklist of an install, in order, worded in the app's language. */
export function installSteps(withBrowsers: boolean): InstallStep[] {
	return installPhases(withBrowsers).map(describePhase);
}
