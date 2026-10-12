// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { apply, emptyChat, LineBuffer } from "./rpc.ts";

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
