// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { apply, applyBatch, coalesce, emptyChat, LineBuffer } from "./rpc.ts";

const run = (lines: object[]) => lines.reduce((s, l) => apply(s, JSON.stringify(l)), emptyChat());

test("streaming deltas and the final message collapse into one assistant item", () => {
	const s = run([
		{ type: "ready" },
		{
			type: "message_end",
			messageId: "msg-1",
			message: { role: "user", content: [{ type: "text", text: "hi" }] },
		},
		{ type: "agent_start" },
		{
			type: "message_update",
			assistantMessageEvent: { partial: { role: "assistant", content: [{ type: "text", text: "po" }] } },
		},
		{
			type: "message_update",
			assistantMessageEvent: { partial: { role: "assistant", content: [{ type: "text", text: "pong" }] } },
		},
		{
			type: "message_end",
			messageId: "msg-3",
			message: { role: "assistant", content: [{ type: "text", text: "pong" }] },
		},
		{ type: "agent_end" },
	]);
	expect(s.items.map((i) => [i.kind, i.text])).toEqual([
		["user", "hi"],
		["assistant", "pong"],
	]);
	expect(s.items[1].streaming).toBe(false);
	expect(s.busy).toBe(false);
});

test("tool results, dialogs and garbage lines", () => {
	let s = run([
		{ type: "tool_execution_start", toolCallId: "t1", toolName: "bash", args: { command: "echo hi" } },
		{
			type: "tool_execution_end",
			toolCallId: "t1",
			toolName: "bash",
			result: { content: [{ type: "text", text: "hi" }] },
			isError: false,
		},
		{ type: "extension_ui_request", id: "u1", method: "select", title: "Pick", options: ["a", "b"] },
		{ type: "extension_ui_request", id: "u2", method: "setWidget", widgetKey: "x" },
	]);
	s = apply(s, "not json");
	expect(s.tools.t1).toMatchObject({ status: "done", result: "hi" });
	expect(s.ui).toEqual([
		{
			id: "u1",
			method: "select",
			title: "Pick",
			options: ["a", "b"],
			message: undefined,
			placeholder: undefined,
		},
	]);
});

test("line buffer keeps partial lines across chunks", () => {
	const b = new LineBuffer();
	expect(b.push('{"a":1}\n{"b"')).toEqual(['{"a":1}']);
	expect(b.push(":2}\n")).toEqual(['{"b":2}']);
});

test("a replayed dialog disappears once its tool call ended", () => {
	const s = run([
		{ type: "extension_ui_request", id: "u1", method: "select", title: "Pick", options: ["a"] },
		{ type: "tool_execution_end", toolCallId: "t1", toolName: "ask", result: "a", isError: false },
	]);
	expect(s.ui).toEqual([]);
});

test("get_messages seeds a resumed transcript once", () => {
	const hist = {
		type: "response",
		command: "get_messages",
		success: true,
		data: {
			messages: [
				{ role: "user", content: [{ type: "text", text: "old q" }] },
				{ role: "assistant", content: [{ type: "text", text: "old a" }] },
			],
		},
	};
	const s = run([hist, hist]);
	expect(s.items.map((i) => i.text)).toEqual(["old q", "old a"]);
	expect(s.items[1].streaming).toBe(false);
});

// ── streaming: one apply per batch (#289, docs/perf/linux-webkit-study.md) ──
// omp sends the whole message so far in every `message_update`, so a batch of N deltas used to cost N JSON parses, N item
// replacements and N re-renders of the transcript for a result only the last one decides.
const line = (o: object) => JSON.stringify(o);
const delta = (text: string) =>
	line({
		type: "message_update",
		assistantMessageEvent: { partial: { role: "assistant", content: [{ type: "text", text }] } },
	});
const end = (text: string, messageId = "a1") =>
	line({ type: "message_end", messageId, message: { role: "assistant", content: [{ type: "text", text }] } });
const toolStart = (id: string) =>
	line({ type: "tool_execution_start", toolCallId: id, toolName: "bash", args: {} });
const toolEnd = (id: string) =>
	line({ type: "tool_execution_end", toolCallId: id, toolName: "bash", result: "ok" });

/** A chat whose `items` counts how many times a slot is assigned (what Svelte turns into a re-render of that row). */
function counted() {
	const writes: number[] = [];
	const s = emptyChat();
	s.items = new Proxy(s.items, {
		set(t, k, v) {
			if (k !== "length") writes.push(Number(k));
			return Reflect.set(t, k, v);
		},
	});
	return { s, writes };
}

test("N cumulative deltas in one batch produce one item update, with the newest text", () => {
	const { s, writes } = counted();
	const batch = Array.from({ length: 30 }, (_, i) => delta("x".repeat(i + 1)));
	applyBatch(s, ['{"type":"agent_start"}', ...batch]);
	expect(writes).toEqual([0]);
	expect(s.items.map((i) => [i.kind, i.text.length])).toEqual([["assistant", 30]]);
	// The same lines one at a time are what it cost before: 30 writes.
	const old = counted();
	for (const l of batch) apply(old.s, l);
	expect(old.writes).toHaveLength(30);
});

test("a batch with a message_end keeps the deltas before it: the end finalizes the item they made", () => {
	const s = applyBatch(emptyChat(), [delta("a"), delta("ab"), end("abc"), delta("x"), delta("xy")]);
	expect(s.items.map((i) => [i.text, i.streaming])).toEqual([
		["abc", false],
		["xy", true],
	]);
});

test("deltas on both sides of tool events merge; deltas on both sides of agent_end do not", () => {
	const merged = counted();
	applyBatch(merged.s, [delta("a"), toolStart("t1"), toolEnd("t1"), delta("ab")]);
	expect(merged.writes).toEqual([0]);
	expect(merged.s.tools.t1.status).toBe("done");
	// agent_end closes the streaming item: the next delta starts a NEW assistant item, so the first one must exist.
	const split = applyBatch(emptyChat(), [delta("a"), line({ type: "agent_end" }), delta("b")]);
	expect(split.items.map((i) => [i.text, i.streaming])).toEqual([
		["a", false],
		["b", true],
	]);
});

test("applyBatch gives the same state as applying every line, whatever the batch", () => {
	const pool = (n: number) => [
		delta("t".repeat(n)),
		delta("t".repeat(n + 1)),
		end("t".repeat(n), `m${n}`),
		toolStart(`t${n}`),
		toolEnd(`t${n}`),
		line({ type: "agent_start" }),
		line({ type: "agent_end" }),
		line({ type: "ready" }),
		"not json at all",
		line({ type: "message_end", messageId: `u${n}`, message: { role: "user", content: "hi" } }),
	];
	let seed = 7;
	const rnd = (k: number) => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff), seed % k);
	for (let round = 0; round < 300; round++) {
		const batch = Array.from({ length: 1 + rnd(14) }, () => pool(1 + rnd(5))[rnd(10)]);
		const expected = batch.reduce((st, l) => apply(st, l), emptyChat());
		expect(applyBatch(emptyChat(), batch)).toEqual(expected);
	}
});

test("coalesce leaves a line it cannot classify, and anything not a delta, untouched", () => {
	const odd = '{"assistantMessageEvent":{},"type":"message_update"}'; // `type` not first: never dropped
	// It ends a run (it might be a barrier we cannot see), so the delta before it stays; two deltas with only tool events between merge.
	expect(coalesce([odd, delta("a"), odd, delta("b")])).toEqual([odd, delta("a"), odd, delta("b")]);
	expect(coalesce([delta("a"), toolStart("t"), delta("b")])).toEqual([toolStart("t"), delta("b")]);
	expect(coalesce([])).toEqual([]);
});
