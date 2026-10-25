// @ts-nocheck
import { describe, test, expect } from "bun:test";
import {
	INITIAL_STEP,
	nextStep,
	prevStep,
	skipToEnd,
	shouldShow,
	isEngineReady,
	infoStepIndex,
	stepProgress,
	INFO_STEPS,
	type OnboardingStep,
} from "./botsOnboarding.ts";

// The three "how it works" screens became one (`how`): the user found three thin steps slow and unclear.
describe("step machine", () => {
	test("initial step is intro", () => {
		expect(INITIAL_STEP).toBe("intro");
	});

	test("intro → how", () => {
		expect(nextStep({ step: "intro", hasHost: false })).toBe("how");
	});

	test("how → connect when there is no ship yet", () => {
		expect(nextStep({ step: "how", hasHost: false })).toBe("connect");
	});

	test("how → engine when a ship is already connected (connect skipped)", () => {
		expect(nextStep({ step: "how", hasHost: true })).toBe("engine");
	});

	test("connect → engine → ready → done", () => {
		expect(nextStep({ step: "connect", hasHost: false })).toBe("engine");
		expect(nextStep({ step: "engine", hasHost: false })).toBe("ready");
		expect(nextStep({ step: "ready", hasHost: false })).toBe("done");
	});

	test("back: how → intro, connect → how", () => {
		expect(prevStep({ step: "how", hasHost: false })).toBe("intro");
		expect(prevStep({ step: "connect", hasHost: false })).toBe("how");
	});

	test("back from engine goes to connect, or to how when connect was skipped", () => {
		expect(prevStep({ step: "engine", hasHost: false })).toBe("connect");
		expect(prevStep({ step: "engine", hasHost: true })).toBe("how");
	});

	test("back from ready → engine; nothing before intro", () => {
		expect(prevStep({ step: "ready", hasHost: false })).toBe("engine");
		expect(prevStep({ step: "intro", hasHost: false })).toBeNull();
	});

	test("skip always returns done", () => {
		for (const step of ["intro", "how", "connect", "engine", "ready"] as OnboardingStep[]) {
			expect(skipToEnd(step)).toBe("done");
		}
	});
});

describe("seen flag", () => {
	test("shouldShow returns false when seen", () => {
		expect(shouldShow(true, false)).toBe(false);
	});

	test("shouldShow returns true when not seen and no bots", () => {
		expect(shouldShow(false, false)).toBe(true);
	});

	test("shouldShow returns false when already has bots (not first run)", () => {
		expect(shouldShow(false, true)).toBe(false);
	});
});

describe("engine readiness", () => {
	test("unready engines are disabled", () => {
		expect(isEngineReady({ id: "claude", label: "Claude Code", installed: true })).toBe(true);
		expect(isEngineReady({ id: "opencode", label: "OpenCode", installed: false })).toBe(false);
	});
});

describe("step labels and progress", () => {
	test("two info steps, labeled 1 and 2", () => {
		expect(INFO_STEPS).toEqual(["intro", "how"]);
		expect(infoStepIndex("intro")).toBe(0);
		expect(infoStepIndex("how")).toBe(1);
		expect(infoStepIndex("connect")).toBe(-1);
		expect(infoStepIndex("engine")).toBe(-1);
		expect(infoStepIndex("ready")).toBe(-1);
	});

	test("progress across 5 steps without a ship", () => {
		expect(stepProgress({ step: "intro", hasHost: false })).toBeCloseTo(1 / 5);
		expect(stepProgress({ step: "how", hasHost: false })).toBeCloseTo(2 / 5);
		expect(stepProgress({ step: "connect", hasHost: false })).toBeCloseTo(3 / 5);
		expect(stepProgress({ step: "ready", hasHost: false })).toBe(1);
	});

	test("progress across 4 steps with a ship (connect skipped)", () => {
		expect(stepProgress({ step: "intro", hasHost: true })).toBeCloseTo(1 / 4);
		expect(stepProgress({ step: "engine", hasHost: true })).toBeCloseTo(3 / 4);
		expect(stepProgress({ step: "ready", hasHost: true })).toBe(1);
	});
});
