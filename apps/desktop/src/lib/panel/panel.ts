/** Pure helpers for the project panel: ordering, grid navigation, shell quoting. */

/** Single-quote `s` for `sh -c`. */
export function shq(s: string): string {
	return `'${s.replaceAll("'", `'\\''`)}'`;
}

/** argv for a login shell started inside `dir` (relative to the worktree root, where the pty starts). */
export function shellIn(dir: string | null): string[] {
	return dir ? ["sh", "-c", `cd ${shq(dir)} && exec "\${SHELL:-/bin/sh}" -l`] : [];
}

/** Sort weight: failures first, then coming up, running, the rest. */
const WEIGHT: Record<string, number> = { failed: 0, starting: 1, running: 2 };

export function byAttention<T extends { name: string; state: string }>(rows: T[]): T[] {
	return [...rows].sort(
		(a, b) => (WEIGHT[a.state] ?? 3) - (WEIGHT[b.state] ?? 3) || a.name.localeCompare(b.name),
	);
}

/**
 * Next index when moving through a grid of `count` cards laid out `cols` per row.
 * Left/right step by one, up/down by a row, Home/End jump; moves clamp at the edges.
 */
export function gridStep(key: string, from: number, count: number, cols: number): number {
	if (count === 0) return -1;
	const last = count - 1;
	switch (key) {
		case "ArrowRight":
			return Math.min(last, from + 1);
		case "ArrowLeft":
			return Math.max(0, from - 1);
		case "ArrowDown":
			return Math.min(last, from + cols);
		case "ArrowUp":
			return Math.max(0, from - cols);
		case "Home":
			return 0;
		case "End":
			return last;
		default:
			return from;
	}
}
