// managed by atlas setup — atlas __ATLAS_VERSION__ — do not edit; `atlas setup omp --apply` rewrites this file
//
// Posts omp lifecycle events to Atlas Desktop so it can show agent state
// (working / waiting / blocked / done) per terminal. Inert unless omp runs in an
// Atlas Desktop terminal (ATLAS_HOOK_ENDPOINT set). The endpoint file is re-read
// on every event, so it survives app restarts. Never throws; 1s timeout.

import { readFileSync } from "node:fs";

interface Ctx {
	sessionManager?: { getSessionId?: () => string; getSessionFile?: () => string | undefined };
	model?: { id?: string };
}
type Handler = (event: Record<string, unknown>, ctx: Ctx) => unknown;
interface Api {
	on(event: string, handler: Handler): void;
}

const ENDPOINT = process.env.ATLAS_HOOK_ENDPOINT;
const EVENTS = [
	"session_start",
	"before_agent_start",
	"agent_start",
	"tool_call",
	"tool_execution_start",
	"tool_execution_end",
	"tool_approval_requested",
	"tool_approval_resolved",
	"message_end",
	"agent_end",
	"session_shutdown",
];

function endpoint(): { port: string; token: string } | undefined {
	try {
		const env: Record<string, string> = {};
		for (const line of readFileSync(ENDPOINT!, "utf8").split("\n")) {
			const i = line.indexOf("=");
			if (i > 0) env[line.slice(0, i)] = line.slice(i + 1).trim();
		}
		return env.ATLAS_HOOK_PORT && env.ATLAS_HOOK_TOKEN
			? { port: env.ATLAS_HOOK_PORT, token: env.ATLAS_HOOK_TOKEN }
			: undefined;
	} catch {
		return undefined;
	}
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;

function text(message: unknown): string | undefined {
	if (!isObj(message) || message.role !== "assistant") return undefined;
	const content = message.content;
	if (typeof content === "string") return content;
	if (!Array.isArray(content)) return undefined;
	const parts = content
		.map((c) => (isObj(c) && c.type === "text" && typeof c.text === "string" ? c.text : ""))
		.filter(Boolean);
	return parts.length ? parts.join("\n") : undefined;
}

function payload(name: string, event: Record<string, unknown>, ctx: Ctx): Record<string, unknown> {
	const p: Record<string, unknown> = { event: name };
	try {
		p.session_id = ctx.sessionManager?.getSessionId?.();
		p.session_file = ctx.sessionManager?.getSessionFile?.();
	} catch {}
	p.model = ctx.model?.id;
	if (name === "tool_call" || name.startsWith("tool_")) p.tool_name = event.toolName ?? event.name;
	if (name === "before_agent_start" && typeof event.prompt === "string") p.prompt = event.prompt;
	if (name === "message_end") p.text = text(event.message);
	if (name === "agent_end") p.will_continue = event.willContinue === true;
	return p;
}

export default function atlasStatus(pi: Api): void {
	if (!ENDPOINT) return;
	for (const name of EVENTS) {
		pi.on(name, (event, ctx) => {
			const ep = endpoint();
			if (!ep) return undefined;
			const body = {
				pane_key: process.env.ATLAS_PANE_KEY,
				launch_token: process.env.ATLAS_LAUNCH_TOKEN,
				tab_id: process.env.ATLAS_TAB_ID,
				worktree_id: process.env.ATLAS_WORKTREE_ID,
				agent: "omp",
				payload: payload(name, event ?? {}, ctx ?? {}),
			};
			fetch(`http://127.0.0.1:${ep.port}/hook/omp`, {
				method: "POST",
				headers: { "content-type": "application/json", "x-atlas-hook-token": ep.token },
				body: JSON.stringify(body),
				signal: AbortSignal.timeout(1000),
			}).catch(() => {});
			return undefined;
		});
	}
}
