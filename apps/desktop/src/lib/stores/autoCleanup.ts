/** Pure policy of auto clean up (#58); the sweep that applies it is `autoCleanup.svelte.ts`. */
import type { Worktree } from "#lib/stores/workspaceTypes.ts";

export type CleanupMode = "off" | "merged" | "idle";
export interface CleanupPrefs {
	mode: CleanupMode;
	days: number;
}
/** A worktree auto clean up removed; its branch is kept so it can be restored. */
export interface Cleaned {
	repoId: string;
	name: string;
	branch: string;
	at: number;
}

const DAY = 86_400_000;
/** How long a cleaned worktree stays in the restore list. */
export const RESTORE_MS = 7 * DAY;

/** `autoCleanup` / `autoCleanupDays` from settings.json, which has no schema: anything unknown is off / 14 days. */
export function cleanupPrefs(s: Record<string, unknown>): CleanupPrefs {
	const mode = s.autoCleanup === "merged" || s.autoCleanup === "idle" ? s.autoCleanup : "off";
	const d = Number(s.autoCleanupDays);
	return { mode, days: Number.isFinite(d) && d > 0 ? Math.min(365, Math.max(1, Math.round(d))) : 14 };
}

/**
 * Worktrees a sweep looks at: linked, not pinned, not the one on screen, no running stack (`busy`), and in idle
 * mode N days without agent activity. Merged mode is decided by Rust, which asks `gh`.
 */
export function candidates(
	list: Worktree[],
	prefs: CleanupPrefs,
	selected: string | null,
	now: number,
	busy: ReadonlySet<string> = new Set(),
) {
	if (prefs.mode === "off") return [];
	return list.filter(
		(w) =>
			!w.isMain &&
			!w.pinned &&
			w.id !== selected &&
			!busy.has(w.id) &&
			(prefs.mode === "merged" || now - w.lastActivityAt >= prefs.days * DAY),
	);
}

/** The restore list, without entries older than 7 days or malformed ones. */
export function restorable(raw: unknown, now: number): Cleaned[] {
	if (!Array.isArray(raw)) return [];
	return (raw as unknown[]).filter((v): v is Cleaned => {
		const c = (v ?? {}) as Partial<Cleaned>;
		return (
			typeof c.repoId === "string" &&
			typeof c.name === "string" &&
			typeof c.branch === "string" &&
			typeof c.at === "number" &&
			now - c.at < RESTORE_MS
		);
	});
}
