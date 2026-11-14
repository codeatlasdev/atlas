/**
 * Pure parser for `atlas crew install` stdout markers.
 * The CLI emits `step: <key>` lines (mapped to UI phases) and one `user: <name>` line naming the service user.
 * No imports — stays bun-testable without Svelte/Tauri.
 */

export type InstallPhase = "check" | "install" | "user" | "service" | "ssh" | "done";

const PHASES: InstallPhase[] = ["check", "install", "user", "service", "ssh", "done"];

/**
 * If `line` is a step-marker line (`step: <key>`), return the phase key.
 * Returns null for all other lines (progress output, errors, etc.).
 */
export function parseInstallLine(line: string): InstallPhase | null {
	const m = line.match(/^step: (\w+)/);
	if (!m) return null;
	return PHASES.includes(m[1] as InstallPhase) ? (m[1] as InstallPhase) : null;
}

/** The `user: <name>` marker: the service user hostd runs as (an existing `atlas-bot` is reused). Null for other lines. */
export function parseInstallUser(line: string): string | null {
	const m = line.match(/^user: ([a-z_][a-z0-9_-]*)\s*$/);
	return m ? m[1] : null;
}

export type InstallStep = { phase: InstallPhase; label: string };

/** Ordered steps for the progress checklist UI. */
export const INSTALL_STEPS: InstallStep[] = [
	{ phase: "check", label: "Checking the server" },
	{ phase: "install", label: "Installing Atlas" },
	{ phase: "user", label: "Creating the service user" },
	{ phase: "service", label: "Starting the service" },
	{ phase: "ssh", label: "Configuring SSH access" },
	{ phase: "done", label: "Connecting" },
];
