/** Back/forward over (worktree, tab) visits, like a browser's history. Pure for tests. */

export interface Visit {
	worktreeId: string;
	tabId: string | null;
}

export interface History {
	entries: Visit[];
	/** Index of the current entry; -1 when empty. */
	at: number;
}

export const MAX_HISTORY = 100;

export const emptyHistory = (): History => ({ entries: [], at: -1 });

const same = (a: Visit | undefined, b: Visit) => !!a && a.worktreeId === b.worktreeId && a.tabId === b.tabId;

/** Record a visit: drops the forward entries, ignores a repeat of the current one. */
export function visit(h: History, v: Visit): History {
	if (same(h.entries[h.at], v)) return h;
	const entries = [...h.entries.slice(0, h.at + 1), v];
	const cut = Math.max(0, entries.length - MAX_HISTORY);
	return { entries: entries.slice(cut), at: entries.length - cut - 1 };
}

/**
 * Step `delta` entries, skipping any `alive` rejects (closed tabs, removed worktrees).
 * Returns the new history and the visit to show, or `null` when there is nothing in that direction.
 */
export function step(
	h: History,
	delta: 1 | -1,
	alive: (v: Visit) => boolean,
): { history: History; to: Visit } | null {
	for (let i = h.at + delta; i >= 0 && i < h.entries.length; i += delta)
		if (alive(h.entries[i])) return { history: { ...h, at: i }, to: h.entries[i] };
	return null;
}

export const canGo = (h: History, delta: 1 | -1, alive: (v: Visit) => boolean) =>
	step(h, delta, alive) !== null;
