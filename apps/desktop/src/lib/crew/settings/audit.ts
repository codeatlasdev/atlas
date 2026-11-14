/**
 * The Audit section's logic, kept pure: reading the `crewmate_audit` reply (newest first, as hostd sends it),
 * grouping kinds into the filter, and the one-line summary / copy text of an entry. The entry shape is hostd's
 * (`crates/atlas-host/src/crew/audit`); `audit.test.ts` reads the real reply as a fixture.
 */

export type AuditKind =
	| "tool_call"
	| "approval_asked"
	| "approval_decision"
	| "task_queued"
	| "task_started"
	| "task_finished"
	| "config_changed"
	| "secret_set";

/** One line of `audit.jsonl`: the common keys, plus whatever the kind adds (`tool_name`, `decision`, `field`, ...). */
export interface AuditEntry {
	seq: number;
	ts: number;
	kind: string;
	crewmate_id?: string;
	task_id?: string;
	tool_name?: string;
	tool_input?: unknown;
	decision?: string;
	by?: string;
	field?: string;
	from?: unknown;
	to?: unknown;
	name?: string;
	state?: string;
	prompt?: string;
	[key: string]: unknown;
}

export type AuditGroup = "tools" | "approvals" | "tasks" | "config";

/** Every kind hostd writes for a crewmate: its label and the filter group it sits in. */
export const AUDIT_KINDS: Array<{ kind: AuditKind; label: string; group: AuditGroup }> = [
	{ kind: "tool_call", label: "Tool call", group: "tools" },
	{ kind: "approval_asked", label: "Approval asked", group: "approvals" },
	{ kind: "approval_decision", label: "Approval decided", group: "approvals" },
	{ kind: "task_queued", label: "Task queued", group: "tasks" },
	{ kind: "task_started", label: "Task started", group: "tasks" },
	{ kind: "task_finished", label: "Task finished", group: "tasks" },
	{ kind: "config_changed", label: "Setting changed", group: "config" },
	{ kind: "secret_set", label: "Secret set", group: "config" },
];

export const AUDIT_GROUPS: Array<{ id: AuditGroup; label: string }> = [
	{ id: "tools", label: "Tool calls" },
	{ id: "approvals", label: "Approvals" },
	{ id: "tasks", label: "Tasks" },
	{ id: "config", label: "Settings" },
];

/** The entries of a `crewmate_audit` reply. A reply without them (an older server) or a malformed row gives none. */
export function parseAudit(reply: unknown): AuditEntry[] {
	const rows = (reply as { entries?: unknown } | undefined)?.entries;
	if (!Array.isArray(rows)) return [];
	return rows.filter(
		(r): r is AuditEntry =>
			!!r &&
			typeof r === "object" &&
			typeof r.seq === "number" &&
			typeof r.ts === "number" &&
			typeof r.kind === "string",
	);
}

export function auditKindLabel(kind: string): string {
	return AUDIT_KINDS.find((k) => k.kind === kind)?.label ?? kind;
}

/** Only the entries of one group (`null` = all). */
export function filterAudit(entries: AuditEntry[], group: AuditGroup | null): AuditEntry[] {
	if (!group) return entries;
	const kinds = new Set(AUDIT_KINDS.filter((k) => k.group === group).map((k) => k.kind as string));
	return entries.filter((e) => kinds.has(e.kind));
}

const LIST_MAX = 120;
const clip = (s: string) => (s.length > LIST_MAX ? `${s.slice(0, LIST_MAX)}…` : s);

/** A tool's input as one line: a lone string argument as is (a command, a path), else compact JSON. */
function inputText(input: unknown): string {
	if (input == null) return "";
	if (typeof input === "string") return input;
	if (typeof input === "object") {
		const vals = Object.values(input as Record<string, unknown>);
		if (vals.length === 1 && typeof vals[0] === "string") return vals[0];
	}
	return JSON.stringify(input);
}

/** Who answered an approval, readable: `desktop`, `phone:<node>`, `telegram:<chat>`, `none`. */
function answerer(by: string | undefined): string {
	if (!by || by === "none") return "nobody answered";
	if (by.startsWith("phone:")) return `answered by phone ${by.slice(6, 14)}…`;
	if (by.startsWith("telegram:")) return `answered by Telegram chat ${by.slice(9)}`;
	return `answered by ${by}`;
}

/** One line for the list; the full entry is what Copy puts on the clipboard. */
export function auditSummary(e: AuditEntry): string {
	switch (e.kind) {
		case "tool_call": {
			const input = inputText(e.tool_input);
			return clip(input ? `${e.tool_name ?? "?"}: ${input}` : `${e.tool_name ?? "?"}`);
		}
		case "approval_asked": {
			const input = inputText(e.tool_input);
			return clip(`Asked to run ${e.tool_name ?? "?"}${input ? `: ${input}` : ""}`);
		}
		case "approval_decision": {
			const verb = { allow: "Allowed", deny: "Denied", timeout: "Timed out" }[e.decision ?? ""] ?? "Decided";
			return `${verb} ${e.tool_name ?? "a tool"} (${answerer(e.by)})`;
		}
		case "task_queued":
			return clip(`Task queued (${e.source ?? "?"}): ${e.prompt ?? ""}`);
		case "task_started":
			return clip(`Task started: ${e.prompt ?? ""}`);
		case "task_finished":
			return `Task finished: ${e.state ?? "?"}${e.error ? ` (${e.error})` : ""}`;
		case "config_changed":
			return clip(`Changed ${e.field ?? "?"}: ${JSON.stringify(e.from)} → ${JSON.stringify(e.to)}`);
		case "secret_set":
			return `Set secret ${e.name ?? "?"}`;
		default:
			return e.kind;
	}
}

/** What Copy puts on the clipboard: the time, then the whole entry as JSON (nothing is shortened). */
export function auditCopyText(e: AuditEntry): string {
	return `${new Date(e.ts * 1000).toISOString()}\n${JSON.stringify(e, null, 2)}`;
}
