// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import type { SeedReport } from "#lib/api.ts";
import { readyLine, seedLine } from "#lib/worktreeTiming.ts";

const report = (over: Partial<SeedReport> = {}): SeedReport => ({
	seeded: [],
	skipped: [],
	ms: 0,
	...over,
});

test("seedLine names what was cloned with its time and what the file system could not clone", () => {
	expect(seedLine(report({ seeded: ["node_modules", "target"], ms: 87 }))).toBe(
		"cloned node_modules, target in 87 ms",
	);
	expect(
		seedLine(
			report({
				seeded: ["node_modules"],
				ms: 5,
				skipped: [["target", { reason: "unsupported" }]],
			}),
		),
	).toBe("cloned node_modules in 5 ms; target skipped (this file system cannot clone)");
});

test("seedLine stays silent for normal skips and names failures", () => {
	expect(
		seedLine(
			report({
				skipped: [
					["target", { reason: "missing" }],
					["node_modules", { reason: "exists" }],
				],
			}),
		),
	).toBeNull();
	expect(seedLine(null)).toBeNull();
	expect(seedLine(report({ skipped: [[".venv", { reason: "not_relocatable" }]] }))).toBe(
		".venv not seeded (a Python virtualenv is not relocatable: recreate it)",
	);
	expect(seedLine(report({ skipped: [[".venv", { reason: "failed", detail: "denied" }]] }))).toBe(
		".venv: denied",
	);
});

test("readyLine adds the setup time after the seed time, and works with either missing", () => {
	const seed = report({ seeded: ["node_modules"], ms: 40 });
	expect(readyLine(seed, 12_340)).toBe("cloned node_modules in 40 ms · setup 12.3 s");
	expect(readyLine(null, 800)).toBe("setup 800 ms");
	expect(readyLine(seed, null)).toBe("cloned node_modules in 40 ms");
	expect(readyLine(null, null)).toBeNull();
	// A boundary: just under and at one second.
	expect(readyLine(null, 999)).toBe("setup 999 ms");
	expect(readyLine(null, 1000)).toBe("setup 1.0 s");
});
