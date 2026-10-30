/**
 * Pure logic for the Crew Board view.
 * `boardColumns(entries, now)` maps flat task entries to the four columns,
 * sorted and filtered — testable without any DOM or Svelte.
 */
import type { Bot, BotTask } from "#lib/bots.ts";

// ─── types ────────────────────────────────────────────────────────────────────

export interface BoardEntry {
	hostKey: string;
	bot: Bot;
	task: BotTask;
}

export interface BoardCard extends BoardEntry {
	/** The agent's name for the work ("STORE: Explicar scripts"); the card's heading when set. */
	title?: string;
	/** Task ended in an error. */
	failed?: boolean;
	/** Task was cancelled by the user. */
	cancelled?: boolean;
}

export type ColumnId = "queued" | "working" | "needs_you" | "done";

export interface BoardColumn {
	id: ColumnId;
	label: string;
	cards: BoardCard[];
}
// ─── filter options ───────────────────────────────────────────────────────────

export interface BoardFilterOptions {
	/** Only show cards from these bots (`${hostKey}\u0000${botId}`). Empty set = show all. */
	bots?: Set<string>;
	/** 'chat' = desktop/mobile only; 'auto' = non-chat only; 'all' / omitted = no filter. */
	source?: "all" | "chat" | "auto";
	/** Case-insensitive substring match on card heading (title ?? prompt). */
	query?: string;
	/** How many days back to show in Done. 1 = calendar today (default); 7 = rolling 7 days. */
	doneDays?: 1 | 7;
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/** True when `ts` (ms) falls within the same calendar day as `now` (local time). */
function isToday(ts: number, now: number): boolean {
	const d = new Date(ts);
	const n = new Date(now);
	return d.getFullYear() === n.getFullYear() && d.getMonth() === n.getMonth() && d.getDate() === n.getDate();
}

/** True when `ts` is within the last `days` × 24 h from `now` (rolling window). */
function isWithinDays(ts: number, now: number, days: number): boolean {
	return now - ts <= days * 24 * 60 * 60 * 1000;
}

/** The timestamp to use for "when did this task end" — prefer finished_at over ts. */
function endTs(task: BotTask): number {
	return task.finished_at ?? task.ts;
}

// ─── boardColumns ─────────────────────────────────────────────────────────────

/**
 * Partition `entries` into the four board columns and sort each:
 *   Queued    — oldest first (lowest ts)
 *   Working   — longest-running first (lowest started_at / ts)
 *   Needs you — oldest first (lowest ts)
 *   Done      — newest first (highest finished_at / ts); calendar today by default
 *
 * A card is a task, not a message: the work the agent names itself (`title`), one card per bot and title holding the
 * latest message about it. A finished chat message the agent never named ("oi") is conversation and stays off; an
 * unnamed one still in flight, or one an automation started, is shown under its prompt.
 * A running task whose bot has `pending_approval` goes to Needs you, not Working.
 * failed / cancelled tasks go to Done with a flag; pre-window entries are dropped.
 *
 * `opts.bots`     — only show cards from these bots (`${hostKey}\u0000${botId}`); empty set = all.
 * `opts.source`   — 'chat' = desktop/mobile; 'auto' = non-chat; 'all'/omitted = no filter.
 * `opts.query`    — case-insensitive substring on card heading (title ?? prompt).
 * `opts.doneDays` — 1 = calendar today (default); 7 = rolling 7-day window.
 */
export function boardColumns(entries: BoardEntry[], now: number, opts?: BoardFilterOptions): BoardColumn[] {
	const { bots: botFilter, source: sourceFilter, query, doneDays = 1 } = opts ?? {};
	const q = query?.toLowerCase();

	// ── pre-dedup entry filters ──────────────────────────────────────────────
	let filtered = entries;
	if (botFilter && botFilter.size > 0) {
		filtered = filtered.filter((e) => botFilter.has(`${e.hostKey}\u0000${e.bot.id}`));
	}
	if (sourceFilter === "chat") {
		filtered = filtered.filter((e) => e.task.source === "desktop" || e.task.source === "mobile");
	} else if (sourceFilter === "auto") {
		filtered = filtered.filter((e) => e.task.source !== "desktop" && e.task.source !== "mobile");
	}

	const queued: BoardCard[] = [];
	const working: BoardCard[] = [];
	const needsYou: BoardCard[] = [];
	const done: BoardCard[] = [];

	// ── deduplication ────────────────────────────────────────────────────────
	const latest = new Map<string, BoardCard>();
	for (const e of filtered) {
		const { task } = e;
		const title = task.title?.trim();
		if (!title) {
			const isChat = task.source === "desktop" || task.source === "mobile";
			const finished = task.state === "done" || task.state === "failed" || task.state === "cancelled";
			if (isChat && finished) continue;
			latest.set(`${e.hostKey}\u0000${e.bot.id}\u0000#${task.id}`, e);
			continue;
		}
		const key = `${e.hostKey}\u0000${e.bot.id}\u0000${title}`;
		const prev = latest.get(key);
		if (!prev || task.ts >= prev.task.ts) latest.set(key, { ...e, title });
	}

	// ── column assignment ────────────────────────────────────────────────────
	for (const e of latest.values()) {
		const { task, bot } = e;

		// query filter runs post-dedup so it matches the resolved title
		if (q && !((e as BoardCard).title ?? task.prompt).toLowerCase().includes(q)) continue;

		const inDoneWindow =
			doneDays === 1 ? isToday(endTs(task), now) : isWithinDays(endTs(task), now, doneDays);

		switch (task.state) {
			case "queued":
				queued.push(e);
				break;

			case "running":
				if (bot.pending_approval) {
					needsYou.push(e);
				} else {
					working.push(e);
				}
				break;

			case "waiting_action":
				needsYou.push(e);
				break;

			case "done":
				if (inDoneWindow) done.push(e);
				break;

			case "failed":
				if (inDoneWindow) done.push({ ...e, failed: true });
				break;

			case "cancelled":
				if (inDoneWindow) done.push({ ...e, cancelled: true });
				break;
		}
	}

	queued.sort((a, b) => a.task.ts - b.task.ts);
	working.sort((a, b) => (a.task.started_at ?? a.task.ts) - (b.task.started_at ?? b.task.ts));
	needsYou.sort((a, b) => a.task.ts - b.task.ts);
	done.sort((a, b) => endTs(b.task) - endTs(a.task));

	return [
		{ id: "queued", label: "Queued", cards: queued },
		{ id: "working", label: "Working", cards: working },
		{ id: "needs_you", label: "Needs you", cards: needsYou },
		{ id: "done", label: "Done", cards: done },
	];
}
