// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { actionLabel, elapsed, finishedLabel } from "./boardText.ts";

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
		expect(actionLabel(card({ kind: "something_new" }))).toBe("Needs input");
	});
});
