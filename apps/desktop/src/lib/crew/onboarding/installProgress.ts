/**
 * Reducer for the crew-server install checklist: folds the `crew://install-line` stream (`step: <key>` and
 * `user: <name>` markers) into which phases are ticked, which one spins and which service user the installer
 * reported. Pure, so the checklist rules are bun-testable.
 */
import {
	installSteps,
	parseInstallLine,
	parseInstallUser,
	type InstallPhase,
} from "#lib/crew/installSteps.ts";

export interface InstallProgress {
	/** Phases that have completed (ticked in the checklist). */
	done: Set<InstallPhase>;
	/** The phase currently running (spinner). */
	active: InstallPhase | null;
	/** The service user the installer reported (`user: <name>`), or "" until it did. */
	user: string;
}

export const initialInstallProgress = (): InstallProgress => ({ done: new Set(), active: null, user: "" });

/** Every phase ticked, nothing spinning: the install finished. `browsers` is ticked only if the install reached it. */
export function completeInstall(p: InstallProgress): InstallProgress {
	const ran = p.done.has("browsers") || p.active === "browsers";
	return { ...p, done: new Set(installSteps(ran).map((s) => s.phase)), active: null };
}

/** Fold one installer output line into the progress; lines that are not markers leave it unchanged. */
export function applyInstallLine(p: InstallProgress, line: string): InstallProgress {
	const user = parseInstallUser(line);
	if (user) return { ...p, user };
	const phase = parseInstallLine(line);
	if (!phase) return p;
	if (phase === "done") return completeInstall(p);
	return { ...p, done: p.active ? new Set([...p.done, p.active]) : p.done, active: phase };
}

/** Whether a (string) failure of the browsers commands is sudo asking for a password, so the page asks for it and retries. */
export function needsSudoPassword(err: string): boolean {
	return (
		err.includes("asks for a password") ||
		err.includes("passwordless sudo") ||
		err.includes("sudo password rejected")
	);
}
