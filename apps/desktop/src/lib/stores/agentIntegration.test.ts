// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { promptFor } from "./agentIntegration.ts";

const a = (id: string, state: string) => ({ id, label: id, setup: id, state });

// Only omp was ever offered, so Claude Code and Codex users were never told their agents
// report nothing to Atlas or the phone.
test("a missing Claude Code integration is offered once omp is set up", () => {
	expect(promptFor([a("omp", "ok"), a("claude", "missing")], () => false)?.id).toBe("claude");
});

test("a dismissed or unknown integration is never offered", () => {
	expect(promptFor([a("omp", "missing")], (id) => id === "omp")).toBeNull();
	expect(promptFor([a("omp", "unknown")], () => false)).toBeNull();
});
