// @ts-nocheck
import { describe, test, expect } from "bun:test";
import {
	applyInstallLine,
	needsSudoPassword,
	completeInstall,
	initialInstallProgress,
} from "./installProgress.ts";
import { installSteps } from "#lib/crew/installSteps.ts";
import { crewServerTarget } from "#lib/crew/sshTarget.ts";

const INSTALL_STEPS = installSteps(false);

const fold = (lines: string[]) => lines.reduce(applyInstallLine, initialInstallProgress());

describe("applyInstallLine", () => {
	test("starts empty", () => {
		const p = initialInstallProgress();
		expect(p.done.size).toBe(0);
		expect(p.active).toBeNull();
		expect(p.user).toBe("");
	});

	test("a step marker spins that phase and ticks the previous one", () => {
		const a = fold(["step: check"]);
		expect(a.active).toBe("check");
		expect([...a.done]).toEqual([]);
		const b = fold(["step: check", "step: install"]);
		expect(b.active).toBe("install");
		expect([...b.done]).toEqual(["check"]);
	});

	test("the `done` marker ticks every phase and stops the spinner", () => {
		const p = fold(["step: check", "step: install", "step: done"]);
		expect(p.active).toBeNull();
		expect([...p.done].sort()).toEqual(INSTALL_STEPS.map((s) => s.phase).sort());
	});

	test("with --browsers the phase spins after ssh and ticks when `done` arrives", () => {
		const during = fold(["step: ssh", "step: browsers"]);
		expect(during.active).toBe("browsers");
		expect(during.done.has("ssh")).toBe(true);
		const p = fold(["step: ssh", "step: browsers", "step: done"]);
		expect([...p.done].sort()).toEqual(
			installSteps(true)
				.map((s) => s.phase)
				.sort(),
		);
	});

	test("progress output and unknown markers change nothing", () => {
		const before = fold(["step: check"]);
		const after = fold(["step: check", "Downloading atlas…", "step: reboot", ""]);
		expect(after.active).toBe(before.active);
		expect([...after.done]).toEqual([...before.done]);
	});

	test("a `user:` marker records the service user without touching the phases", () => {
		const p = fold(["step: user", "user: atlas-bot"]);
		expect(p.user).toBe("atlas-bot");
		expect(p.active).toBe("user");
	});

	test("the reported user survives later markers and `done`", () => {
		const p = fold(["step: user", "user: atlas-bot", "step: service", "step: done"]);
		expect(p.user).toBe("atlas-bot");
	});

	test("look-alike user lines are ignored", () => {
		expect(fold(["created user: atlas-crew", "user: root; rm -rf /"]).user).toBe("");
	});

	test("does not mutate the previous state", () => {
		const a = fold(["step: check"]);
		const snapshot = [...a.done];
		applyInstallLine(a, "step: install");
		expect([...a.done]).toEqual(snapshot);
		expect(a.active).toBe("check");
	});

	// The desktop's own first step ("Signing in") spins before the CLI starts; the CLI's first marker ticks it.
	test("the sign-in step is ticked by the CLI's first marker", () => {
		const start = { ...initialInstallProgress(), active: "signin" };
		const p = ["step: check"].reduce(applyInstallLine, start);
		expect(p.active).toBe("check");
		expect(p.done.has("signin")).toBe(true);
		expect([...completeInstall(p).done]).toContain("signin");
	});
});

describe("completeInstall", () => {
	test("ticks everything and keeps the user", () => {
		const p = completeInstall({ done: new Set(), active: "ssh", user: "atlas-bot" });
		expect(p.done.size).toBe(INSTALL_STEPS.length);
		expect(p.active).toBeNull();
		expect(p.user).toBe("atlas-bot");
	});
});

describe("target after install", () => {
	test("a server that kept atlas-bot is registered as atlas-bot, a fresh one as atlas-crew", () => {
		const kept = fold(["user: atlas-bot"]).user;
		expect(crewServerTarget("ubuntu@10.0.0.5", kept || undefined)).toBe("atlas-bot@10.0.0.5");
		const fresh = fold(["step: check"]).user;
		expect(crewServerTarget("ubuntu@10.0.0.5", fresh || undefined)).toBe("atlas-crew@10.0.0.5");
	});
});

// The browsers page reads the CLI's text (a string); onboarding gets the classified `ConnectError` instead
// (connectError.test.ts).
describe("needsSudoPassword", () => {
	test("every sudo password failure asks for the sudo password", () => {
		expect(needsSudoPassword("sudo asks for a password")).toBe(true);
		expect(needsSudoPassword("this user needs passwordless sudo")).toBe(true);
		expect(needsSudoPassword("sudo password rejected")).toBe(true);
	});

	test("anything else is a plain error, a refused key included", () => {
		expect(needsSudoPassword("ubuntu@host: Permission denied (publickey).")).toBe(false);
		expect(needsSudoPassword("connection refused")).toBe(false);
		expect(needsSudoPassword("")).toBe(false);
	});
});
