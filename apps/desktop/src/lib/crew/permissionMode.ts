/**
 * Permission modes as the desktop shows them: what a crewmate's agent may do without asking (Ask, Accept edits,
 * Bypass), who may switch it, and how an approval is answered (Allow once / Allow for this task / Deny, with a note).
 * Pure (no Svelte): the tests feed it the fixtures hostd's own serializer wrote (`crew_permission_modes.json`).
 *
 * Bypass is the only coloured mode (`modeIsAmber`); an engine that cannot ask (`approval_supported: false`) shows its
 * mode fixed. The answers' limits (`scope`, the 2000-character note) are hostd's contract for `agent_approve`.
 */
import type { ApprovalRail, ApprovalScope, Crewmate, PendingApproval, PermissionMode } from "#lib/crew.ts";
import { m } from "#lib/i18n/m.ts";

export const PERMISSION_MODES: readonly PermissionMode[] = ["ask", "edits", "bypass"];

/** The longest note hostd takes with an answer. */
export const COMMENT_MAX = 2000;

/** The crew-level policy of one crew server (`crew_settings_get`): the mode new crewmates start in, and whether Bypass exists. */
export interface CrewPolicy {
	default_approval: PermissionMode;
	allow_bypass: boolean;
}

/** What a crew server that predates permission modes implies. */
export const DEFAULT_POLICY: CrewPolicy = { default_approval: "ask", allow_bypass: true };

const isMode = (v: unknown): v is PermissionMode => v === "ask" || v === "edits" || v === "bypass";

/** The policy in a `crew_settings_get` / `crew_settings_set` reply; a field an older hostd does not send reads as its default. */
export function parsePolicy(r: { default_approval?: unknown; allow_bypass?: unknown }): CrewPolicy {
	return {
		default_approval: isMode(r.default_approval) ? r.default_approval : DEFAULT_POLICY.default_approval,
		allow_bypass: typeof r.allow_bypass === "boolean" ? r.allow_bypass : DEFAULT_POLICY.allow_bypass,
	};
}

/** The crewmate's mode: hostd's `mode`, or for a crew server that only sends the old `approval` (`auto` is Bypass). */
export function modeOf(c: Pick<Crewmate, "mode" | "approval">): PermissionMode {
	return isMode(c.mode) ? c.mode : c.approval === "auto" ? "bypass" : "ask";
}

/** Whether the person can change this crewmate's mode: an engine without an approval channel is fixed. */
export function modeLocked(c: Pick<Crewmate, "approval_supported">): boolean {
	return c.approval_supported === false;
}

/** Only Bypass is coloured; Ask and Accept edits are neutral. */
export const modeIsAmber = (mode: PermissionMode): boolean => mode === "bypass";

/** The modes the person can pick: Bypass is hidden while the crew does not allow it. */
export function selectableModes(policy: Pick<CrewPolicy, "allow_bypass">): PermissionMode[] {
	return PERMISSION_MODES.filter((mode) => mode !== "bypass" || policy.allow_bypass);
}

export function modeLabel(mode: PermissionMode): string {
	switch (mode) {
		case "ask":
			return m.crew_permission_mode_ask();
		case "edits":
			return m.crew_permission_mode_edits();
		case "bypass":
			return m.crew_permission_mode_bypass();
	}
}

export function modeHint(mode: PermissionMode): string {
	switch (mode) {
		case "ask":
			return m.crew_permission_hint_ask();
		case "edits":
			return m.crew_permission_hint_edits();
		case "bypass":
			return m.crew_permission_hint_bypass();
	}
}

/** Why a rail held a call that no mode would have allowed. */
export function railText(rail: ApprovalRail): string {
	return rail === "protected_path"
		? m.crew_permission_rail_protected_path()
		: m.crew_permission_rail_no_auto();
}

/** Whether "Allow for this task" exists for this prompt: hostd never lets a grant cover a call a rail held. */
export const canGrantForTask = (pa: Pick<PendingApproval, "rail">): boolean => !pa.rail;

/** How a prompt was answered. */
export interface ApprovalAnswer {
	allow: boolean;
	scope: ApprovalScope;
	/** Free text for the agent; trimmed, and absent when empty. */
	comment?: string;
}

/** An answer with its note trimmed and cut to what hostd takes; a deny never carries a scope other than `once`. */
export function makeAnswer(allow: boolean, scope: ApprovalScope, note: string): ApprovalAnswer {
	const comment = note.trim().slice(0, COMMENT_MAX);
	return { allow, scope: allow ? scope : "once", ...(comment ? { comment } : {}) };
}

/** The `agent_approve` arguments of an answer (the routing `host` is added by the store). */
export function approveArgs(pane: string, req: string, a: ApprovalAnswer) {
	return { pane, req, allow: a.allow, scope: a.scope, ...(a.comment ? { comment: a.comment } : {}) };
}
