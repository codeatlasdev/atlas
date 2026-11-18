/**
 * The words of the phone settings: what a permission is called and risks, what a phone's access adds up to, the line under
 * a phone's name and each line of its activity. Kept apart from `mobile.ts` on purpose: that module is imported at startup
 * (the crew store reads `pairUrl`), and these ~60 messages belong only to the Mobile settings screen.
 */
import { appLocale } from "#lib/i18n/locale.svelte.ts";
import { dateTime, relativeTime } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { SCOPE_IDS, type Activity, type Device, type Scope } from "#lib/settings/mobile.ts";

/** The words of one permission: its switch label and what granting it risks. Read at render time, so they follow the language. */
export function scopeText(id: Scope): { label: string; risk: string } {
	switch (id) {
		case "read":
			return { label: m.mobile_scope_read_label(), risk: m.mobile_scope_read_risk() };
		case "term_read":
			return { label: m.mobile_scope_term_read_label(), risk: m.mobile_scope_term_read_risk() };
		case "act":
			return { label: m.mobile_scope_act_label(), risk: m.mobile_scope_act_risk() };
		case "term_write":
			return { label: m.mobile_scope_term_write_label(), risk: m.mobile_scope_term_write_risk() };
		case "approve":
			return { label: m.mobile_scope_approve_label(), risk: m.mobile_scope_approve_risk() };
	}
}

/** The permissions, grouped the way they are edited, each with what granting it risks. */
export function scopeGroups(): { title: string; scopes: { id: Scope; label: string; risk: string }[] }[] {
	const of = (id: Scope) => ({ id, ...scopeText(id) });
	return [
		{ title: m.mobile_scope_group_view(), scopes: [of("read"), of("term_read")] },
		{ title: m.mobile_scope_group_control(), scopes: [of("act"), of("term_write"), of("approve")] },
	];
}

export interface Access {
	/** `Full access`, `View only`, `No access`, `Custom: 3 of 5` (in the display language). */
	label: string;
	kind: "full" | "view" | "none" | "custom";
	/** One plain sentence on what the phone can do. */
	detail: string;
}

/** What a phone's scopes add up to, in words. */
export function access(scopes: Scope[]): Access {
	const all = SCOPE_IDS.length;
	const on = SCOPE_IDS.filter((id) => scopes.includes(id));
	if (on.length === all) {
		return { label: m.mobile_access_full_label(), kind: "full", detail: m.mobile_access_full_detail() };
	}
	if (on.length === 0) {
		return { label: m.mobile_access_none_label(), kind: "none", detail: m.mobile_access_none_detail() };
	}
	if (scopes.includes("read") && on.every((id) => id === "read" || id === "term_read")) {
		return {
			label: m.mobile_access_view_label(),
			kind: "view",
			detail: scopes.includes("term_read")
				? m.mobile_access_view_terminals_detail()
				: m.mobile_access_view_detail(),
		};
	}
	const held = on.map((id) => scopeText(id).label.toLocaleLowerCase(appLocale()));
	return {
		label: m.mobile_access_custom_label({ count: on.length, total: all }),
		kind: "custom",
		detail: m.mobile_access_custom_detail({ scopes: held.join("; ") }),
	};
}

/** The line under a phone's name: connection, last seen, pairing date (both follow the display language). */
export function deviceStatus(d: Device, nowMs: number): string {
	const paired = dateTime(d.paired_at * 1000, { dateStyle: "medium" });
	if (d.connected) return m.mobile_device_status_connected({ paired });
	if (d.last_seen === undefined) return m.mobile_device_status_never({ paired });
	return m.mobile_device_status_seen({ seen: relativeTime(d.last_seen * 1000, nowMs), paired });
}

/** `"<12 chars>"` (what `audit.log` keeps of typed text) → 12. */
function sizeOf(v: unknown): number | null {
	const size = typeof v === "string" ? /^<(\d+) (?:chars|bytes base64)>$/.exec(v) : null;
	return size ? Number(size[1]) : null;
}

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

/** One line of `audit.log` as a sentence. A command the list does not know is shown by its name, never nothing. */
export function describeActivity(e: Activity): string {
	const a = e.args ?? {};
	switch (e.cmd) {
		case "pair":
			return m.mobile_activity_pair();
		case "project_service": {
			const target = str(a.service) ?? m.mobile_activity_the_stack();
			const op = str(a.op);
			if (op === "restart") return m.mobile_activity_restarted({ target });
			if (op === "start") return m.mobile_activity_started({ target });
			if (op === "stop") return m.mobile_activity_stopped({ target });
			return op ? m.mobile_activity_ran_op({ op, target }) : m.mobile_activity_ran_operation({ target });
		}
		case "project_action": {
			const key = str(a.key);
			return key ? m.mobile_activity_action_key({ key }) : m.mobile_activity_action();
		}
		case "agent_send": {
			const n = sizeOf(a.text);
			return n === null ? m.mobile_activity_prompt() : m.mobile_activity_prompt_size({ count: n });
		}
		case "agent_approve":
			return a.allow === true ? m.mobile_activity_approved() : m.mobile_activity_denied();
		case "agent_interrupt":
			return m.mobile_activity_interrupt();
		case "agent_launch": {
			const agent = str(a.agent);
			return agent ? m.mobile_activity_launch({ name: agent }) : m.mobile_activity_launch_unnamed();
		}
		case "pty_open":
			return m.mobile_activity_terminal_open();
		case "pty_write": {
			const n = sizeOf(a.data);
			return n === null
				? m.mobile_activity_terminal_write()
				: m.mobile_activity_terminal_write_size({ count: Math.floor((n * 3) / 4) });
		}
		case "pty_signal":
			return m.mobile_activity_terminal_signal();
		case "git_stage":
			return m.mobile_activity_git_stage();
		case "git_unstage":
			return m.mobile_activity_git_unstage();
		case "git_commit":
			return m.mobile_activity_git_commit();
		case "git_push":
			return m.mobile_activity_git_push();
		case "git_pr_create":
			return m.mobile_activity_pull_request();
		case "worktree_create":
			return m.mobile_activity_worktree_create();
		case "worktree_remove":
			return m.mobile_activity_worktree_remove();
		case "crewmate_task":
			return m.mobile_activity_crewmate_task();
		case "crewmate_cancel":
			return m.mobile_activity_crewmate_cancel();
		case "crewmate_answer":
			return m.mobile_activity_crewmate_answer();
		default:
			return e.cmd.trim() || m.mobile_activity_unknown();
	}
}
