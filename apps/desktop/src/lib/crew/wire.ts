/**
 * The boundary between hostd's wire shapes and the UI's: hostd sends unix seconds, every timestamp the UI holds is
 * milliseconds. The store calls these once per response so no component ever sees seconds.
 */
import type { Crewmate, CrewmateTask } from "#lib/crew.ts";

const toMs = (secs: number | undefined) => (secs != null ? secs * 1000 : secs);

/** A task with `ts`, `started_at`, `finished_at` and every step's `ts` in milliseconds. */
export function normalizeTask(t: CrewmateTask): CrewmateTask {
	return {
		...t,
		ts: t.ts * 1000,
		started_at: toMs(t.started_at),
		finished_at: toMs(t.finished_at),
		steps: t.steps?.map((s) => ({ ...s, ts: s.ts * 1000 })),
	};
}

/** A crewmate with `last.ts` in milliseconds. */
export function normalizeCrewmate(c: Crewmate): Crewmate {
	return c.last ? { ...c, last: { ...c.last, ts: c.last.ts * 1000 } } : c;
}
