/**
 * The crew server's capture browsers (Playwright chromium + webkit behind the crewmates' `capture`, `ui_gate` and
 * `locate` tools): what `crew_browsers_status` answers and how the screen words it. Pure, no Svelte/Tauri, so the
 * rules are bun-testable. The problem lines are the CLI's own (`atlas crew browsers status`): the Rust side strips
 * the `✗` marker, this reads what is left.
 */

import type { AdminAddress } from "#lib/crew.ts";
import type { ParsedTarget } from "#lib/crew/sshTarget.ts";

/** What `crew_browsers_status` answers: ready, or one line per missing piece. */
export interface BrowsersStatus {
	ready: boolean;
	problems: string[];
}

/** The oldest Node the pinned Playwright runs on; the CLI refuses anything older. */
export const MIN_NODE = 20;

/** Why the user is offered the browsers, and what they cost (shown next to the opt-in and the settings row). */
export const BROWSERS_WHY =
	"Crewmates look at the UI they build: the capture, UI gate and locate tools need them.";
export const BROWSERS_COST = "Needs Node.js 20+ on the server and about 500 MB of downloads.";

/** The kind of a missing piece, from the prefix the CLI's status script gives each line. */
export type ProblemKind = "node" | "npm" | "runner" | "playwright" | "browsers" | "deps" | "other";

export interface Problem {
	kind: ProblemKind;
	/** The CLI's line, without the "run atlas crew browsers install" tail the button replaces. */
	text: string;
}

const KINDS: ProblemKind[] = ["node", "npm", "runner", "playwright", "browsers", "deps"];

/** `: run atlas crew browsers install` (or `, run …`) ends most lines; the Install button is that instruction. */
const RUN_TAIL = /[:,]\s*run atlas crew browsers install\s*$/;

export function parseProblem(line: string): Problem {
	const kind = KINDS.find((k) => line.startsWith(`${k}:`)) ?? "other";
	return { kind, text: line.replace(RUN_TAIL, "").trim() };
}

/** Node (or npm) is what the install cannot fix: the user installs Node first, then the button works. */
export function needsNode(problems: Problem[]): boolean {
	return problems.some((p) => p.kind === "node" || p.kind === "npm");
}

/** The first node problem, worded as the requirement ("Node 18.19.1 found, need >= 20 …"), or null. */
export function nodeProblem(problems: Problem[]): string | null {
	const p = problems.find((p) => p.kind === "node" || p.kind === "npm");
	return p ? p.text.replace(/^(node|npm):\s*/, "") : null;
}

export type BrowsersView =
	| { phase: "checking" }
	| { phase: "ready" }
	| { phase: "missing"; problems: Problem[]; needsNode: boolean }
	| { phase: "error"; error: string };

export const checkingView: BrowsersView = { phase: "checking" };

export function viewOf(status: BrowsersStatus): BrowsersView {
	if (status.ready) return { phase: "ready" };
	const problems = status.problems.map(parseProblem);
	return { phase: "missing", problems, needsNode: needsNode(problems) };
}

export const errorView = (error: string): BrowsersView => ({ phase: "error", error });

/** The Install button is only useful when the install can succeed: not while Node is missing or too old. */
export function canInstall(v: BrowsersView): boolean {
	return v.phase === "missing" && !v.needsNode;
}

/** The text field of an admin address: `user@host`, or `user@host:2222` for another port (what `parseSSHTarget` reads back). */
export function addressText(a: AdminAddress | undefined): string {
	return a ? (a.port && a.port !== 22 ? `${a.target}:${a.port}` : a.target) : "";
}

/** The registry's form of what the user typed: the port is left out when it is the default 22. */
export function adminOf(p: ParsedTarget): AdminAddress {
	return p.port !== 22 ? { target: p.target, port: p.port } : { target: p.target };
}

/** Whether two addresses are the same one (an absent port is 22), so a save is only made for a real change. */
export function sameAdmin(a: AdminAddress | undefined, b: AdminAddress | undefined): boolean {
	return !!a && !!b && a.target === b.target && (a.port ?? 22) === (b.port ?? 22);
}
