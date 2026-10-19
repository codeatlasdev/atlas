/** Pure tab rules: pinned tabs, closed-tab stack, tab-N and MRU switching. */

interface TabLike {
	id: string;
	kind: string;
	worktreeId: string;
}

type TabOf = (id: string) => TabLike | undefined;

/** Single source of truth: the project Panel can never be closed, moved or reordered. */
export const pinned = (t: { kind: string } | undefined | null): boolean => t?.kind === "services";

/** Index where a movable tab may sit in `order`: right after the pinned tabs. */
export function firstMovable(order: string[], tabOf: TabOf): number {
	let n = 0;
	while (n < order.length && pinned(tabOf(order[n]))) n++;
	return n;
}

/** `order` with pinned tabs first (stable otherwise). */
export function pinnedFirst(order: string[], tabOf: TabOf): string[] {
	return [...order.filter((id) => pinned(tabOf(id))), ...order.filter((id) => !pinned(tabOf(id)))];
}

/** Ids that close-others / close-all may remove: never pinned, never `keep`. */
export function closable(order: string[], tabOf: TabOf, keep?: string): string[] {
	return order.filter((id) => id !== keep && !pinned(tabOf(id)));
}

/** Tab for ⌘N: 1…8 by position, 9 = last (browser convention); undefined when out of range. */
export function tabAt(order: string[], n: number): string | undefined {
	if (n < 1 || n > 9) return undefined;
	return n === 9 ? order[order.length - 1] : order[n - 1];
}

/** Enough of a closed tab to reopen it (terminals restart with their argv and cwd). */
export interface ClosedTab {
	kind: string;
	label: string;
	customLabel?: boolean;
	path?: string;
	url?: string;
	argv?: string[];
	env?: Record<string, string>;
	/** Folder a terminal was opened in (absent = the worktree root). */
	cwd?: string;
	agent?: string;
	prNumber?: number;
	chat?: boolean;
	/** Position in its group when closed. */
	index: number;
}

export const CLOSED_MAX = 20;

/** Kinds `reopen` restores; the rest are singletons reopened from the palette. */
const REOPENABLE: Record<string, true> = {
	terminal: true,
	editor: true,
	diff: true,
	browser: true,
	logs: true,
	config: true,
	pr: true,
};

export function snapshot(t: Omit<ClosedTab, "index">, index: number): ClosedTab | null {
	if (!(t.kind in REOPENABLE)) return null;
	const { kind, label, customLabel, path, url, argv, env, cwd, agent, prNumber, chat } = t;
	return { kind, label, customLabel, path, url, argv, env, cwd, agent, prNumber, chat, index };
}

/** `stack` with `c` on top, capped at `CLOSED_MAX`. */
export const pushClosed = (stack: ClosedTab[], c: ClosedTab): ClosedTab[] => [...stack, c].slice(-CLOSED_MAX);

/** Next id in a most-recently-used list, `step` entries away (wraps). */
export function mruNext(mru: string[], current: string | null, step: 1 | -1): string | undefined {
	if (mru.length < 2) return undefined;
	const from = Math.max(0, current ? mru.indexOf(current) : 0);
	return mru[(from + step + mru.length) % mru.length];
}
