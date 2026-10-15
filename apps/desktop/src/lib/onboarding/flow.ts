/**
 * The first-run flow as a state machine, and the two summaries it shows. Pure: the Svelte layer feeds it events.
 *
 *   welcome ──scan──▶ scanning ──scanned(n>0)──▶ results ──added──▶ starting ──stack-up──▶ up ──continue──▶ done
 *      ▲                 │  └──scanned(0)──▶ empty ──scan──▶ scanning        └──stack-failed──▶ results
 *      └──cancel/failed──┘
 *   `skip` reaches done from anywhere; a project added by any path ends the welcome at once (`added`).
 */
import type { Found } from "#lib/components/sidebar/discovery.ts";
import { SIGNATURE_PREROLL } from "./intro/player.ts";
import { BEATS, DURATION } from "./intro/timeline.ts";

export type Phase = "welcome" | "scanning" | "empty" | "results" | "starting" | "up" | "done";

export type Event =
	| { t: "scan" }
	| { t: "scanned"; projects: number }
	| { t: "scan-failed" }
	| { t: "cancel" }
	| { t: "added" }
	| { t: "stack-up" }
	| { t: "stack-failed" }
	| { t: "continue" }
	| { t: "skip" };

export const INITIAL: Phase = "welcome";

/** Next phase; an event that does not apply to `phase` leaves it unchanged. */
export function next(phase: Phase, ev: Event): Phase {
	if (phase === "done") return "done";
	if (ev.t === "skip") return "done";
	switch (phase) {
		case "welcome":
			if (ev.t === "scan") return "scanning";
			if (ev.t === "added") return "starting";
			return phase;
		case "scanning":
			if (ev.t === "scanned") return ev.projects > 0 ? "results" : "empty";
			if (ev.t === "cancel" || ev.t === "scan-failed") return "welcome";
			if (ev.t === "added") return "starting";
			return phase;
		case "empty":
			if (ev.t === "scan") return "scanning";
			if (ev.t === "added") return "starting";
			return phase;
		case "results":
			if (ev.t === "added") return "starting";
			if (ev.t === "scan") return "scanning";
			return phase;
		case "starting":
			if (ev.t === "stack-up") return "up";
			if (ev.t === "stack-failed") return "results";
			return phase;
		case "up":
			return ev.t === "continue" ? "done" : phase;
	}
}

export interface ScanSummary {
	/** Folders with an atlas.yaml that are not packages inside another project. */
	projects: number;
	/** Services across those projects (a broken config adds none). */
	services: number;
	/** Folders without an atlas.yaml that could use Atlas. */
	candidates: number;
	/** Projects whose atlas.yaml did not parse. */
	broken: number;
	nested: number;
	/** The newest projects, at most `limit`, for the live list. */
	top: Found[];
	/** Projects beyond `limit`, for "and 7 more". */
	hidden: number;
}

/** What the scan found so far, shaped for the hero: counts plus the few newest projects worth naming. */
export function summarize(found: Found[], limit = 6): ScanSummary {
	const configured = found.filter((f) => f.has_config);
	const top = configured.filter((f) => !f.nested);
	const sorted = [...top].sort(
		(a, b) => (b.modified ?? 0) - (a.modified ?? 0) || a.name.localeCompare(b.name),
	);
	return {
		projects: top.length,
		services: top.reduce((n, f) => n + (f.error ? 0 : f.services), 0),
		candidates: found.length - configured.length,
		broken: top.filter((f) => f.error).length,
		nested: configured.length - top.length,
		top: sorted.slice(0, limit),
		hidden: Math.max(0, sorted.length - limit),
	};
}

interface Row {
	state: string;
	completed_ok?: boolean;
	deselected?: boolean;
}

export interface StartProgress {
	up: number;
	total: number;
	failed: number;
	done: boolean;
}

/**
 * How far a stack start is: running over wanted services (deselected and finished one-shots do not count).
 * `reported`: the runner has sent its state; with nothing wanted the stack is then up (an atlas.yaml whose
 * services are all commented out), while before that report an empty list only means "not heard from yet".
 */
export function progress(rows: Row[], reported = false): StartProgress {
	const want = rows.filter((r) => !r.deselected && !r.completed_ok);
	const up = want.filter((r) => r.state === "running").length;
	const failed = want.filter((r) => r.state === "failed").length;
	return { up, total: want.length, failed, done: want.length > 0 ? up === want.length : reported };
}

/**
 * Whether the welcome overlay is up. A first run is "welcome never finished and nothing added yet"; but once the hero
 * itself adds the first project the count is no longer zero, so the overlay must stay for the rest of the guided flow
 * (`starting`, `up`) or the user would never see their stack come up.
 */
export function showWelcome(
	state: { done: boolean },
	loaded: boolean,
	projectCount: number,
	phase: Phase,
): boolean {
	if (!loaded || phase === "done") return false;
	if (phase === "starting" || phase === "up") return !state.done;
	return projectCount === 0 && !state.done;
}

/** The event Esc raises at `phase`, or null. Esc stops a scan before it dismisses anything, and never abandons a starting stack. */
export function escapeEvent(phase: Phase): Event | null {
	if (phase === "done" || phase === "starting") return null;
	return phase === "scanning" ? { t: "cancel" } : { t: "skip" };
}

/** What Enter does at `phase`: the step's one primary action, named for the view to run. */
export function primaryEvent(phase: Phase): "scan" | "init" | "add" | "continue" | null {
	switch (phase) {
		case "welcome":
			return "scan";
		case "empty":
			return "init";
		case "results":
			return "add";
		case "up":
			return "continue";
		default:
			return null;
	}
}

/** Whether the main pane shows the standing empty state: no project, lists loaded, and the first-run overlay is over. */
export function showStanding(state: { done: boolean }, loaded: boolean, projectCount: number): boolean {
	return loaded && projectCount === 0 && state.done;
}

/** `data-testid` of a phase's stage: `welcome-<phase>`. Scripted recordings wait on these instead of timing guesses. */
export const testId = (phase: string) => `welcome-${phase}`;

/** `data-testid` of each control a recording clicks. */
export const ACTIONS = {
	scan: "welcome-action-scan",
	addFolder: "welcome-action-add-folder",
	init: "welcome-action-init",
	add: "welcome-action-add",
	skip: "welcome-action-skip",
	finish: "welcome-action-finish",
} as const;

/** The title sequence's real timing in ms, for `atlas desktop debug eval` scripts (import, do not copy the numbers). */
export const INTRO_TIMING = {
	durationMs: Math.round(DURATION * 1000),
	hitMs: Math.round(BEATS.hit * 1000),
	/** When the signature is started (its strike is pre-rolled, so it lands on `hitMs`). */
	soundMs: Math.round((BEATS.hit - SIGNATURE_PREROLL) * 1000),
} as const;
