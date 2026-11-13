/**
 * What a runner service state means for the panel. One place for the strings the runner sends
 * (`ServiceState::name` in Rust): `unhealthy` (process alive, liveness probe not answering) and
 * `degraded` (liveness passes, `ready` probe fails or the same error keeps repeating) sit between
 * `running` and `failed`, so every counter, banner and button asks here instead of comparing strings.
 */

/** The fields of the runner's `service_json` that classification reads. */
export interface StateRow {
	state: string;
	note?: string | null;
	exit_code?: number | null;
	probe_error?: string | null;
	unhealthy_since?: number | null;
	unhealthy_misses?: number | null;
}

/** Green: up and answering, ready probe (when there is one) included. */
export const isGreen = (s: Pick<StateRow, "state">): boolean => s.state === "running";

/** A process is alive: restart/stop make sense, and its log ticker is live. */
export const isAlive = (s: Pick<StateRow, "state">): boolean =>
	s.state === "running" || s.state === "starting" || s.state === "degraded" || s.state === "unhealthy";

/** Failed, hung or degraded: something a human should look at. */
export const needsAttention = (s: Pick<StateRow, "state">): boolean =>
	s.state === "failed" || s.state === "unhealthy" || s.state === "degraded";

/**
 * The card banner: why the service needs attention, or null. `now` (ms) is injectable so the
 * "for Ns" text is deterministic.
 */
export function problemText(s: StateRow, now: number = Date.now()): string | null {
	switch (s.state) {
		case "failed":
			return s.note ?? (s.exit_code != null ? `Exited with code ${s.exit_code}` : "Failed to start");
		case "unhealthy": {
			const secs = s.unhealthy_since != null ? Math.max(0, Math.floor((now - s.unhealthy_since * 1000) / 1000)) : 0;
			const misses = s.unhealthy_misses != null ? `, ${s.unhealthy_misses} probes missed` : "";
			return `Not responding for ${secs}s${misses}`;
		}
		case "degraded":
			return s.probe_error ?? s.note ?? "Degraded";
		default:
			return null;
	}
}
