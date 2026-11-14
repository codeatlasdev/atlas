/** Board card text (pure): elapsed time, finish time and the "needs you" line of a card. */
import type { BoardCard } from "#lib/crew/board.ts";

/** Elapsed ms → "1m 23s" style label. */
export function elapsed(startedAt: number | undefined, n: number): string {
	const ms = n - (startedAt ?? n);
	if (ms < 0) return "";
	const s = Math.floor(ms / 1000);
	if (s < 60) return `${s}s`;
	const m = Math.floor(s / 60);
	const rem = s % 60;
	return rem === 0 ? `${m}m` : `${m}m ${rem}s`;
}

/** Format a finish timestamp as a short time string. */
export function finishedLabel(ts: number | undefined): string {
	if (!ts) return "";
	return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Action kind → human readable short label. */
export function actionLabel(card: BoardCard): string {
	const a = card.task.action;
	if (!a) return card.crewmate.pending_approval ? "Wants approval" : "Needs input";
	switch (a.kind) {
		case "input":
			return a.label;
		case "choose":
			return a.question;
		case "connect_github":
			return "Connect GitHub";
		case "provide_secret":
			return a.label;
		case "sign_in_engine":
			return `Sign in to ${a.engine}`;
		case "add_trigger":
			return "Add trigger";
		default:
			return "Needs input";
	}
}
