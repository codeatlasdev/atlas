/**
 * Which crewmates have replies the user has not seen: a `last seen` timestamp per crewmate (`crewmateKey`),
 * persisted in localStorage under `crew:seen:<serverKey>:<crewmateId>`. Pure functions over a Map and a Storage-like object so the
 * store stays thin and this is testable without a DOM.
 */

import { crewmateKey } from "#lib/crew/crewmateKey.ts";

const PREFIX = "crew:seen:";

/** The subset of `Storage` the bookkeeping needs. */
export interface SeenStorage {
	readonly length: number;
	key(index: number): string | null;
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

/** Read every saved last-seen timestamp. A storage that throws (private mode) yields an empty map. */
export function loadSeen(storage: SeenStorage): Map<string, number> {
	const seen = new Map<string, number>();
	try {
		for (let i = 0; i < storage.length; i++) {
			const k = storage.key(i);
			if (!k?.startsWith(PREFIX)) continue;
			const v = Number(storage.getItem(k));
			// The id holds no colon (a uuid), the server key may (`ssh:user@host`): split at the last one.
			const rest = k.slice(PREFIX.length);
			const cut = rest.lastIndexOf(":");
			if (v > 0 && cut >= 0) seen.set(crewmateKey(rest.slice(0, cut), rest.slice(cut + 1)), v);
		}
	} catch {
		/* private mode */
	}
	return seen;
}

/** Persist one timestamp. A full or unavailable storage is ignored: the in-memory map still has it. */
export function saveSeen(storage: SeenStorage, hostKey: string, crewmateId: string, ts: number): void {
	try {
		storage.setItem(`${PREFIX}${hostKey}:${crewmateId}`, String(ts));
	} catch {
		/* storage full */
	}
}

/** True when the crewmate's last reply (ms) is newer than what the user last saw. */
export function isUnread(lastTs: number | undefined, seenTs: number | undefined): boolean {
	return !!lastTs && lastTs > (seenTs ?? 0);
}
