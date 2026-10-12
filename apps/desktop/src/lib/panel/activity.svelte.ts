import type { Filter } from "./activity.ts";

/**
 * UI state of the Activity drawer, shared by the header button, the command palette and the
 * drawer itself. `seen` is the time the drawer was last open per project: newer rows are unread.
 */
class ActivityState {
	open = $state(false);
	filter = $state<Filter>("all");
	seen = $state<Record<string, number>>({});
	/** A service's logs were requested at a moment (Logs consumes it once). */
	seek = $state<{ project: string; service: string | null; ts: number } | null>(null);

	show() {
		this.open = true;
	}

	markSeen(project: string) {
		this.seen[project] = Date.now();
	}
}

export const activity = new ActivityState();
