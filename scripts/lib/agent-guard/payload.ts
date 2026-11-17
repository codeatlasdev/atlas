// Each harness describes a tool call in its own JSON; these turn them into the guard's `Call`, or `undefined` for a tool
// the guard has no opinion on (reads, searches, MCP…).
import type { Call } from "./decide";

const str = (v: unknown): string | undefined => (typeof v === "string" && v !== "" ? v : undefined);
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === "object" ? (v as Record<string, unknown>) : {});

/** `*** Add|Update|Delete File: p` and `*** Move to: p` lines of a Codex `apply_patch` body. */
function patchPaths(patch: string): string[] {
	return [...patch.matchAll(/^\*\*\* (?:Add File|Update File|Delete File|Move to): (.+)$/gm)].map((m) => m[1].trim());
}

/**
 * PreToolUse payload of Claude Code (`Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `Bash`) and Codex (`Bash`, `apply_patch`):
 * `{ tool_name, tool_input, cwd }` on stdin (https://code.claude.com/docs/en/hooks, https://developers.openai.com/codex/hooks).
 */
export function fromHook(payload: unknown): Call | undefined {
	const p = obj(payload);
	const input = obj(p.tool_input);
	switch (p.tool_name) {
		case "Write":
		case "Edit":
		case "MultiEdit":
		case "NotebookEdit": {
			const path = str(input.file_path) ?? str(input.notebook_path);
			return path ? { kind: "write", paths: [path] } : undefined;
		}
		case "apply_patch": {
			const patch = str(input.command);
			return patch ? { kind: "write", paths: patchPaths(patch) } : undefined;
		}
		case "Bash": {
			const command = str(input.command);
			return command ? { kind: "bash", command } : undefined;
		}
		default:
			return undefined;
	}
}

/** omp `tool_call` event (`toolName`, `input`): https://github.com/can1357/oh-my-pi docs/hooks.md. */
export function fromOmp(event: unknown): Call | undefined {
	const e = obj(event);
	const input = obj(e.input);
	switch (e.toolName) {
		case "bash": {
			const command = str(input.command);
			return command ? { kind: "bash", command, cwd: str(input.cwd) } : undefined;
		}
		case "write":
		case "edit":
		case "ast_edit":
		case "notebook": {
			const paths = [input.path, input.file_path, ...(Array.isArray(input.paths) ? input.paths : [])].map(str).filter((p): p is string => !!p);
			// The hashline `edit` carries its files as `[path#TAG]` headers.
			const text = str(input.input);
			if (text) for (const m of text.matchAll(/^\[(.+?)#[0-9A-Fa-f]{4}\]/gm)) paths.push(m[1]);
			return paths.length ? { kind: "write", paths } : undefined;
		}
		default:
			return undefined;
	}
}
