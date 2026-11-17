/**
 * Reducer for a file changing under an open editor. `reduceExternal` returns the next state and
 * the one effect the integrator must perform; it never touches disk or the view itself.
 *
 * - Our own save echoing back (disk text === `lastSaved`) is ignored.
 * - Disk text === the buffer as it would be written: `in-sync` (the integrator takes it as the baseline).
 * - Clean buffer + auto reload: `reload`. Clean buffer without auto reload, or dirty: `prompt-conflict`.
 * - Deleted on disk: `notice-deleted` (the buffer is kept; saving recreates the file).
 */

export interface ExternalState {
	/** Latest text seen on disk that is not the buffer's (null: in sync or deleted). */
	diskText: string | null;
	bufferText: string;
	/** The text last written to or read from disk by this editor. */
	lastSaved: string;
	mtime: number | null;
	dirty: boolean;
	autoReload: boolean;
}

export type ExternalEvent =
	| { kind: "disk-changed"; text: string; mtime: number }
	| { kind: "disk-deleted" }
	| { kind: "saved"; text: string; mtime: number }
	| { kind: "edited"; text: string }
	| { kind: "resolve"; choice: "reload" | "keep" | "compare" };

export type ExternalEffect =
	"none" | "reload" | "in-sync" | "prompt-conflict" | "notice-deleted" | "ignore-own-write";

export interface ExternalResult {
	state: ExternalState;
	effect: ExternalEffect;
}

/** Next state and effect for `event`. */
export function reduceExternal(state: ExternalState, event: ExternalEvent): ExternalResult {
	switch (event.kind) {
		case "disk-changed": {
			// Our own write coming back through the watcher (or a touch with identical content).
			if (event.text === state.lastSaved) {
				return { state: { ...state, diskText: null, mtime: event.mtime }, effect: "ignore-own-write" };
			}
			if (event.text === state.bufferText) {
				// Someone wrote exactly what we have: in sync, nothing to ask.
				return {
					state: { ...state, diskText: null, lastSaved: event.text, mtime: event.mtime, dirty: false },
					effect: "in-sync",
				};
			}
			const next = { ...state, diskText: event.text, mtime: event.mtime };
			if (!state.dirty && state.autoReload) return { state: next, effect: "reload" };
			return { state: next, effect: "prompt-conflict" };
		}
		case "disk-deleted":
			return { state: { ...state, diskText: null, mtime: null }, effect: "notice-deleted" };
		case "saved":
			return {
				state: {
					...state,
					diskText: null,
					lastSaved: event.text,
					bufferText: event.text,
					mtime: event.mtime,
					dirty: false,
				},
				effect: "none",
			};
		case "edited":
			return {
				state: { ...state, bufferText: event.text, dirty: event.text !== state.lastSaved },
				effect: "none",
			};
		case "resolve": {
			if (state.diskText === null) return { state, effect: "none" };
			if (event.choice === "reload") {
				const text = state.diskText;
				return {
					state: { ...state, diskText: null, bufferText: text, lastSaved: text, dirty: false },
					effect: "reload",
				};
			}
			// `keep` ends the conflict for this disk text (a later save overwrites it); `compare`
			// leaves it open while the integrator shows the diff.
			if (event.choice === "keep") return { state: { ...state, diskText: null }, effect: "none" };
			return { state, effect: "none" };
		}
	}
}
