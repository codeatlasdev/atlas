// managed by atlas setup — atlas __ATLAS_VERSION__ — do not edit; `atlas setup omp --apply` rewrites this file
//
// OMP hook for Atlas projects (a directory tree containing atlas.yaml), installed by
// `atlas setup omp` as part of the ~/.atlas/omp-plugin plugin:
//   1. first agent turn of a session: inject `atlas dev --status` (once per session)
//   2. tool_call guard: block commands (bash, or hub `start`) that would start the interactive `atlas dev` TUI
// Never throws, no network, every exec has a 2s timeout.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const CONFIG_FILE = "atlas.yaml";
const EXEC_TIMEOUT_MS = 2000;
const MAX_STATUS_LINES = 25;

// Minimal structural slice of OMP's ExtensionAPI / ExtensionContext used here.
interface HookContext {
	cwd?: string;
	sessionManager?: { getSessionId?: () => string };
}
interface ExecResult {
	stdout?: string;
	code: number;
}
interface HookApi {
	exec(command: string, args: string[], options?: { cwd?: string; timeout?: number }): Promise<ExecResult>;
	on(event: "before_agent_start", handler: (event: unknown, ctx: HookContext) => Promise<unknown>): void;
	on(event: "tool_call", handler: (event: unknown, ctx: HookContext) => Promise<unknown>): void;
}

function findRoot(start: string): string | undefined {
	let dir = start;
	for (let i = 0; i < 64; i++) {
		if (existsSync(join(dir, CONFIG_FILE))) return dir;
		const parent = dirname(dir);
		if (parent === dir) return undefined;
		dir = parent;
	}
	return undefined;
}

function devScriptIsAtlasDev(root: string): boolean {
	try {
		const pkg: unknown = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
		const dev = (pkg as { scripts?: { dev?: unknown } } | null)?.scripts?.dev;
		return /^\s*atlas\s+dev(\s|$)/.test(String(dev ?? ""));
	} catch {
		return false;
	}
}

const ATLAS_DEV = /^(?:(?:\S*\/)?atlas|(?:bunx|npx)\s+atlas)\s+dev(\s[^;&|]*)?$/;
const SAFE_FLAGS = /(^|\s)(--(status|ci|headless)(\s|=|$)|-h(\s|$)|--help(\s|$))/;
const PKG_DEV = /^(?:(?:bun|npm|pnpm|yarn)\s+(?:run\s+)?dev|(?:bunx|npx)\s+atlas\s+dev)(\s|$)/;

/** Returns a block reason when `command` (run from `cwd`) starts the interactive stack. */
function blockReason(command: string, cwd: string): string | undefined {
	const root = findRoot(cwd);
	if (!root) return undefined;
	for (const raw of command.split(/&&|\|\||;|\n/)) {
		const segment = raw.trim().replace(/^(?:\w+=\S*\s+)+/, "");
		const atlasDev = segment.match(ATLAS_DEV);
		if (atlasDev && !SAFE_FLAGS.test(atlasDev[1] ?? "")) {
			return "atlas dev is an interactive TUI and must be started by the user. Use the atlas MCP tools (atlas_status, atlas_logs, atlas_restart_service, atlas_run_action, atlas_wait_ready, …) or `atlas dev --status`, or run the `/atlas:diagnose <service>` prompt. If it is not running, ask the user to start it.";
		}
		if (PKG_DEV.test(segment) && devScriptIsAtlasDev(root)) {
			return "This project's dev script runs `atlas dev` (interactive TUI) — the user starts it, not you. Use the atlas MCP tools (atlas_status, atlas_logs, atlas_restart_service, atlas_run_action, atlas_wait_ready, …) or `atlas dev --status` to inspect it, run the `/atlas:diagnose <service>` prompt, and ask the user to start it if it is down.";
		}
	}
	return undefined;
}

export default function atlasHook(pi: HookApi): void {
	const seen = new Set<string>();

	pi.on("before_agent_start", async (_event, ctx) => {
		try {
			const root = findRoot(ctx.cwd ?? process.cwd());
			if (!root) return;
			const sid = String(ctx.sessionManager?.getSessionId?.() ?? "default");
			if (seen.has(sid)) return;
			seen.add(sid);

			let body: string;
			try {
				const r = await pi.exec("atlas", ["dev", "--status", "--dir", root], { cwd: root, timeout: EXEC_TIMEOUT_MS });
				const out = (r.stdout ?? "").trim();
				if (r.code === 0 && out) {
					const lines = out.split("\n");
					const shown =
						lines.length > MAX_STATUS_LINES
							? [...lines.slice(0, MAX_STATUS_LINES), `… (${lines.length - MAX_STATUS_LINES} more lines)`]
							: lines;
					body = `atlas dev is RUNNING for ${root}:\n${shown.join("\n")}\nUse the atlas MCP tools (atlas_status, atlas_logs, atlas_restart_service, atlas_run_action, atlas_wait_ready, …) to inspect or control it. Never run \`atlas dev\` yourself.`;
				} else {
					body = `atlas dev is NOT running for ${root}. If you need the stack, ask the user to start it (\`atlas dev\`). Never run \`atlas dev\` yourself; the atlas MCP tools and \`atlas dev --status\` are safe.`;
				}
			} catch {
				body = `Atlas project at ${root} (atlas.yaml found). Could not read \`atlas dev --status\`. Never run \`atlas dev\` yourself; ask the user to start it and use the atlas MCP tools.`;
			}
			return { message: { customType: "atlas-context", content: body, display: true, attribution: "agent" } };
		} catch {
			return;
		}
	});

	pi.on("tool_call", async (event, ctx) => {
		try {
			const e = event as { toolName?: string; input?: { command?: unknown; cwd?: unknown; application?: unknown; args?: unknown; op?: unknown } };
			let command = "";
			if (e.toolName === "bash") {
				command = String(e.input?.command ?? "");
			} else if (e.toolName === "hub" && e.input?.op === "start") {
				// hub starts a named process as `application` + `args[]`, not one command string.
				const args = Array.isArray(e.input.args) ? e.input.args.map(String) : [];
				command = [String(e.input.application ?? ""), ...args].join(" ").trim();
			}
			if (!command) return;
			const base = String(ctx.cwd ?? process.cwd());
			const reason = blockReason(command, e.input?.cwd ? resolve(base, String(e.input.cwd)) : base);
			if (reason) return { block: true, reason };
		} catch {
			return;
		}
	});
}
