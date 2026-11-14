/**
 * Reducer for the crew-server install checklist: folds the `crew://install-line` stream (`step: <key>` and
 * `user: <name>` markers) into which phases are ticked, which one spins and which service user the installer
 * reported. Pure, so the checklist rules are bun-testable.
 */
import {
	INSTALL_STEPS,
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

/** Every phase ticked, nothing spinning: the install finished. */
export function completeInstall(p: InstallProgress): InstallProgress {
	return { ...p, done: new Set(INSTALL_STEPS.map((s) => s.phase)), active: null };
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

/** Which password prompt an install failure calls for: the Mac's key is not authorized yet, sudo needs a password, or neither. */
export function authModeForError(err: string): "authorize" | "sudo" | "none" {
	if (err.includes("Permission denied (publickey)")) return "authorize";
	if (
		err.includes("asks for a password") ||
		err.includes("passwordless sudo") ||
		err.includes("sudo password rejected")
	) {
		return "sudo";
	}
	return "none";
}
