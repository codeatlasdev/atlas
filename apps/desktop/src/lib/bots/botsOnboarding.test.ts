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
	type OnboardingState,
} from "./botsOnboarding.ts";

describe("step machine", () => {
	test("initial step is intro", () => {
		expect(INITIAL_STEP).toBe("intro");
	});

	test("continue from intro → how1", () => {
		expect(nextStep({ step: "intro", hasHost: false })).toBe("how1");
	});

	test("continue from how1 → how2", () => {
		expect(nextStep({ step: "how1", hasHost: false })).toBe("how2");
	});

	test("continue from how2 → how3", () => {
		expect(nextStep({ step: "how2", hasHost: false })).toBe("how3");
	});

	test("continue from how3 → connect (no host)", () => {
		expect(nextStep({ step: "how3", hasHost: false })).toBe("connect");
	});

	test("step 5 skipped when host exists: how3 → engine", () => {
		expect(nextStep({ step: "how3", hasHost: true })).toBe("engine");
	});

	test("continue from connect → engine", () => {
		expect(nextStep({ step: "connect", hasHost: false })).toBe("engine");
	});

	test("continue from engine → ready", () => {
		expect(nextStep({ step: "engine", hasHost: false })).toBe("ready");
	});

	test("continue from ready → done (terminal)", () => {
		expect(nextStep({ step: "ready", hasHost: false })).toBe("done");
	});

	test("back from how2 → how1", () => {
		expect(prevStep({ step: "how2", hasHost: false })).toBe("how1");
	});

	test("back from how3 → how2", () => {
		expect(prevStep({ step: "how3", hasHost: false })).toBe("how2");
	});

	test("back from connect → how3", () => {
		expect(prevStep({ step: "connect", hasHost: false })).toBe("how3");
	});

	test("back from engine (no host) → connect", () => {
		expect(prevStep({ step: "engine", hasHost: false })).toBe("connect");
	});

	test("back from engine (has host) → how3", () => {
		expect(prevStep({ step: "engine", hasHost: true })).toBe("how3");
	});

	test("back from ready → engine", () => {
		expect(prevStep({ step: "ready", hasHost: false })).toBe("engine");
	});

	test("back from intro returns null (no prev)", () => {
		expect(prevStep({ step: "intro", hasHost: false })).toBeNull();
	});

	test("skip always returns done", () => {
		for (const step of ["intro", "how1", "how2", "how3", "connect", "engine", "ready"] as OnboardingStep[]) {
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
	test("step 6 disables unready engines", () => {
		expect(isEngineReady({ id: "claude", label: "Claude Code", installed: true })).toBe(true);
		expect(isEngineReady({ id: "opencode", label: "OpenCode", installed: false })).toBe(false);
	});
});

describe("step labels and progress", () => {
	test("info steps are labeled 1–4", () => {
		expect(INFO_STEPS).toHaveLength(4);
		expect(infoStepIndex("intro")).toBe(0);
		expect(infoStepIndex("how1")).toBe(1);
		expect(infoStepIndex("how2")).toBe(2);
		expect(infoStepIndex("how3")).toBe(3);
	});

	test("non-info steps return -1", () => {
		expect(infoStepIndex("connect")).toBe(-1);
		expect(infoStepIndex("engine")).toBe(-1);
		expect(infoStepIndex("ready")).toBe(-1);
	});

	test("progress fills across all steps (no host)", () => {
		// 7 steps without skip
		expect(stepProgress({ step: "intro", hasHost: false })).toBeCloseTo(1 / 7);
		expect(stepProgress({ step: "how1", hasHost: false })).toBeCloseTo(2 / 7);
		expect(stepProgress({ step: "connect", hasHost: false })).toBeCloseTo(5 / 7);
		expect(stepProgress({ step: "ready", hasHost: false })).toBe(1);
	});

	test("progress fills across all steps (has host, connect skipped)", () => {
		// 6 steps with skip
		expect(stepProgress({ step: "intro", hasHost: true })).toBeCloseTo(1 / 6);
		expect(stepProgress({ step: "engine", hasHost: true })).toBeCloseTo(5 / 6);
		expect(stepProgress({ step: "ready", hasHost: true })).toBe(1);
	});
});
