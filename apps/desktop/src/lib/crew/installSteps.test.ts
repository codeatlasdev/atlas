// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { parseInstallLine, parseInstallUser, INSTALL_STEPS, type InstallPhase } from "./installSteps.ts";

describe("parseInstallLine", () => {
	test("returns null for non-marker lines", () => {
		expect(parseInstallLine("")).toBeNull();
		expect(parseInstallLine("some random output")).toBeNull();
		expect(parseInstallLine("\u001b[36m▸\u001b[0m preflight ubuntu@server")).toBeNull();
		expect(parseInstallLine("Error: connection refused")).toBeNull();
	});

	test("parses all known phase keys", () => {
		const keys: InstallPhase[] = ["check", "install", "user", "service", "ssh", "done"];
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

	test("INSTALL_STEPS covers all 6 phases", () => {
		expect(INSTALL_STEPS).toHaveLength(6);
		const phases = INSTALL_STEPS.map((s) => s.phase);
		expect(phases).toContain("check");
		expect(phases).toContain("done");
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
