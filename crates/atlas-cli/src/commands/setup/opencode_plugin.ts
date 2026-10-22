// managed by atlas setup — atlas __ATLAS_VERSION__ — do not edit; `atlas setup opencode --apply` rewrites this file
//
// Reports OpenCode lifecycle events to Atlas so it can show agent state
// (working / blocked / done) per terminal pane.  Also proxies permission
// requests to Atlas for remote approval (Atlas Desktop / Mobile / Telegram).
// Inert unless OpenCode runs in an Atlas terminal (ATLAS_HOOK_ENDPOINT set).
// Never throws; 1 s timeout on non-blocking posts.
//
// Verified against OpenCode v2 SDK types (permission.updated, postSessionIdPermissionsPermissionId).
// Also handles the plugin-docs name "permission.asked" for forward-compat.

import { readFileSync } from "node:fs";

// Endpoint file path — frozen at process start; the file itself is re-read on
// every event so Atlas restarts do not require restarting OpenCode.
const ENDPOINT_FILE = process.env.ATLAS_HOOK_ENDPOINT;
const PANE_KEY = process.env.ATLAS_PANE_KEY;
const LAUNCH_TOKEN = process.env.ATLAS_LAUNCH_TOKEN;
const TAB_ID = process.env.ATLAS_TAB_ID;
const WORKTREE_ID = process.env.ATLAS_WORKTREE_ID;
// Bot runner sets ATLAS_HOOK_DECISION_SECS=1800; interactive sessions default to 55 s.
const DECISION_SECS = Number(process.env.ATLAS_HOOK_DECISION_SECS ?? 55);

interface Endpoint {
	port: string;
	token: string;
}

function endpoint(): Endpoint | undefined {
	try {
		const env: Record<string, string> = {};
		for (const line of readFileSync(ENDPOINT_FILE!, "utf8").split("\n")) {
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

async function post(
	port: string,
	token: string,
	path: string,
	body: unknown,
	timeoutMs = 1000,
): Promise<Response | null> {
	try {
		return await fetch(`http://127.0.0.1:${port}${path}`, {
			method: "POST",
			headers: {
				"content-type": "application/json",
				"x-atlas-hook-token": token,
			},
			body: JSON.stringify(body),
			signal: AbortSignal.timeout(timeoutMs),
		});
	} catch {
		return null;
	}
}

// Minimal slice of the OpenCode SDK client used for permission replies.
interface OpencodeClient {
	postSessionIdPermissionsPermissionId(opts: {
		path: { id: string; permissionID: string };
		body: { response: "once" | "always" | "reject" };
	}): Promise<unknown>;
}

// Shape of a permission object from EventPermissionUpdated.properties (OpenCode SDK v2).
interface PermissionProps {
	id?: string;
	sessionID?: string;
	type?: string;   // tool/action type, used as tool_name
	title?: string;
	metadata?: Record<string, unknown>;
	// "permission.asked" variant fields (plugin-docs name, older/future versions)
	action?: string;
	resources?: unknown;
}

type EventProps = Record<string, unknown>;

export const AtlasStatus = async ({ client }: { client: OpencodeClient }) => {
	if (!ENDPOINT_FILE) return {};

	return {
		event: async ({ event }: { event: { type: string; properties?: EventProps } }) => {
			const ep = endpoint();
			if (!ep) return;

			const { port, token } = ep;
			const base = {
				pane_key: PANE_KEY,
				launch_token: LAUNCH_TOKEN,
				tab_id: TAB_ID,
				worktree_id: WORKTREE_ID,
				agent: "opencode",
			};

			const { type, properties: props = {} } = event;

			// ── session idle ─────────────────────────────────────────────────────────
			if (type === "session.idle") {
				await post(port, token, "/hook/opencode", {
					...base,
					payload: { event: "stop" },
				});

			// ── session working (busy status) ────────────────────────────────────────
			} else if (type === "session.status") {
				const statusType = (props.status as { type?: string } | undefined)?.type;
				if (statusType === "busy") {
					await post(port, token, "/hook/opencode", {
						...base,
						payload: { event: "user_prompt_submit" },
					});
				}

			// ── tool use ─────────────────────────────────────────────────────────────
			} else if (type === "tool.execute.after") {
				await post(port, token, "/hook/opencode", {
					...base,
					payload: {
						event: "post_tool_use",
						tool_name: props.tool ?? props.name ?? "",
					},
				});

			// ── message updates ───────────────────────────────────────────────────────
			} else if (type === "message.updated") {
				const info = (props.info ?? {}) as { role?: string; text?: string };
				if (info.role === "user") {
					await post(port, token, "/hook/opencode", {
						...base,
						payload: { event: "user_prompt_submit" },
					});
				} else if (info.role === "assistant") {
					await post(port, token, "/hook/opencode", {
						...base,
						payload: { event: "post_tool_use" },
					});
				}

			// ── permission request ────────────────────────────────────────────────────
			// Handles both SDK name ("permission.updated", EventPermissionUpdated) and
			// plugin-docs name ("permission.asked") for compatibility across versions.
			} else if (type === "permission.updated" || type === "permission.asked") {
				const perm = props as PermissionProps;
				const permId = String(perm.id ?? "");
				const sessionId = String(perm.sessionID ?? "");
				if (!permId || !sessionId) return;

				// tool_name: .type for permission.updated, .action for permission.asked
				const toolName = String(perm.type ?? perm.action ?? perm.title ?? "");
				const toolInput = (perm.metadata ?? {}) as Record<string, unknown>;

				// Notify Atlas that the agent is waiting for approval (sets state → Blocked).
				await post(port, token, "/hook/opencode", {
					...base,
					payload: {
						event: "permission_request",
						tool_name: toolName,
						tool_input: toolInput,
					},
				});

				// Block until Atlas delivers a remote decision (phone / Desktop / Telegram).
				// Server blocks for DECISION_SECS; add 5 s of slack for network round-trip.
				const resp = await post(
					port,
					token,
					"/hook/opencode/decision",
					{
						...base,
						payload: {
							event: "permission_request",
							tool_name: toolName,
							tool_input: toolInput,
						},
					},
					(DECISION_SECS + 5) * 1000,
				);

				if (!resp?.ok) return; // no approver connected → let OpenCode handle natively

				const json = (await resp.json()) as { decision?: string | null };
				const decision = json.decision;

				// null → no approver answered; skip: OpenCode shows its own TUI dialog (or
				// auto-approves when started with --auto).
				if (decision !== "allow" && decision !== "deny") return;

				await client
					.postSessionIdPermissionsPermissionId({
						path: { id: sessionId, permissionID: permId },
						body: { response: decision === "allow" ? "once" : "reject" },
					})
					.catch(() => {});
			}
		},
	};
};
