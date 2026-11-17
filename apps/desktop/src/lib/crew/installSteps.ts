/**
 * Pure parser for `atlas crew install` stdout markers.
 * The CLI emits `step: <key>` lines (mapped to UI phases) and one `user: <name>` line naming the service user.
 * No imports — stays bun-testable without Svelte/Tauri.
 */

/** `signin` is Atlas Desktop's own first step (reaching the server, with the password once if need be); the CLI never prints it. */
export type InstallPhase = "signin" | "check" | "install" | "user" | "service" | "ssh" | "browsers" | "done";

/** The phases the CLI prints a `step: <key>` marker for. */
const PHASES: InstallPhase[] = ["check", "install", "user", "service", "ssh", "browsers", "done"];

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

export type InstallStep = {
	phase: InstallPhase;
	label: string;
	/** What the step does, shown under the one that is running. */
	detail: string;
};

/** Ordered steps for the progress checklist UI. */
export const INSTALL_STEPS: InstallStep[] = [
	{ phase: "signin", label: "Signing in", detail: "Opens an SSH connection to the server." },
	{
		phase: "check",
		label: "Checking the server",
		detail: "Looks at the system, systemd and sudo. Changes nothing.",
	},
	{ phase: "install", label: "Installing Atlas", detail: "Downloads Atlas onto the server." },
	{ phase: "user", label: "Creating the service user", detail: "A separate user your crew runs as." },
	{
		phase: "service",
		label: "Starting the service",
		detail: "Atlas runs as a systemd service and restarts itself.",
	},
	{ phase: "ssh", label: "Configuring SSH access", detail: "Lets this computer connect to that user." },
	{ phase: "done", label: "Connecting", detail: "Adds the server to your list." },
];

/** The checklist of an install: `browsers` (the capture browsers, `--browsers`) is a step only when it was asked for. */
export function installSteps(withBrowsers: boolean): InstallStep[] {
	if (!withBrowsers) return INSTALL_STEPS;
	const at = INSTALL_STEPS.findIndex((s) => s.phase === "done");
	return [
		...INSTALL_STEPS.slice(0, at),
		{
			phase: "browsers",
			label: "Installing screenshot browsers",
			detail: "Chromium and WebKit, for screenshots of UI work.",
		},
		...INSTALL_STEPS.slice(at),
	];
}
