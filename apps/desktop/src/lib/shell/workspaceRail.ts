/** Pure functions for rail show/hide decision — testable without DOM. */

/** Returns true when the mouse x-coordinate is inside the left-edge hot zone. */
export function inHotZone(mouseX: number, hotZonePx = 6): boolean {
	return mouseX >= 0 && mouseX <= hotZonePx;
}

/** Returns true when the mouse has left the rail (rail width + small buffer). */
export function leftRail(mouseX: number, railWidthPx = 56): boolean {
	return mouseX > railWidthPx;
}

/** Whether cmd-shift-b / ctrl-shift-b should toggle the rail (never inside a focused input). */
export function railToggleKey(e: {
	key: string;
	shiftKey: boolean;
	metaKey: boolean;
	ctrlKey: boolean;
	target: EventTarget | null;
}): boolean {
	const el = e.target as HTMLElement | null;
	const inInput = el?.tagName === "INPUT" || el?.tagName === "TEXTAREA" || el?.isContentEditable;
	return (e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "B" && !inInput;
}
