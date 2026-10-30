// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { projectForShip } from "./projectRef.ts";

// Incident: picking "ac" sent the ship /Users/luancoleto/Projetos/codeatlasdev/ac; the agent started in a folder
// that does not exist there and every task ended "agent exited (code 1)".
const ac = { name: "ac", path: "/Users/luancoleto/Projetos/codeatlasdev/ac", branch: "feat/portal-cliente" };

describe("projectForShip", () => {
	test("a ship gets the git origin and branch, never the Mac path", () => {
		const r = projectForShip(ac, "https://github.com/codeatlasdev/ac.git", "ssh:atlas-bot@atlas-test-ship");
		expect(r).toEqual({
			ok: true,
			project: {
				kind: "git",
				name: "ac",
				url: "https://github.com/codeatlasdev/ac.git",
				branch: "feat/portal-cliente",
			},
		});
	});

	test("a project with no remote is an error for a ship, not a Mac path it cannot open", () => {
		const r = projectForShip(ac, null, "ssh:atlas-bot@atlas-test-ship");
		expect(r.ok).toBe(false);
		expect(r.error).toContain("no git remote");
	});

	test("this Mac's own hostd keeps the local path", () => {
		const r = projectForShip(ac, "https://github.com/codeatlasdev/ac.git", "");
		expect(r).toEqual({
			ok: true,
			project: { kind: "path", name: "ac", path: ac.path, branch: "feat/portal-cliente" },
		});
	});
});
