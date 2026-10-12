/**
 * omp `--mode rpc-ui` client state: one JSON object per stdout line in, a
 * transcript out. Pure (no IO) so a replay of the whole stdout rebuilds it.
 */

export interface ToolCall {
	id: string;
	name: string;
	args: unknown;
	status: "running" | "done" | "error";
	result?: string;
}

export type Item =
	| { kind: "user"; id: string; text: string; images: number }
	| { kind: "assistant"; id: string; text: string; thinking: string; tools: string[]; streaming: boolean }
	| { kind: "note"; id: string; text: string }
	| { kind: "error"; id: string; text: string };

export interface UiRequest {
	id: string;
	method: "select" | "confirm" | "input" | "editor";
	title: string;
	message?: string;
	options?: string[];
	placeholder?: string;
}

export interface Command {
	name: string;
	description?: string;
}

export interface ChatState {
	ready: boolean;
	items: Item[];
	tools: Record<string, ToolCall>;
	ui: UiRequest[];
	commands: Command[];
	busy: boolean;
	status: string;
	model: string;
	thinking: string;
	thinkingLevels: string[];
	contextPercent: number | null;
}

export const emptyChat = (): ChatState => ({
	ready: false,
	items: [],
	tools: {},
	ui: [],
	commands: [],
	busy: false,
	status: "",
	model: "",
	thinking: "",
	thinkingLevels: [],
	contextPercent: null,
});

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const asStr = (v: unknown) => (typeof v === "string" ? v : "");

function textOf(content: unknown, type: "text" | "thinking" = "text"): string {
	if (typeof content === "string") return type === "text" ? content : "";
	if (!Array.isArray(content)) return "";
	return content
		.filter((c): c is Obj => isObj(c) && c.type === type)
		.map((c) => asStr(type === "text" ? c.text : c.thinking))
		.join("");
}

function toolIds(content: unknown): string[] {
	return Array.isArray(content)
		? content.filter((c): c is Obj => isObj(c) && c.type === "toolCall").map((c) => asStr(c.id))
		: [];
}

function resultText(r: unknown): string {
	if (typeof r === "string") return r;
	if (isObj(r) && Array.isArray(r.content)) return textOf(r.content);
	return r === undefined ? "" : JSON.stringify(r);
}

function upsert(s: ChatState, item: Item) {
	const i = s.items.findIndex((x) => x.id === item.id);
	if (i < 0) s.items.push(item);
	else s.items[i] = item;
}

/** Apply one stdout line; non-JSON lines are ignored. Mutates and returns `s`. */
export function apply(s: ChatState, line: string): ChatState {
	let e: unknown;
	try {
		e = JSON.parse(line);
	} catch {
		return s;
	}
	if (!isObj(e)) return s;
	switch (e.type) {
		case "ready":
			s.ready = true;
			break;
		case "available_commands_update":
			if (Array.isArray(e.commands))
				s.commands = e.commands
					.filter(isObj)
					.map((c) => ({ name: asStr(c.name), description: asStr(c.description) }));
			break;
		case "agent_start":
			s.busy = true;
			break;
		case "agent_end":
			s.busy = false;
			s.ui = [];
			for (const it of s.items) if (it.kind === "assistant") it.streaming = false;
			break;
		case "message_update":
		case "message_end": {
			const m =
				e.type === "message_end"
					? e.message
					: isObj(e.assistantMessageEvent)
						? e.assistantMessageEvent.partial
						: null;
			if (!isObj(m)) break;
			const id = asStr(e.messageId) || `m${s.items.length}`;
			if (m.role === "user") {
				const images = Array.isArray(m.content)
					? m.content.filter((c) => isObj(c) && c.type === "image").length
					: 0;
				upsert(s, { kind: "user", id, text: textOf(m.content), images });
			} else if (m.role === "assistant") {
				// Deltas carry no messageId: they and the final message_end land on the trailing streaming item.
				const last = s.items[s.items.length - 1];
				const sid = last?.kind === "assistant" && last.streaming ? last.id : id;
				upsert(s, {
					kind: "assistant",
					id: sid,
					text: textOf(m.content),
					thinking: textOf(m.content, "thinking"),
					tools: toolIds(m.content),
					streaming: e.type === "message_update",
				});
				if (m.stopReason === "error" && m.errorMessage)
					s.items.push({ kind: "error", id: `${id}-err`, text: asStr(m.errorMessage) });
			} else if (m.role === "custom" && m.display) {
				upsert(s, { kind: "note", id, text: textOf(m.content) });
			}
			break;
		}
		case "tool_execution_start": {
			const id = asStr(e.toolCallId);
			s.tools[id] = { id, name: asStr(e.toolName), args: e.args, status: "running" };
			break;
		}
		case "tool_execution_end": {
			// A dialog is part of a tool call; once the tool ends it was answered (also on replay).
			s.ui = [];
			const id = asStr(e.toolCallId);
			const prev = s.tools[id] ?? { id, name: asStr(e.toolName), args: undefined, status: "running" };
			s.tools[id] = {
				...prev,
				status: e.isError ? "error" : "done",
				result: resultText(e.result).slice(0, 20_000),
			};
			break;
		}
		case "extension_ui_request": {
			const method = asStr(e.method);
			if (method === "setStatus") s.status = asStr(e.statusText ?? e.text);
			else if (method === "select" || method === "confirm" || method === "input" || method === "editor")
				s.ui.push({
					id: asStr(e.id),
					method,
					title: asStr(e.title),
					message: asStr(e.message) || undefined,
					options: Array.isArray(e.options) ? e.options.map(String) : undefined,
					placeholder: asStr(e.placeholder) || undefined,
				});
			break;
		}
		case "response":
			// History of a resumed session: only seeds an empty transcript.
			if (
				e.command === "get_messages" &&
				isObj(e.data) &&
				Array.isArray(e.data.messages) &&
				!s.items.length
			) {
				e.data.messages.forEach((m, i) =>
					apply(s, JSON.stringify({ type: "message_end", messageId: `h${i}`, message: m })),
				);
				for (const it of s.items) if (it.kind === "assistant") it.streaming = false;
			}
			if (e.command === "get_state" && isObj(e.data)) {
				const d = e.data;
				if (isObj(d.model)) {
					s.model = asStr(d.model.name) || asStr(d.model.id);
					const t = d.model.thinking;
					s.thinkingLevels = isObj(t) && Array.isArray(t.efforts) ? t.efforts.map(String) : [];
				}
				s.thinking = asStr(d.thinkingLevel);
				s.busy = d.isStreaming === true;
				const cu = d.contextUsage;
				s.contextPercent = isObj(cu) && typeof cu.percent === "number" ? cu.percent : null;
			}
			if (e.success === false && e.error)
				s.items.push({ kind: "error", id: `r${s.items.length}`, text: asStr(e.error) });
			break;
	}
	return s;
}

/** Splits a byte stream into complete lines, keeping the unfinished tail. */
export class LineBuffer {
	private tail = "";
	push(chunk: string): string[] {
		const parts = (this.tail + chunk).split("\n");
		this.tail = parts.pop() ?? "";
		return parts.filter((l) => l.trim());
	}
	reset() {
		this.tail = "";
	}
}
