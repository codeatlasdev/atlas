// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, describe, expect, mock, test } from "bun:test";

const calls: [string, unknown][] = [];
let replies: Record<string, unknown> = {};
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (name: string, args?: unknown) => {
		calls.push([name, args]);
		const r = replies[name];
		if (r instanceof Error) throw r;
		return r;
	},
}));

// Dynamic import on purpose: `mock.module` must be installed before `history.ts` loads and binds `invoke`.
const { loadHistory, remove, reveal, open } = await import("./history.ts");

const e = (at, p = `/s/${at}.png`) => ({
	path: p,
	kind: "screenshot",
	at,
	width: 1,
	height: 1,
	bytes: 1,
	secs: null,
});

beforeEach(() => {
	calls.length = 0;
	replies = {};
});

describe("loadHistory", () => {
	test("calls shots_history with no args and returns the entries newest first, whatever Rust sent", async () => {
		replies.shots_history = { entries: [e(1), e(3), e(2)] };
		const got = await loadHistory();
		expect(calls).toEqual([["shots_history", undefined]]);
		expect(got.map((x) => x.at)).toEqual([3, 2, 1]);
	});
	test("does not mutate what Rust returned", async () => {
		const payload = { entries: [e(1), e(2)] };
		replies.shots_history = payload;
		await loadHistory();
		expect(payload.entries.map((x) => x.at)).toEqual([1, 2]);
	});
	test("equal timestamps keep Rust's order (stable)", async () => {
		replies.shots_history = { entries: [e(5, "/a"), e(5, "/b"), e(5, "/c")] };
		expect((await loadHistory()).map((x) => x.path)).toEqual(["/a", "/b", "/c"]);
	});
	test("an empty history, and a missing entries field, is []", async () => {
		replies.shots_history = { entries: [] };
		expect(await loadHistory()).toEqual([]);
		replies.shots_history = {};
		expect(await loadHistory()).toEqual([]);
	});
	test("a rejection propagates so the page can show the text", async () => {
		replies.shots_history = new Error("boom");
		await expect(loadHistory()).rejects.toThrow("boom");
	});
});

describe("remove / reveal / open", () => {
	test("each calls its own command with {path}", async () => {
		await remove("/a.png");
		await reveal("/b.png");
		await open("/c.png");
		expect(calls).toEqual([
			["shots_history_remove", { path: "/a.png" }],
			["shots_history_reveal", { path: "/b.png" }],
			["shots_history_open", { path: "/c.png" }],
		]);
	});
	test("a rejection propagates", async () => {
		replies.shots_history_open = new Error("gone");
		await expect(open("/x")).rejects.toThrow("gone");
	});
});
