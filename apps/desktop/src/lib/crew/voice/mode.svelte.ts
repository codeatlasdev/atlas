/** How the microphone key works on this computer: hold to talk (default) or press to start and press to stop. */
import type { KeyMode } from "./dictation.svelte.ts";

const KEY = "atlas.dictationKeyMode";

/** The saved mode; anything but "tap" is the default. */
export function readMode(store: Pick<Storage, "getItem">): KeyMode {
	return store.getItem(KEY) === "tap" ? "tap" : "hold";
}

export const dictationKey = $state<{ mode: KeyMode }>({ mode: readMode(localStorage) });

export function setMode(mode: KeyMode): void {
	dictationKey.mode = mode;
	localStorage.setItem(KEY, mode);
}
