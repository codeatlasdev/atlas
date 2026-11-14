// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { projectForServer } from "./projectRef.ts";

// Incident: picking "ac" sent the crew server /Users/luancoleto/Projetos/codeatlasdev/ac; the agent started in a folder
// that does not exist there and every task ended "agent exited (code 1)".
const ac = { name: "ac", path: "/Users/luancoleto/Projetos/codeatlasdev/ac", branch: "feat/portal-cliente" };

describe("projectForServer", () => {
	test("a crew server gets the git origin and branch, never the Mac path", () => {
		const r = projectForServer(
			ac,
			"https://github.com/codeatlasdev/ac.git",
			"ssh:atlas-crew@atlas-test-server",
		);
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

	test("a project with no remote is an error for a crew server, not a Mac path it cannot open", () => {
		const r = projectForServer(ac, null, "ssh:atlas-crew@atlas-test-server");
		expect(r.ok).toBe(false);
		expect(r.error).toContain("no git remote");
	});

	test("this Mac's own hostd keeps the local path", () => {
		const r = projectForServer(ac, "https://github.com/codeatlasdev/ac.git", "");
		expect(r).toEqual({
			ok: true,
			project: { kind: "path", name: "ac", path: ac.path, branch: "feat/portal-cliente" },
		});
	});
});
