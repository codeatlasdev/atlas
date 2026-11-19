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

/**
 * A crewmate with `last.ts` in milliseconds, and its `state` as it really is.
 *
 * hostd's `state` is the one answer ("needs_you" for a tool approval or a task parked on any card), and is used as
 * sent. A crew server before #524 (the report ran 1.5.0-alpha.24) says "working" for a crewmate that is parked on a
 * card, because its runner is held: its `waiting_task` (alpha.26 and later) or a `last` task that is itself
 * `waiting_action` still tell it. Older servers that send neither (and whose parked task is hidden behind newer
 * queued ones) cannot be told from here: the chat of the selected crewmate reads its tasks (`withParkedTask`).
 */
export function normalizeCrewmate(c: Crewmate): Crewmate {
	const parked = c.waiting_task != null || c.last?.state === "waiting_action";
	const fixed: Crewmate = c.state === "working" && parked ? { ...c, state: "needs_you" } : c;
	return fixed.last ? { ...fixed, last: { ...fixed.last, ts: fixed.last.ts * 1000 } } : fixed;
}

/** `c` as `needs_you` when one of its `tasks` (already loaded for the open chat) is parked on a card. */
export function withParkedTask(c: Crewmate, tasks: CrewmateTask[]): Crewmate {
	const parked = tasks.some((t) => t.state === "waiting_action" && t.action);
	return c.state === "working" && parked ? { ...c, state: "needs_you" } : c;
}
