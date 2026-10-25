/**
 * Which tabs of a group are mounted. A tab that was mounted stays mounted: unmounting a hidden editor, diff or logs
 * tree costs as much as mounting it (a 400-line diff is ~15k nodes), and the old rule did both on every space switch.
 * Leaving a space dropped every tab but the active one; coming back mounted them all in one pass inside the switch frame.
 * Pure.
 */

/** The active tab plus every tab already shown (the fill-in adds the rest one at a time, see `fillStep`). */
export function mountedTabs({
	tabs,
	active,
	seen,
}: {
	/** Tab ids in strip order. */
	tabs: string[];
	active: string | undefined;
	/** Tabs shown, or filled in, at least once. */
	seen: Set<string>;
}): Set<string> {
	return new Set(tabs.filter((t) => t === active || seen.has(t)));
}

/**
 * Should the fill-in take another step: only a current space fills in, and only while a tab is still unmounted.
 * It compares what is mounted, not a counter, so a counter left over from an earlier visit cannot end it early.
 */
export function fillStep({
	tabs,
	mounted,
	current,
}: {
	tabs: string[];
	mounted: Set<string>;
	current: boolean;
}): boolean {
	return current && tabs.some((t) => !mounted.has(t));
}
