// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../../test/locale.ts";
import { actionLabel, elapsed, finishedLabel, queuedLabel } from "./boardText.ts";

describe("elapsed", () => {
	test("seconds, minutes, minutes with seconds", () => {
		expect(elapsed(0, 45_000)).toBe("45s");
		expect(elapsed(0, 120_000)).toBe("2m");
		expect(elapsed(0, 83_000)).toBe("1m 23s");
	});
	test("no start time or a start in the future is empty / zero", () => {
		expect(elapsed(undefined, 5_000)).toBe("0s");
		expect(elapsed(10_000, 5_000)).toBe("");
	});
});

describe("finishedLabel", () => {
	test("no timestamp is empty, a timestamp is a time string", () => {
		expect(finishedLabel(undefined)).toBe("");
		expect(finishedLabel(1_700_000_000_000)).not.toBe("");
	});
});

describe("actionLabel", () => {
	const card = (action, pending_approval = false) => ({ task: { action }, crewmate: { pending_approval } });
	test("a missing action is approval or generic input", () => {
		expect(actionLabel(card(undefined, true))).toBe("Wants approval");
		expect(actionLabel(card(undefined))).toBe("Needs input");
	});
	test("each action kind has its line", () => {
		expect(actionLabel(card({ kind: "input", label: "Your name?" }))).toBe("Your name?");
		expect(actionLabel(card({ kind: "choose", question: "Which one?" }))).toBe("Which one?");
		expect(actionLabel(card({ kind: "connect_github" }))).toBe("Connect GitHub");
		expect(actionLabel(card({ kind: "provide_secret", label: "API key" }))).toBe("API key");
		expect(actionLabel(card({ kind: "sign_in_engine", engine: "claude" }))).toBe("Sign in to claude");
		expect(actionLabel(card({ kind: "add_trigger" }))).toBe("Add trigger");
		expect(actionLabel(card({ kind: "refused", reasons: ["x"] }))).toBe("Work refused");
		expect(actionLabel(card({ kind: "something_new" }))).toBe("Needs input");
	});
});

describe("queuedLabel", () => {
	test("a plain queue is Queued, a task waiting for something says what", () => {
		expect(queuedLabel({})).toBe("Queued");
		expect(queuedLabel({ waiting_for: undefined })).toBe("Queued");
		expect(queuedLabel({ waiting_for: "server busy" })).toBe("Queued · server busy");
	});

	// hostd's `waiting_for` is its own English word, not a message: the one value it sends today (the `BUSY` constant of
	// crates/atlas-host/src/crew/runner/slots.rs, read here from the real source) must be the catalog's message in the
	// user's language, and a value this build has never heard of is shown as sent.
	const slots = readFileSync(
		join(import.meta.dir, "../../../../../../crates/atlas-host/src/crew/runner/slots.rs"),
		"utf8",
	);
	const busy = /const BUSY: &str = "([^"]+)";/.exec(slots)?.[1];

	test("hostd's busy word is translated, not shown raw", async () => {
		expect(busy).toBeTruthy();
		await inLocale("pt-BR", () => {
			expect(queuedLabel({ waiting_for: busy })).toBe(m.crew_workspace_board_queued_server_busy());
			expect(queuedLabel({ waiting_for: busy })).not.toContain(busy);
		});
	});

	test("a reason it does not know is kept as sent", async () => {
		await inLocale("pt-BR", () => {
			expect(queuedLabel({ waiting_for: "disk full" })).toBe(
				m.crew_workspace_board_queued_reason({ reason: "disk full" }),
			);
			expect(queuedLabel({ waiting_for: "disk full" })).toContain("disk full");
		});
	});
});
