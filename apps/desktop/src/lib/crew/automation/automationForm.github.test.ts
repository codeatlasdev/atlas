// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { normalizeTask } from "#lib/crew/wire.ts";
import { AutomationForm } from "./automationForm.svelte.ts";

// hostd's real serializer wrote this file (crates/atlas-host/src/crew/tests/payloads.rs,
// UPDATE_FIXTURES=1): a task from GitHub and a GitHub automation with every field it can carry.
const fixture = JSON.parse(
	readFileSync(join(import.meta.dir, "../../../../../mobile/test/fixtures/github_origin.json"), "utf8"),
);

describe("a task that came from GitHub", () => {
	test("keeps its origin through the wire boundary", () => {
		const out = normalizeTask(fixture.task);
		expect(out.origin).toEqual(fixture.task.origin);
		expect(out.origin.kind).toBe("pr_review_comment");
		expect(out.origin.number).toBe(12);
		expect(out.origin.reply_comment_id).toBe(9001);
	});
});

// Incident guarded: the editor rebuilt a GitHub automation as {type, repo, events}, and
// `crewmate_triggers_set` replaces the whole list, so saving an edit reset `authors` to the default
// and `allow_auto` to false (and would drop `handle` and `assignee`).
describe("editing a GitHub automation", () => {
	test("hands back every field the form has no control for", () => {
		const form = new AutomationForm();
		form.load(fixture.trigger);
		expect(form.toKind()).toEqual(fixture.trigger.kind);
	});

	test("changes only what the user changed", () => {
		const form = new AutomationForm();
		form.load(fixture.trigger);
		form.githubEvents = new Set(["mention"]);
		form.githubRepo = " o/other ";
		expect(form.toKind()).toEqual({ ...fixture.trigger.kind, repo: "o/other", events: ["mention"] });
	});

	test("a new automation carries no stale fields, and another kind carries none either", () => {
		const fresh = new AutomationForm();
		fresh.kind = "github";
		fresh.githubRepo = "o/r";
		fresh.githubEvents = new Set(["release"]);
		expect(fresh.toKind()).toEqual({ type: "github", repo: "o/r", events: ["release"] });

		const form = new AutomationForm();
		form.load(fixture.trigger);
		form.kind = "interval";
		expect(form.toKind().type).toBe("interval");
		expect(form.toKind().authors).toBeUndefined();
	});
});
