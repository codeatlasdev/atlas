/**
 * Pure helpers for per-field autosave in CrewmateSettingsDialog.
 *
 * Non-trivial case: on save error the savedRef must NOT advance to the
 * attempted value, so the field stays dirty and the user can retry.  A naive
 * implementation that copies `current` into `savedRef` on every blur would
 * silently mark an un-persisted value as "saved" and never retry.
 */

/** True when `current` differs from the last successfully persisted value. */
export function isDirty(current: string, lastSaved: string): boolean {
	return current !== lastSaved;
}

/**
 * Returns the new saved ref after a save attempt.
 *
 * - On success: advances to `attempted` (the value that reached the server).
 * - On failure: stays at `oldSaved` so the field remains dirty for retry.
 */
export function updateSavedRef(oldSaved: string, attempted: string, success: boolean): string {
	return success ? attempted : oldSaved;
}
