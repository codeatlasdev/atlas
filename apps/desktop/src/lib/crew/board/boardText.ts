/** Board card text (pure): elapsed time, finish time and the "needs you" line of a card. */
import type { BoardCard } from "#lib/crew/board.ts";
import { dateTime } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

/** hostd's `waiting_for` while every agent slot of the crew server is taken (`BUSY`, runner/slots.rs). */
const SERVER_BUSY = "server busy";

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

/**
 * The line of a queued task: a bare "Queued", or why it is not starting. `waiting_for` is hostd's own word for the
 * reason (`BUSY` in crates/atlas-host/src/crew/runner/slots.rs); a reason this build does not know shows as sent.
 */
export function queuedLabel(task: { waiting_for?: string }): string {
	if (!task.waiting_for) return m.crew_workspace_board_queued();
	if (task.waiting_for === SERVER_BUSY) return m.crew_workspace_board_queued_server_busy();
	return m.crew_workspace_board_queued_reason({ reason: task.waiting_for });
}

/** Format a finish timestamp as a short time string. */
export function finishedLabel(ts: number | undefined): string {
	if (!ts) return "";
	return dateTime(ts, { hour: "2-digit", minute: "2-digit" });
}

/** Action kind → human readable short label. */
export function actionLabel(card: BoardCard): string {
	const a = card.task.action;
	if (!a)
		return card.crewmate.pending_approval
			? m.crew_workspace_board_wants_approval()
			: m.crew_workspace_board_needs_input();
	switch (a.kind) {
		case "input":
			return a.label;
		case "choose":
			return a.question;
		case "connect_github":
			return m.crew_workspace_board_connect_github();
		case "provide_secret":
			return a.label;
		case "sign_in_engine":
			return m.crew_chat_task_action_card_sign_in_to_engine({ engine: a.engine });
		case "add_trigger":
			return m.crew_workspace_board_add_trigger();
		case "refused":
			return m.crew_workspace_board_work_refused();
		default:
			return m.crew_workspace_board_needs_input();
	}
}
