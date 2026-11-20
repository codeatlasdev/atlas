/**
 * Which finished task is read aloud: one that finished while this chat was open. The first list a chat shows is only
 * a baseline (old replies are never read out loud when it opens), and a task is read once.
 */
import type { CrewmateTask } from "#lib/crew.ts";

export interface Watch {
	/** Task id -> last state seen. `null` until the first list (the baseline) has been taken. */
	seen: Map<string, string> | null;
	/** Tasks already read (or deliberately skipped): never again. */
	done: Set<string>;
}

export const newWatch = (): Watch => ({ seen: null, done: new Set() });

/**
 * Look at the chat's tasks (newest first or oldest first, either) and return the text to say, or `null`.
 * Updates `w`. The task read is the most recently finished of those that just finished.
 */
export function nextSpoken(w: Watch, tasks: CrewmateTask[]): string | null {
	const before = w.seen;
	w.seen = new Map(tasks.map((t) => [t.id, t.state]));
	if (!before) return null;
	let pick: CrewmateTask | null = null;
	for (const t of tasks) {
		if (t.state !== "done" || w.done.has(t.id)) continue;
		// Seen unfinished, or not in the baseline at all (it started after this chat opened).
		const was = before.get(t.id);
		if (was === "done") continue;
		w.done.add(t.id);
		if (!pick || (t.finished_at ?? t.ts) >= (pick.finished_at ?? pick.ts)) pick = t;
	}
	// hostd computes `spoken` (the agent's own `spoken:` line, or the first sentences); empty means nothing to say.
	return pick?.spoken?.trim() || null;
}
