/**
 * The first-run label next to an edge handle ("Workspaces Ctrl+Shift+B"). It names a hover target for someone who has
 * never opened it, so it is shown for a few seconds and then gone for good: the 1.5.0-alpha.26 first-run pass saw it
 * sit over the sidebar on every screen until the rail was opened, which a new user may never do.
 */
export const EDGE_HINT_MS = 8000;

type Store = Pick<Storage, "getItem" | "setItem">;

/** Whether the hint under `key` was never shown to the end or used. */
export function hintPending(store: Store | undefined, key: string): boolean {
	try {
		return store?.getItem(key) !== "1";
	} catch {
		return true;
	}
}

/** Remember it was seen; storage that is full or blocked only means it may show once more. */
export function markHintSeen(store: Store | undefined, key: string): void {
	try {
		store?.setItem(key, "1");
	} catch {
		/* storage full or blocked */
	}
}

/** Hide the hint after `ms` and mark it seen. Returns the cancel (the component calls it on unmount). */
export function expireHint(
	store: Store | undefined,
	key: string,
	hide: () => void,
	ms = EDGE_HINT_MS,
): () => void {
	const t = setTimeout(() => {
		markHintSeen(store, key);
		hide();
	}, ms);
	return () => clearTimeout(t);
}
