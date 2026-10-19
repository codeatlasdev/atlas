/**
 * Working/idle of an agent that reports no state of its own (#56: a custom CLI without `atlas hook`): hostd's
 * `pty_activity` (at most one a second while the program prints) means working, `QUIET_MS` without one means
 * idle. Timers are injected so tests drive time without waiting.
 */
export const QUIET_MS = 3_000;

export interface Timers {
	set(fn: () => void, ms: number): unknown;
	clear(handle: unknown): void;
}

export function activityTracker(
	onState: (pane: string, state: "working" | "idle") => void,
	timers: Timers = { set: (fn, ms) => setTimeout(fn, ms), clear: (h) => clearTimeout(h as number) },
) {
	const quiet = new Map<string, unknown>();
	return {
		/** Output seen in `pane`: working now (once per burst), idle after `QUIET_MS` of silence. */
		bump(pane: string) {
			const t = quiet.get(pane);
			if (t === undefined) onState(pane, "working");
			else timers.clear(t);
			quiet.set(
				pane,
				timers.set(() => {
					quiet.delete(pane);
					onState(pane, "idle");
				}, QUIET_MS),
			);
		},
		/** The pane is gone (exit, close): no idle callback for it later. */
		forget(pane: string) {
			const t = quiet.get(pane);
			if (t !== undefined) timers.clear(t);
			quiet.delete(pane);
		},
	};
}
