// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { boardColumns } from "./board.ts";
import type { Bot, BotTask } from "#lib/bots.ts";

// ─── helpers ──────────────────────────────────────────────────────────────────

const NOW = new Date("2026-10-29T12:00:00.000Z").getTime(); // midday today

/** Local midnight for NOW (start of today in local time). */
function localMidnight(ts: number): number {
	const d = new Date(ts);
	d.setHours(0, 0, 0, 0);
	return d.getTime();
}

const TODAY_START = localMidnight(NOW);
const YESTERDAY = TODAY_START - 1; // 1ms before today

function makeBot(overrides: Partial<Bot> = {}): Bot {
	return {
		id: "bot1",
		name: "TestBot",
		role: "",
		agent: "omp",
		approval: "ask",
		state: "idle",
		queued: 0,
		...overrides,
	};
}

function makeTask(overrides: Partial<BotTask> = {}): BotTask {
	return {
		id: "task1",
		ts: NOW - 10_000,
		// Column logic below uses work the bot was given by an automation; what a chat message counts as is tested
		// on its own ("what is a task").
		source: "trigger",
		prompt: "Do something",
		state: "queued",
		...overrides,
	};
}

function entry(task: BotTask, bot?: Partial<Bot>) {
	return { hostKey: "", bot: makeBot(bot), task };
}

// ─── column assignment ─────────────────────────────────────────────────────────

describe("boardColumns: column assignment", () => {
	test("queued task goes to Queued", () => {
		const cols = boardColumns([entry(makeTask({ state: "queued" }))], NOW);
		expect(cols[0].id).toBe("queued");
		expect(cols[0].cards).toHaveLength(1);
		expect(cols[1].cards).toHaveLength(0);
		expect(cols[2].cards).toHaveLength(0);
		expect(cols[3].cards).toHaveLength(0);
	});

	test("running task goes to Working", () => {
		const cols = boardColumns([entry(makeTask({ state: "running" }))], NOW);
		expect(cols[1].id).toBe("working");
		expect(cols[1].cards).toHaveLength(1);
	});

	test("running task whose bot has pending_approval goes to Needs you", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "running" }), { pending_approval: { req: "r", tool_name: "bash", tool_input: {}, pane_key: "p" } })],
			NOW,
		);
		expect(cols[2].id).toBe("needs_you");
		expect(cols[2].cards).toHaveLength(1);
		expect(cols[1].cards).toHaveLength(0);
	});

	test("waiting_action task goes to Needs you", () => {
		const cols = boardColumns([entry(makeTask({ state: "waiting_action" }))], NOW);
		expect(cols[2].id).toBe("needs_you");
		expect(cols[2].cards).toHaveLength(1);
	});

	test("done task finished today goes to Done", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", finished_at: NOW - 3600_000 }))],
			NOW,
		);
		expect(cols[3].id).toBe("done");
		expect(cols[3].cards).toHaveLength(1);
	});

	test("done task finished before today is excluded", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", finished_at: YESTERDAY }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(0);
	});

	test("done task with no finished_at uses ts; today included", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", ts: NOW - 1000 }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(1);
	});

	test("done task with no finished_at uses ts; yesterday excluded", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", ts: YESTERDAY }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(0);
	});

	test("failed task today goes to Done with failed flag", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "failed", finished_at: NOW - 1000 }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(1);
		expect(cols[3].cards[0].failed).toBe(true);
		expect(cols[3].cards[0].cancelled).toBeFalsy();
	});

	test("failed task yesterday is excluded", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "failed", finished_at: YESTERDAY }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(0);
	});

	test("cancelled task today goes to Done with cancelled flag", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "cancelled", finished_at: NOW - 500 }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(1);
		expect(cols[3].cards[0].cancelled).toBe(true);
		expect(cols[3].cards[0].failed).toBeFalsy();
	});

	test("cancelled task yesterday is excluded", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "cancelled", finished_at: YESTERDAY }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(0);
	});
});

// ─── sort orders ──────────────────────────────────────────────────────────────

describe("boardColumns: sort orders", () => {
	test("Queued: oldest first (lowest ts)", () => {
		const old = makeTask({ id: "old", state: "queued", ts: NOW - 9000 });
		const new_ = makeTask({ id: "new", state: "queued", ts: NOW - 1000 });
		const cols = boardColumns([entry(new_), entry(old)], NOW);
		expect(cols[0].cards[0].task.id).toBe("old");
		expect(cols[0].cards[1].task.id).toBe("new");
	});

	test("Working: longest-running first (lowest started_at)", () => {
		const long_ = makeTask({ id: "long", state: "running", started_at: NOW - 60_000 });
		const short_ = makeTask({ id: "short", state: "running", started_at: NOW - 5_000 });
		const cols = boardColumns([entry(short_), entry(long_)], NOW);
		expect(cols[1].cards[0].task.id).toBe("long");
		expect(cols[1].cards[1].task.id).toBe("short");
	});

	test("Working: task without started_at falls back to ts", () => {
		const a = makeTask({ id: "a", state: "running", ts: NOW - 80_000 });
		const b = makeTask({ id: "b", state: "running", ts: NOW - 2_000, started_at: NOW - 1_000 });
		const cols = boardColumns([entry(b), entry(a)], NOW);
		// a has no started_at, uses ts (NOW-80_000) → it ran longer → first
		expect(cols[1].cards[0].task.id).toBe("a");
	});

	test("Needs you: oldest first (lowest ts)", () => {
		const old = makeTask({ id: "old", state: "waiting_action", ts: NOW - 9000 });
		const new_ = makeTask({ id: "new", state: "waiting_action", ts: NOW - 1000 });
		const cols = boardColumns([entry(new_), entry(old)], NOW);
		expect(cols[2].cards[0].task.id).toBe("old");
	});

	test("Done: newest first (highest finished_at)", () => {
		const earlier = makeTask({ id: "early", state: "done", finished_at: NOW - 7200_000 });
		const later = makeTask({ id: "late", state: "done", finished_at: NOW - 600_000 });
		const cols = boardColumns([entry(earlier), entry(later)], NOW);
		expect(cols[3].cards[0].task.id).toBe("late");
		expect(cols[3].cards[1].task.id).toBe("early");
	});
});

// ─── column structure ─────────────────────────────────────────────────────────

describe("boardColumns: structure", () => {
	test("always returns exactly 4 columns in order", () => {
		const cols = boardColumns([], NOW);
		expect(cols).toHaveLength(4);
		expect(cols.map((c) => c.id)).toEqual(["queued", "working", "needs_you", "done"]);
	});

	test("cards carry hostKey and bot reference", () => {
		const cols = boardColumns(
			[{ hostKey: "ssh:atlas-bot@server", bot: makeBot({ id: "b1" }), task: makeTask() }],
			NOW,
		);
		expect(cols[0].cards[0].hostKey).toBe("ssh:atlas-bot@server");
		expect(cols[0].cards[0].bot.id).toBe("b1");
	});

	test("midnight boundary: task at exactly TODAY_START (00:00:00.000) is today", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", finished_at: TODAY_START }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(1);
	});

	test("midnight boundary: task at TODAY_START - 1ms is yesterday", () => {
		const cols = boardColumns(
			[entry(makeTask({ state: "done", finished_at: TODAY_START - 1 }))],
			NOW,
		);
		expect(cols[3].cards).toHaveLength(0);
	});
});

// Incident: the board listed every chat message ("oi", "e agora?", "opaa") as a task. Steps don't tell them apart (a
// bot reads its memory and checks git before any reply), so on the real ship "oi" had 2 steps. A task is the work the
// agent names itself ("STORE: Explicar scripts e api.ts da store"); one card per task, however many messages it took.
describe("what is a task", () => {
	const step = { ts: NOW / 1000, text: "read memory.md" };
	const board = (...tasks) =>
		boardColumns(
			tasks.map((t, i) => entry(makeTask({ id: `t${i}`, ts: NOW - 60_000 + i * 1000, ...t }))),
			NOW,
		);
	const where = (cols) => cols.flatMap((c) => c.cards.map((k) => [c.id, k.title ?? k.task.prompt]));

	test("a message the agent never named as work is not a card, even if it ran steps", () => {
		expect(where(board({ source: "desktop", prompt: "oi", state: "done", steps: [step], finished_at: NOW - 1000 }))).toEqual([]);
	});

	test("a finished task shows the agent's name for the work, not the message that started it", () => {
		const cols = board({ source: "desktop", prompt: "lê o package.json", title: "STORE: Explicar scripts", state: "done", finished_at: NOW - 1000 });
		expect(where(cols)).toEqual([["done", "STORE: Explicar scripts"]]);
	});

	test("messages about the same work are one card, in the state of the latest", () => {
		const cols = board(
			{ source: "desktop", prompt: "corrige o billing", title: "BILL: Wait on billing fix", state: "done", finished_at: NOW - 5000 },
			{ source: "desktop", prompt: "e agora?", title: "BILL: Wait on billing fix", state: "running", steps: [step] },
		);
		expect(where(cols)).toEqual([["working", "BILL: Wait on billing fix"]]);
	});

	test("the same title on two bots is two tasks", () => {
		const a = entry(makeTask({ id: "a", title: "PR: Review", state: "running" }), { id: "b1" });
		const b = entry(makeTask({ id: "b", title: "PR: Review", state: "running" }), { id: "b2" });
		expect(boardColumns([a, b], NOW)[1].cards).toHaveLength(2);
	});

	test("work in flight is shown before the agent names it: queued, running, or waiting on you", () => {
		expect(where(board({ source: "desktop", prompt: "faz o deploy", state: "queued" }))).toEqual([["queued", "faz o deploy"]]);
		expect(where(board({ source: "desktop", prompt: "faz o deploy", state: "running" }))).toEqual([["working", "faz o deploy"]]);
		expect(where(board({ source: "desktop", prompt: "x", state: "waiting_action" }))).toEqual([["needs_you", "x"]]);
	});

	test("an automation run is a task even when the agent gave it no name", () => {
		expect(where(board({ source: "trigger", prompt: "Revise os PRs", state: "done", finished_at: NOW - 1000 }))).toEqual([
			["done", "Revise os PRs"],
		]);
	});
});
// ─── filters ──────────────────────────────────────────────────────────────────

describe("boardColumns: filters", () => {
	// ─── bot filter ────────────────────────────────────────────────────────────
	test("bot filter: only cards from selected bot are shown", () => {
		const a = { hostKey: "h1", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", state: "queued" }) };
		const b = { hostKey: "h1", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", state: "queued" }) };
		const cols = boardColumns([a, b], NOW, { bots: new Set(["h1\u0000b1"]) });
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(1);
		expect(cards[0].bot.id).toBe("b1");
	});

	test("bot filter: empty Set shows all", () => {
		const a = { hostKey: "h1", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", state: "queued" }) };
		const b = { hostKey: "h1", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", state: "queued" }) };
		const cols = boardColumns([a, b], NOW, { bots: new Set() });
		expect(cols.flatMap((c) => c.cards)).toHaveLength(2);
	});

	test("bot filter: multiple bots selected", () => {
		const a = { hostKey: "h1", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", state: "queued" }) };
		const b = { hostKey: "h1", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", state: "queued" }) };
		const c = { hostKey: "h1", bot: makeBot({ id: "b3" }), task: makeTask({ id: "t3", state: "queued" }) };
		const cols = boardColumns([a, b, c], NOW, { bots: new Set(["h1\u0000b1", "h1\u0000b3"]) });
		const cards = cols.flatMap((col) => col.cards);
		expect(cards).toHaveLength(2);
		expect(cards.map((card) => card.bot.id).sort()).toEqual(["b1", "b3"]);
	});

	// ─── source filter ─────────────────────────────────────────────────────────
	test("source 'chat': only desktop/mobile tasks", () => {
		// titled running tasks so neither is dropped by the unnamed-chat-finished guard
		const chat = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", source: "desktop", state: "running", title: "Chat work" }) };
		const auto = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", source: "trigger", state: "running" }) };
		const cols = boardColumns([chat, auto], NOW, { source: "chat" });
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(1);
		expect(cards[0].task.source).toBe("desktop");
	});

	test("source 'auto': only non-chat tasks (trigger, routine, …)", () => {
		const chat = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", source: "desktop", state: "running", title: "Chat work" }) };
		const auto1 = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", source: "trigger", state: "running" }) };
		const auto2 = { hostKey: "", bot: makeBot({ id: "b3" }), task: makeTask({ id: "t3", source: "routine", state: "running" }) };
		const cols = boardColumns([chat, auto1, auto2], NOW, { source: "auto" });
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(2);
		expect(cards.every((card) => card.task.source !== "desktop" && card.task.source !== "mobile")).toBe(true);
	});

	test("source 'all': no filtering applied", () => {
		const entries = [
			{ hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", source: "desktop", state: "running", title: "A" }) },
			{ hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", source: "trigger", state: "running" }) },
		];
		const cols = boardColumns(entries, NOW, { source: "all" });
		expect(cols.flatMap((c) => c.cards)).toHaveLength(2);
	});

	// ─── query filter ──────────────────────────────────────────────────────────
	test("query: matches title case-insensitively", () => {
		const a = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", state: "queued", title: "STORE: Deploy scripts", prompt: "deploy" }) };
		const b = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", state: "queued", title: "BILL: Fix billing", prompt: "billing" }) };
		const cols = boardColumns([a, b], NOW, { query: "store" });
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(1);
		expect(cards[0].title).toBe("STORE: Deploy scripts");
	});

	test("query: matches prompt when no title", () => {
		// unnamed trigger tasks stay visible; two different bots so dedup keeps both
		const a = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", source: "trigger", state: "queued", prompt: "Fix the database" }) };
		const b = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", source: "trigger", state: "queued", prompt: "Review the PR" }) };
		const cols = boardColumns([a, b], NOW, { query: "database" });
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(1);
		expect(cards[0].task.prompt).toBe("Fix the database");
	});

	test("query: no match returns empty columns", () => {
		const a = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", state: "queued", title: "STORE: Deploy" }) };
		const cols = boardColumns([a], NOW, { query: "xxxxxxnotfound" });
		expect(cols.flatMap((c) => c.cards)).toHaveLength(0);
	});

	// ─── doneDays ──────────────────────────────────────────────────────────────
	test("doneDays 7: includes a task finished 6 days ago", () => {
		const SIX_DAYS_AGO = NOW - 6 * 24 * 60 * 60 * 1000;
		const cols = boardColumns([entry(makeTask({ state: "done", finished_at: SIX_DAYS_AGO }))], NOW, { doneDays: 7 });
		expect(cols[3].cards).toHaveLength(1);
	});

	test("doneDays 7: excludes a task finished 8 days ago", () => {
		const EIGHT_DAYS_AGO = NOW - 8 * 24 * 60 * 60 * 1000;
		const cols = boardColumns([entry(makeTask({ state: "done", finished_at: EIGHT_DAYS_AGO }))], NOW, { doneDays: 7 });
		expect(cols[3].cards).toHaveLength(0);
	});

	test("doneDays 7: still includes a task from today", () => {
		const cols = boardColumns([entry(makeTask({ state: "done", finished_at: NOW - 3_600_000 }))], NOW, { doneDays: 7 });
		expect(cols[3].cards).toHaveLength(1);
	});

	test("doneDays 1 (default): task at YESTERDAY is excluded even if within rolling 24h", () => {
		const cols = boardColumns([entry(makeTask({ state: "done", finished_at: YESTERDAY }))], NOW, { doneDays: 1 });
		expect(cols[3].cards).toHaveLength(0);
	});

	// ─── combined ──────────────────────────────────────────────────────────────
	test("combined bot + source: intersection", () => {
		const b1chat = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t1", source: "desktop", state: "running", title: "Chat b1" }) };
		const b1auto = { hostKey: "", bot: makeBot({ id: "b1" }), task: makeTask({ id: "t2", source: "trigger", state: "running" }) };
		const b2chat = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t3", source: "desktop", state: "running", title: "Chat b2" }) };
		const cols = boardColumns([b1chat, b1auto, b2chat], NOW, {
			bots: new Set(["\u0000b1"]),
			source: "chat",
		});
		const cards = cols.flatMap((c) => c.cards);
		expect(cards).toHaveLength(1);
		expect(cards[0].bot.id).toBe("b1");
		expect(cards[0].task.source).toBe("desktop");
	});

	test("combined query + doneDays 7: both filters applied", () => {
		const SIX_DAYS_AGO = NOW - 6 * 24 * 60 * 60 * 1000;
		const old = entry(makeTask({ state: "done", finished_at: SIX_DAYS_AGO, title: "STORE: Old task" }));
		const recent = { hostKey: "", bot: makeBot({ id: "b2" }), task: makeTask({ id: "t2", state: "done", finished_at: NOW - 3_600_000, title: "BILL: Recent" }) };
		const cols = boardColumns([old, recent], NOW, { doneDays: 7, query: "store" });
		expect(cols[3].cards).toHaveLength(1);
		expect(cols[3].cards[0].title).toBe("STORE: Old task");
	});
});
