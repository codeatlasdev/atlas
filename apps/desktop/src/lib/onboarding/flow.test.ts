// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	INITIAL,
	escapeEvent,
	next,
	primaryEvent,
	progress,
	showStanding,
	showWelcome,
	summarize,
	type Event,
	type Phase,
	ACTIONS,
	INTRO_TIMING,
	testId,
} from "./flow.ts";
import { BEATS, DURATION } from "./intro/timeline.ts";
import { SIGNATURE_PREROLL } from "./intro/player.ts";

const found = (name, o = {}) => ({
	path: `/Users/me/Projects/${name}`,
	root: "/Users/me/Projects",
	name,
	services: 3,
	error: null,
	branch: "main",
	remote: null,
	modified: 1_700_000_000,
	has_config: true,
	nested: false,
	stack: null,
	...o,
});
const run = (events: Event[], from: Phase = INITIAL) => events.reduce((p, e) => next(p, e), from);

describe("first-run flow", () => {
	test("starts at welcome and the one primary action leads to scanning", () => {
		expect(INITIAL).toBe("welcome");
		expect(next("welcome", { t: "scan" })).toBe("scanning");
	});

	test("a scan with projects goes to results; a scan with none goes to empty", () => {
		expect(next("scanning", { t: "scanned", projects: 3 })).toBe("results");
		expect(next("scanning", { t: "scanned", projects: 0 })).toBe("empty");
	});

	test("cancelling or failing a scan returns to welcome, never a dead end", () => {
		expect(next("scanning", { t: "cancel" })).toBe("welcome");
		expect(next("scanning", { t: "scan-failed" })).toBe("welcome");
	});

	test("adding projects from results starts the stack, then reaches 'up', then hands off", () => {
		expect(run([{ t: "scan" }, { t: "scanned", projects: 2 }, { t: "added" }])).toBe("starting");
		expect(run([{ t: "scan" }, { t: "scanned", projects: 2 }, { t: "added" }, { t: "stack-up" }])).toBe("up");
		expect(
			run([
				{ t: "scan" },
				{ t: "scanned", projects: 2 },
				{ t: "added" },
				{ t: "stack-up" },
				{ t: "continue" },
			]),
		).toBe("done");
	});

	test("a stack that fails to come up does not strand the user in 'starting'", () => {
		expect(next("starting", { t: "stack-failed" })).toBe("results");
		expect(next("starting", { t: "skip" })).toBe("done");
	});

	test("a project added by any other path (picker, CLI) ends the welcome at once", () => {
		for (const p of ["welcome", "scanning", "empty", "results"] as Phase[])
			expect(next(p, { t: "added" }), p).toBe("starting");
	});

	test("skip works from every phase and lands on done", () => {
		for (const p of ["welcome", "scanning", "empty", "results", "starting", "up"] as Phase[])
			expect(next(p, { t: "skip" }), p).toBe("done");
	});

	test("'done' is terminal: nothing moves it", () => {
		for (const e of [{ t: "scan" }, { t: "added" }, { t: "stack-up" }, { t: "cancel" }] as Event[])
			expect(next("done", e)).toBe("done");
	});

	test("events that do not apply to a phase are ignored, not errors", () => {
		expect(next("welcome", { t: "stack-up" })).toBe("welcome");
		expect(next("results", { t: "scanned", projects: 5 })).toBe("results");
		expect(next("up", { t: "scan" })).toBe("up");
	});

	test("empty can retry a scan", () => {
		expect(next("empty", { t: "scan" })).toBe("scanning");
	});
});

describe("summarize: the live result of a scan", () => {
	test("counts projects with atlas.yaml and their services, ignores folders without one", () => {
		const s = summarize([
			found("api", { services: 4 }),
			found("web", { services: 2 }),
			found("notes", { has_config: false }),
		]);
		expect(s).toMatchObject({ projects: 2, services: 6, candidates: 1 });
	});

	test("projects with a broken config count as found but add no services", () => {
		const s = summarize([found("ok", { services: 3 }), found("bad", { services: 0, error: "invalid yaml" })]);
		expect(s).toMatchObject({ projects: 2, services: 3, broken: 1 });
	});

	test("nested monorepo packages are not double counted as top-level projects", () => {
		const s = summarize([found("mono"), found("mono-pkg", { nested: true })]);
		expect(s.projects).toBe(1);
		expect(s.nested).toBe(1);
	});

	test("the headline picks the newest projects first and caps the visible list", () => {
		const many = Array.from({ length: 12 }, (_, i) => found(`p${i}`, { modified: 1_000 + i }));
		const s = summarize(many, 5);
		expect(s.top.map((f) => f.name)).toEqual(["p11", "p10", "p9", "p8", "p7"]);
		expect(s.hidden).toBe(7);
	});

	test("an empty scan has zero of everything and no NaN", () => {
		expect(summarize([])).toEqual({
			projects: 0,
			services: 0,
			candidates: 0,
			broken: 0,
			nested: 0,
			top: [],
			hidden: 0,
		});
	});
});

describe("progress: how far a stack start is", () => {
	const s = (state, o = {}) => ({ name: state, state, ...o });
	test("counts running over wanted services, ignoring deselected and finished one-shots", () => {
		const p = progress([
			s("running"),
			s("starting"),
			s("stopped", { deselected: true }),
			s("stopped", { completed_ok: true }),
		]);
		expect(p).toEqual({ up: 1, total: 2, failed: 0, done: false });
	});
	test("done when every wanted service runs", () => {
		expect(progress([s("running"), s("running")])).toMatchObject({ up: 2, total: 2, done: true });
	});
	test("failures are counted and block done", () => {
		expect(progress([s("running"), s("failed")])).toMatchObject({ up: 1, failed: 1, done: false });
	});
	test("no services yet is not done (the stack has not reported)", () => {
		expect(progress([])).toEqual({ up: 0, total: 0, failed: 0, done: false });
	});
	// Incident (first run on Windows, 2026-10-14): a project whose atlas.yaml has every service commented out
	// started a runner with nothing to run, and the welcome sat on "Starting your stack… 0 of … services up"
	// forever. Once the runner has reported, an empty set of wanted services is a stack that is up.
	test("a stack with nothing to start is up once the runner has reported", () => {
		expect(progress([], true)).toEqual({ up: 0, total: 0, failed: 0, done: true });
		expect(progress([s("stopped", { deselected: true })], true)).toMatchObject({ total: 0, done: true });
		// Reported, but a wanted service is still starting: not done.
		expect(progress([s("starting")], true)).toMatchObject({ done: false });
	});
});

describe("showWelcome: the overlay stays for the whole guided flow", () => {
	// Incident-in-waiting: `isFirstRun` is "no projects and not done". The moment the hero adds the first project the
	// count becomes 1, the overlay unmounts, and the user never sees their stack come up.
	test("a first run shows it while the flow is in progress even though a project now exists", () => {
		for (const p of ["starting", "up"] as Phase[])
			expect(showWelcome({ done: false }, true, 1, p), p).toBe(true);
	});
	test("a fresh first run (no project, welcome never finished) shows it", () => {
		expect(showWelcome({ done: false }, true, 0, "welcome")).toBe(true);
	});
	test("a returning user with projects never sees it, even before the flow state is known", () => {
		expect(showWelcome({ done: false }, true, 3, "welcome")).toBe(false);
		expect(showWelcome({ done: true }, true, 0, "welcome")).toBe(false);
	});
	test("waits for settings and the project list so a returning user never flashes it", () => {
		expect(showWelcome({ done: false }, false, 0, "welcome")).toBe(false);
	});
	test("done hides it", () => {
		expect(showWelcome({ done: false }, true, 1, "done")).toBe(false);
	});
});

describe("escapeEvent: what Esc does at each step (keyboard friendly, never destructive)", () => {
	test("while scanning it stops the scan first, it does not dismiss the whole welcome", () => {
		expect(escapeEvent("scanning")).toEqual({ t: "cancel" });
	});
	test("while a stack is starting Esc does nothing (an accidental key must not abandon it)", () => {
		expect(escapeEvent("starting")).toBeNull();
	});
	test("everywhere else it skips the welcome", () => {
		for (const p of ["welcome", "empty", "results", "up"] as Phase[])
			expect(escapeEvent(p), p).toEqual({ t: "skip" });
	});
	test("done has nothing to escape", () => {
		expect(escapeEvent("done")).toBeNull();
	});
});

describe("primaryEvent: Enter runs the one primary action of the step", () => {
	test("welcome and empty scan, results add, up continues, others have none", () => {
		expect(primaryEvent("welcome")).toBe("scan");
		expect(primaryEvent("empty")).toBe("init");
		expect(primaryEvent("results")).toBe("add");
		expect(primaryEvent("up")).toBe("continue");
		expect(primaryEvent("scanning")).toBeNull();
		expect(primaryEvent("starting")).toBeNull();
		expect(primaryEvent("done")).toBeNull();
	});
});

describe("showStanding: the empty state in the main pane", () => {
	// Incident (live app, scratch HOME): during first run BOTH the full-window overlay and the NoProject pane mounted a
	// Hero, so two canvases ran and both wrote the shared welcome store. The standing state shows only after the welcome.
	test("shows when the user has no project and the welcome is over", () => {
		expect(showStanding({ done: true }, true, 0)).toBe(true);
	});
	test("never shows while the first-run overlay owns the screen", () => {
		expect(showStanding({ done: false }, true, 0)).toBe(false);
	});
	test("never shows with projects, or before the lists are loaded", () => {
		expect(showStanding({ done: true }, true, 2)).toBe(false);
		expect(showStanding({ done: true }, false, 0)).toBe(false);
	});
	test("is exactly the complement of the overlay when there are no projects", () => {
		for (const done of [true, false])
			expect(showStanding({ done }, true, 0)).toBe(!showWelcome({ done }, true, 0, "welcome"));
	});
});

describe("recording hooks: stable selectors and timing for scripted captures", () => {
	test("every phase has a unique test id, derived from the phase name", () => {
		const phases = ["welcome", "scanning", "empty", "results", "starting", "up", "done"];
		const ids = phases.map((p) => testId(p));
		expect(new Set(ids).size).toBe(phases.length);
		for (const [i, p] of phases.entries()) expect(ids[i]).toBe(`welcome-${p}`);
	});
	test("the controls a recording clicks have stable ids", () => {
		expect(ACTIONS).toEqual({
			scan: "welcome-action-scan",
			addFolder: "welcome-action-add-folder",
			init: "welcome-action-init",
			add: "welcome-action-add",
			skip: "welcome-action-skip",
			finish: "welcome-action-finish",
		});
	});
	test("INTRO_TIMING exposes the shot's real numbers (one source of truth with the timeline)", () => {
		expect(INTRO_TIMING.durationMs).toBe(Math.round(DURATION * 1000));
		expect(INTRO_TIMING.hitMs).toBe(Math.round(BEATS.hit * 1000));
		expect(INTRO_TIMING.soundMs).toBe(Math.round((BEATS.hit - SIGNATURE_PREROLL) * 1000));
		expect(INTRO_TIMING.hitMs).toBeLessThan(INTRO_TIMING.durationMs);
	});
});
