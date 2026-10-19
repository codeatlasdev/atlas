/**
 * Keyboard map of the Logs body: ⌘F/⌘G search, vim-style scrolling (j/k/g/G), n/N, e/w jumps,
 * f follow, t timestamps. Returns whether the key was handled so the caller can swallow it.
 */

import type { Level } from "./model.ts";
import { ROW } from "./viewport.svelte.ts";
import { otherKey, primaryKey } from "#lib/keys.ts";

export interface LogsKeyActions {
	/** The scrollable body element and its height. */
	viewport: HTMLElement | undefined;
	height: number;
	focusSearch(): void;
	step(dir: 1 | -1): void;
	level(dir: 1 | -1, min: Level): void;
	toBottom(): void;
	setFollow(on: boolean): void;
	toggleFollow(): void;
	toggleTime(): void;
	/** ⎋: clear the query, else the current match. */
	escape(): void;
}

export function handleLogsKey(e: KeyboardEvent, a: LogsKeyActions) {
	if (primaryKey(e) && !e.shiftKey && !e.altKey && !otherKey(e) && e.key === "f") {
		e.preventDefault();
		e.stopPropagation();
		return a.focusSearch();
	}
	if (primaryKey(e) && !e.shiftKey && !e.altKey && e.key === "g") {
		e.preventDefault();
		return a.step(1);
	}
	if (primaryKey(e) && e.shiftKey && e.key.toLowerCase() === "g") {
		e.preventDefault();
		return a.step(-1);
	}
	const vp = a.viewport;
	if (e.target !== vp || e.metaKey || e.ctrlKey || e.altKey || !vp) return;
	const page = Math.max(ROW, a.height - ROW * 2);
	switch (e.key) {
		case "j":
		case "ArrowDown":
			vp.scrollTop += ROW * (e.shiftKey ? 8 : 1);
			break;
		case "k":
		case "ArrowUp":
			vp.scrollTop -= ROW * (e.shiftKey ? 8 : 1);
			break;
		case " ":
		case "PageDown":
			vp.scrollTop += page;
			break;
		case "PageUp":
			vp.scrollTop -= page;
			break;
		case "g":
		case "Home":
			a.setFollow(false);
			vp.scrollTop = 0;
			break;
		case "G":
		case "End":
			a.toBottom();
			break;
		case "n":
			a.step(e.shiftKey ? -1 : 1);
			break;
		case "N":
			a.step(-1);
			break;
		case "e":
			a.level(e.shiftKey ? -1 : 1, "error");
			break;
		case "E":
			a.level(-1, "error");
			break;
		case "w":
			a.level(e.shiftKey ? -1 : 1, "warn");
			break;
		case "W":
			a.level(-1, "warn");
			break;
		case "/":
			a.focusSearch();
			break;
		case "f":
			a.toggleFollow();
			break;
		case "t":
			a.toggleTime();
			break;
		case "Escape":
			a.escape();
			break;
		default:
			return;
	}
	e.preventDefault();
	e.stopPropagation();
}
