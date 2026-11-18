/**
 * The Audit section's logic, kept pure: reading the `crewmate_audit` reply (newest first, as hostd sends it),
 * grouping kinds into the filter, and the one-line summary / copy text of an entry. The entry shape is hostd's
 * (`crates/atlas-host/src/crew/audit`); `audit.test.ts` reads the real reply as a fixture.
 */
import { m } from "#lib/i18n/m.ts";

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

/** Every kind hostd writes for a crewmate and the filter group it sits in; its label is `auditKindLabel`. */
export const AUDIT_KINDS: Array<{ kind: AuditKind; group: AuditGroup }> = [
	{ kind: "tool_call", group: "tools" },
	{ kind: "approval_asked", group: "approvals" },
	{ kind: "approval_decision", group: "approvals" },
	{ kind: "task_queued", group: "tasks" },
	{ kind: "task_started", group: "tasks" },
	{ kind: "task_finished", group: "tasks" },
	{ kind: "config_changed", group: "config" },
	{ kind: "secret_set", group: "config" },
];

export const AUDIT_GROUPS: AuditGroup[] = ["tools", "approvals", "tasks", "config"];

export function auditGroupLabel(group: AuditGroup): string {
	switch (group) {
		case "tools":
			return m.crew_settings_audit_group_tools();
		case "approvals":
			return m.crew_settings_audit_group_approvals();
		case "tasks":
			return m.crew_settings_audit_group_tasks();
		case "config":
			return m.crew_settings_audit_group_config();
	}
}

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
	switch (kind) {
		case "tool_call":
			return m.crew_settings_audit_kind_tool_call();
		case "approval_asked":
			return m.crew_settings_audit_kind_approval_asked();
		case "approval_decision":
			return m.crew_settings_audit_kind_approval_decision();
		case "task_queued":
			return m.crew_settings_audit_kind_task_queued();
		case "task_started":
			return m.crew_settings_audit_kind_task_started();
		case "task_finished":
			return m.crew_settings_audit_kind_task_finished();
		case "config_changed":
			return m.crew_settings_audit_kind_config_changed();
		case "secret_set":
			return m.crew_settings_audit_kind_secret_set();
		default:
			return kind;
	}
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
	if (!by || by === "none") return m.crew_settings_audit_answer_nobody();
	if (by.startsWith("phone:")) return m.crew_settings_audit_answer_phone({ node: by.slice(6, 14) });
	if (by.startsWith("telegram:")) return m.crew_settings_audit_answer_telegram({ chat: by.slice(9) });
	return m.crew_settings_audit_answer_by({ by });
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
			const tool = e.tool_name ?? "?";
			return clip(
				input
					? m.crew_settings_audit_asked_to_run_input({ tool, input })
					: m.crew_settings_audit_asked_to_run({ tool }),
			);
		}
		case "approval_decision": {
			const tool = e.tool_name ?? m.crew_settings_audit_a_tool();
			const answer = answerer(e.by);
			switch (e.decision) {
				case "allow":
					return m.crew_settings_audit_decision_allowed({ tool, answer });
				case "deny":
					return m.crew_settings_audit_decision_denied({ tool, answer });
				case "timeout":
					return m.crew_settings_audit_decision_timed_out({ tool, answer });
				default:
					return m.crew_settings_audit_decision_decided({ tool, answer });
			}
		}
		case "task_queued":
			return clip(
				m.crew_settings_audit_task_queued({ source: String(e.source ?? "?"), prompt: e.prompt ?? "" }),
			);
		case "task_started":
			return clip(m.crew_settings_audit_task_started({ prompt: e.prompt ?? "" }));
		case "task_finished": {
			const state = e.state ?? "?";
			return e.error
				? m.crew_settings_audit_task_finished_error({ state, error: String(e.error) })
				: m.crew_settings_audit_task_finished({ state });
		}
		case "config_changed":
			return clip(
				m.crew_settings_audit_changed({
					field: e.field ?? "?",
					from: JSON.stringify(e.from),
					to: JSON.stringify(e.to),
				}),
			);
		case "secret_set":
			return m.crew_settings_audit_secret_set({ name: e.name ?? "?" });
		default:
			return e.kind;
	}
}

/** What Copy puts on the clipboard: the time, then the whole entry as JSON (nothing is shortened). */
export function auditCopyText(e: AuditEntry): string {
	return `${new Date(e.ts * 1000).toISOString()}\n${JSON.stringify(e, null, 2)}`;
}
