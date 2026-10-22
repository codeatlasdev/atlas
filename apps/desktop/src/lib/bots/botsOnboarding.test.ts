// @ts-nocheck
import { describe, test, expect } from "bun:test";
import {
	INITIAL_STEP,
	nextStep,
	prevStep,
	skipToEnd,
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

	test("continue from engine → done", () => {
		expect(nextStep({ step: "engine", hasHost: false })).toBe("done");
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

	test("back from intro returns null (no prev)", () => {
		expect(prevStep({ step: "intro", hasHost: false })).toBeNull();
	});

	test("skip always returns done", () => {
		for (const step of ["intro", "how1", "how2", "how3", "connect", "engine"] as OnboardingStep[]) {
			expect(skipToEnd(step)).toBe("done");
		}
	});
});

describe("seen flag", () => {
	test("shouldShow returns false when seen", () => {
		const { shouldShow } = require("./botsOnboarding.ts");
		expect(shouldShow(true, false)).toBe(false);
	});

	test("shouldShow returns true when not seen and no bots", () => {
		const { shouldShow } = require("./botsOnboarding.ts");
		expect(shouldShow(false, false)).toBe(true);
	});

	test("shouldShow returns false when already has bots (not first run)", () => {
		const { shouldShow } = require("./botsOnboarding.ts");
		expect(shouldShow(false, true)).toBe(false);
	});
});

describe("engine readiness", () => {
	test("step 6 disables unready engines", () => {
		const { isEngineReady } = require("./botsOnboarding.ts");
		expect(isEngineReady({ id: "claude", label: "Claude Code", installed: true })).toBe(true);
		expect(isEngineReady({ id: "opencode", label: "OpenCode", installed: false })).toBe(false);
	});
});
