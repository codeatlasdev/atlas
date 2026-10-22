import type { BotTask, TaskState } from "#lib/bots.ts";

/**
 * Derive a short display tag from the bot's role field.
 */
export function roleTag(role: string): string {
	if (!role) return "";
	const colonIdx = role.indexOf(":");
	if (colonIdx !== -1) {
		const before = role.slice(0, colonIdx).trim();
		if (before.length > 0 && before.length <= 20) return before;
	}
	const firstWord = role.split(/\s+/)[0];
	if (firstWord && firstWord.length <= 15 && /^[A-Z]/.test(firstWord) && /[A-Z]/.test(firstWord.slice(1)))
		return firstWord;
	return "";
}

/**
 * Return icon prefix for list row preview.
 */
export function previewIcon(last?: { state: TaskState; ts: number }, state?: string): string {
	if (state === "needs_you") return "⚠";
	if (last?.state === "done") return "✓";
	if (last?.state === "failed") return "✕";
	return "";
}

/**
 * Group tasks for Discord cozy mode.
 * Input tasks must be ordered oldest-first.
 * Returns same-length array with groupStart flag per task.
 */
export function groupTasks(tasks: BotTask[]): Array<{ task: BotTask; groupStart: boolean }> {
	const GROUP_GAP_MS = 7 * 60 * 1000;
	return tasks.map((task, i) => {
		if (i === 0) return { task, groupStart: true };
		const gap = task.ts - tasks[i - 1].ts;
		return { task, groupStart: gap > GROUP_GAP_MS };
	});
}

/**
 * Find the first unread task index.
 * Tasks must be ordered oldest-first.
 * `lastSeenTs` is in milliseconds (from markRead / #lastSeen); `t.ts` is in seconds.
 * Returns -1 if lastSeenTs === 0 or all tasks are seen.
 */
export function firstUnreadIndex(tasks: BotTask[], lastSeenTs: number): number {
	if (lastSeenTs === 0) return -1;
	const idx = tasks.findIndex((t) => t.ts * 1000 > lastSeenTs);
	return idx;
}

/**
 * Sort activity entries by task.ts descending (newest first), stable.
 */
export function mergeActivity(
	entries: Array<{ botId: string; botName: string; hostKey: string; task: BotTask }>,
): typeof entries {
	return entries.slice().sort((a, b) => b.task.ts - a.task.ts);
}
