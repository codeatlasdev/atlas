/** Which earcon an observed change deserves, if any. Pure: the store feeds it real runner/agent/scan state. */
import type { Category } from "./policy.ts";

/** The slice of a runner service row the rules read (runner.rs `service_json`). */
interface Row {
	name: string;
	state: string;
	completed_ok?: boolean;
	deselected?: boolean;
}

/** A service that is meant to be up: not deselected, not a one-shot that already finished. */
const wanted = (s: Row) => !s.deselected && !s.completed_ok;

/**
 * Compare a project's service rows before/after one runner event.
 * Failure beats success; the first list ever seen (resync, history replay) is silent so reopening the app never
 * plays the sounds of things that happened earlier.
 */
export function stackSound(prev: Row[] | undefined, next: Row[]): Category | null {
	if (!prev?.length || !next.length) return null;
	const before = new Map(prev.map((s) => [s.name, s.state]));
	// A hang (`unhealthy`) sounds like a crash; `degraded` is a warning, no jingle.
	const down: Record<string, true> = { failed: true, unhealthy: true };
	if (next.some((s) => down[s.state] && before.has(s.name) && !down[before.get(s.name) ?? ""]))
		return "service-failed";
	const want = next.filter(wanted);
	if (!want.length || !want.every((s) => s.state === "running")) return null;
	// Everything is up now; only a *transition* into that sounds (something was still starting before).
	const wasStarting = prev.some((s) => wanted(s) && s.state === "starting");
	return wasStarting ? "stack-up" : null;
}

/** Agent state → earcon (the same states `agents.apply` raises notifications for). */
export function agentSound(state: string): Category | null {
	if (state === "done") return "agent-done";
	if (state === "waiting" || state === "blocked") return "agent-needs-you";
	return null;
}

/** A scan that ended normally and found something. */
export function discoverySound(
	done: { cancelled: boolean } | null,
	projectsFound: number,
	error: string | null,
): Category | null {
	return done && !done.cancelled && !error && projectsFound > 0 ? "discovery-done" : null;
}
