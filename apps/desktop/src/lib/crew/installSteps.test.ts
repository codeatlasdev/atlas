// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { parseInstallLine, parseInstallUser, installSteps, type InstallPhase } from "./installSteps.ts";

describe("parseInstallLine", () => {
	test("returns null for non-marker lines", () => {
		expect(parseInstallLine("")).toBeNull();
		expect(parseInstallLine("some random output")).toBeNull();
		expect(parseInstallLine("\u001b[36m▸\u001b[0m preflight ubuntu@server")).toBeNull();
		expect(parseInstallLine("Error: connection refused")).toBeNull();
	});

	test("parses all known phase keys", () => {
		const keys: InstallPhase[] = ["check", "install", "user", "service", "ssh", "browsers", "done"];
		for (const key of keys) {
			expect(parseInstallLine(`step: ${key}`)).toBe(key);
		}
	});

	test("ignores unknown keys", () => {
		expect(parseInstallLine("step: unknown")).toBeNull();
		expect(parseInstallLine("step: reboot")).toBeNull();
	});

	test("requires the line to start with 'step: '", () => {
		expect(parseInstallLine(" step: check")).toBeNull();
		expect(parseInstallLine("prefix step: check")).toBeNull();
	});

	// "signin" is the desktop's own first step: the CLI never prints a marker for it, so it cannot be parsed.
	test("the checklist starts with the desktop's sign-in and covers the six CLI phases", () => {
		const INSTALL_STEPS = installSteps(false);
		expect(INSTALL_STEPS).toHaveLength(7);
		const phases = INSTALL_STEPS.map((s) => s.phase);
		expect(phases[0]).toBe("signin");
		expect(phases).toContain("check");
		expect(phases).toContain("done");
		expect(parseInstallLine("step: signin")).toBeNull();
		for (const s of INSTALL_STEPS) expect(s.detail.length).toBeGreaterThan(0);
	});

	// `atlas crew install --browsers` prints `step: browsers` after `ssh` and before `done`; the checklist shows that
	// step only when the user opted in.
	test("the browsers step is in the checklist only when asked for, between ssh and done", () => {
		expect(installSteps(false).map((s) => s.phase)).not.toContain("browsers");
		const phases = installSteps(true).map((s) => s.phase);
		expect(phases).toEqual(["signin", "check", "install", "user", "service", "ssh", "browsers", "done"]);
	});
});

describe("parseInstallUser", () => {
	test("reads the service user from the `user: <name>` marker", () => {
		expect(parseInstallUser("user: atlas-crew")).toBe("atlas-crew");
		expect(parseInstallUser("user: atlas-bot")).toBe("atlas-bot");
		expect(parseInstallUser("user: atlas-crew  ")).toBe("atlas-crew");
	});

	test("ignores every other line, including step markers and look-alikes", () => {
		expect(parseInstallUser("step: user")).toBeNull();
		expect(parseInstallUser("user: ")).toBeNull();
		expect(parseInstallUser("user: root; rm -rf /")).toBeNull();
		expect(parseInstallUser(" user: atlas-crew")).toBeNull();
		expect(parseInstallUser("created user: atlas-crew")).toBeNull();
	});
});
