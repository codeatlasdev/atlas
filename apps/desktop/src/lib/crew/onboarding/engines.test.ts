// @ts-nocheck
import { describe, test, expect } from "bun:test";
import {
	engineBadge,
	engineBusy,
	engineStatusLabel,
	phaseFromCheck,
	pickReadyEngine,
	statusAfterInstall,
	STEP_META,
	type EnginePhase,
} from "./engines.ts";
import { INFO_STEPS } from "#lib/crew/onboardingFlow.ts";

const PHASES: EnginePhase[] = [
	"idle",
	"checking",
	"not_installed",
	"installing",
	"install_error",
	"needs_signin",
	"ready",
];

describe("engineStatusLabel", () => {
	test("idle reads like checking: the check starts right away", () => {
		expect(engineStatusLabel("idle")).toBe("Checking…");
		expect(engineStatusLabel("checking")).toBe("Checking…");
	});

	test("each action phase names what a click does", () => {
		expect(engineStatusLabel("not_installed")).toBe("Install");
		expect(engineStatusLabel("install_error")).toBe("Retry");
		expect(engineStatusLabel("needs_signin")).toBe("Sign in");
		expect(engineStatusLabel("installing")).toBe("Installing…");
		expect(engineStatusLabel("ready")).toBe("Ready");
	});

	test("every phase has a non-empty label", () => {
		for (const p of PHASES) expect(engineStatusLabel(p).length).toBeGreaterThan(0);
	});
});

describe("engineBadge / engineBusy", () => {
	test("badge follows the phase", () => {
		expect(engineBadge("ready")).toBe("ready");
		expect(engineBadge("not_installed")).toBe("action");
		expect(engineBadge("needs_signin")).toBe("action");
		expect(engineBadge("install_error")).toBe("error");
		expect(engineBadge("installing")).toBe("busy");
		expect(engineBadge("checking")).toBe("busy");
		expect(engineBadge("idle")).toBe("busy");
	});

	test("busy phases are exactly the ones with a spinner", () => {
		expect(PHASES.filter(engineBusy)).toEqual(["idle", "checking", "installing"]);
		for (const p of PHASES) expect(engineBusy(p)).toBe(engineBadge(p) === "busy");
	});
});

describe("phaseFromCheck / statusAfterInstall", () => {
	test("a check maps installed + signed_in to the phase", () => {
		expect(phaseFromCheck({ installed: false, signed_in: false })).toBe("not_installed");
		expect(phaseFromCheck({ installed: false, signed_in: true })).toBe("not_installed");
		expect(phaseFromCheck({ installed: true, signed_in: false })).toBe("needs_signin");
		expect(phaseFromCheck({ installed: true, signed_in: true })).toBe("ready");
	});

	test("after an install a missing binary is an install error with a reason", () => {
		expect(statusAfterInstall({ installed: false, signed_in: false })).toEqual({
			phase: "install_error",
			error: "Install finished but binary not found",
		});
		expect(statusAfterInstall({ installed: true, signed_in: false })).toEqual({
			phase: "needs_signin",
			error: undefined,
		});
		expect(statusAfterInstall({ installed: true, signed_in: true }).phase).toBe("ready");
	});
});

describe("pickReadyEngine", () => {
	const engines = [{ id: "claude" }, { id: "opencode" }, { id: "codex" }];
	const ready = { phase: "ready" };
	const needs = { phase: "needs_signin" };

	test("keeps the picked engine when it is Ready", () => {
		expect(pickReadyEngine("codex", engines, { claude: ready, codex: ready })).toBe("codex");
	});

	test("falls back to the first Ready engine when the pick is not Ready or empty", () => {
		expect(pickReadyEngine("codex", engines, { claude: needs, opencode: ready, codex: needs })).toBe(
			"opencode",
		);
		expect(pickReadyEngine("", engines, { opencode: ready, codex: ready })).toBe("opencode");
	});

	test("is empty when nothing is Ready or nothing was checked", () => {
		expect(pickReadyEngine("claude", engines, { claude: needs })).toBe("");
		expect(pickReadyEngine("", engines, {})).toBe("");
	});
});

describe("STEP_META", () => {
	test("every step has a headline and body; info steps carry their N-of-2 label", () => {
		for (const [step, m] of Object.entries(STEP_META)) {
			expect(m.headline.length).toBeGreaterThan(0);
			expect(m.body.length).toBeGreaterThan(0);
			expect(Boolean(m.label)).toBe(INFO_STEPS.includes(step));
		}
	});
});
