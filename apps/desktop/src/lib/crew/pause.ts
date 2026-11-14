/**
 * The crew kill switch as the desktop reads it: `crew_pause` / `crew_resume` on a crew server, the `paused` flag of its
 * `crewmate_list` reply and the `crew_paused` push event. Pure (no Tauri/Svelte): the contract tests feed it the
 * fixtures hostd's own serializer wrote.
 */
import { normalizeCrewmate } from "#lib/crew/wire.ts";
import type { Crewmate } from "#lib/crew.ts";

/** A `crewmate_list` reply with its crewmates normalised and the server's paused state. */
export function parseCrewList(r: { crewmates: Crewmate[]; paused?: unknown }): {
	crewmates: Crewmate[];
	paused: boolean;
} {
	// Strictly a boolean `true`: a crew server older than the kill switch sends nothing, and must not look paused.
	return { crewmates: r.crewmates.map(normalizeCrewmate), paused: r.paused === true };
}

/** The new state in a `crew_paused` event; `null` for any other event or a malformed one. */
export function pausedFromEvent(ev: unknown): boolean | null {
	const o = (ev ?? {}) as { ev?: unknown; paused?: unknown };
	return o.ev === "crew_paused" && typeof o.paused === "boolean" ? o.paused : null;
}

/** Whether any crew server is paused (drives the banner). */
export function anyPaused(servers: { paused?: boolean }[]): boolean {
	return servers.some((s) => s.paused === true);
}
