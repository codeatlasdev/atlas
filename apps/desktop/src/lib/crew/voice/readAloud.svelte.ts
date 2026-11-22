/**
 * The "Read replies aloud" toggle: per device (it is saved in this app's storage, never on the crew server) and per
 * crewmate, off by default. Turning it on in a chat also asks the crewmate to write a short `spoken:` line
 * (`spoken_replies`, see `ChatState.setReadAloud`); turning it off only silences this device.
 */

const KEY = "atlas.readAloud";

/** The saved set of `crewmateKey`s; anything unreadable is the empty set (off). */
export function readSet(store: Pick<Storage, "getItem">): Set<string> {
	try {
		const raw = JSON.parse(store.getItem(KEY) ?? "[]");
		return new Set(Array.isArray(raw) ? raw.filter((k): k is string => typeof k === "string") : []);
	} catch {
		return new Set();
	}
}

class ReadAloud {
	private keys = $state<Set<string>>(readSet(localStorage));

	isOn(key: string): boolean {
		return this.keys.has(key);
	}

	set(key: string, on: boolean): void {
		const next = new Set(this.keys);
		if (on) next.add(key);
		else next.delete(key);
		this.keys = next;
		localStorage.setItem(KEY, JSON.stringify([...next]));
	}
}

export const readAloud = new ReadAloud();
