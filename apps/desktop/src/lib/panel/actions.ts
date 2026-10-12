/** Pure model of the header Actions menu: grouping, availability and the reason when disabled. */

export interface ActionLike {
	key: string;
	label: string;
	group: string | null;
	confirm: string | null;
	/** Interactive sessions (`session:` / `platform:`) live in the TUI, not here. */
	session: boolean;
}

export interface ActionItem {
	key: string;
	label: string;
	/** `confirm:` text: the action is destructive and asks first. */
	confirm: string | null;
	busy: boolean;
	/** Why it cannot run now; null = runnable. */
	disabled: string | null;
}

export interface ActionGroup {
	label: string;
	items: ActionItem[];
}

/** Why `a` cannot run right now, or null. */
export function unavailable(a: ActionLike, running: boolean, busy: boolean): string | null {
	if (a.session) return "Interactive session: run it from the terminal UI (atlas dev)";
	if (!running) return "Start the stack to run actions";
	if (busy) return "Already running";
	return null;
}

/** Ungrouped actions first, then each group in order of first appearance (same as the TUI help). */
export function menuGroups(
	actions: ActionLike[],
	running: boolean,
	busy: (key: string) => boolean,
): ActionGroup[] {
	const by: Record<string, ActionItem[]> = {};
	const order: string[] = [];
	for (const a of actions) {
		const g = a.group ?? "";
		if (!(g in by)) {
			by[g] = [];
			order.push(g);
		}
		const b = busy(a.key);
		by[g].push({
			key: a.key,
			label: a.label,
			confirm: a.confirm,
			busy: b,
			disabled: unavailable(a, running, b),
		});
	}
	order.sort((x, y) => Number(x !== "") - Number(y !== ""));
	return order.map((label) => ({ label, items: by[label] }));
}

/** What the button itself shows: the running action's label, else the idle caption. */
export function buttonState(items: ActionItem[]): { busy: ActionItem | null; label: string } {
	const busy = items.find((i) => i.busy) ?? null;
	return { busy, label: busy ? busy.label : "Actions" };
}
