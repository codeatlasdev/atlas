// scripts/heavy.sh decides whether two heavy jobs (builds, the gate, benches) may run at once. Overlap froze the
// machine (2026-10-15) and made timing tests flaky (PR #106); a lock that outlives its owner would block every job.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dir, "heavy.sh");

describe("heavy.sh", () => {
	test("two jobs started together run one after the other, and both run", async () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const log = join(dir, "log");
		const env = { ...process.env, ATLAS_HEAVY_LOCK: join(dir, "lock"), ATLAS_HEAVY_MAXLOAD: "100000" };
		const job = (n: string) =>
			Bun.spawn([script, "sh", "-c", `echo start-${n} >> ${log}; sleep 1; echo end-${n} >> ${log}`], { env });
		const [a, b] = [job("a"), job("b")];
		expect([await a.exited, await b.exited]).toEqual([0, 0]);
		const lines = readFileSync(log, "utf8").trim().split("\n");
		expect(lines).toHaveLength(4);
		// Whichever went first finished before the other started.
		expect(lines[1]).toBe(lines[0].replace("start", "end"));
		expect(lines[2].startsWith("start-")).toBe(true);
	});

	test("a lock left by a job that died does not block the next one, and the exit code passes through", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const lock = join(dir, "lock");
		writeFileSync(lock, "999999\n"); // shlock's pid file of a process that no longer exists
		const p = Bun.spawnSync([script, "sh", "-c", "exit 3"], {
			env: { ...process.env, ATLAS_HEAVY_LOCK: lock, ATLAS_HEAVY_MAXLOAD: "100000" },
			timeout: 10_000,
		});
		expect(p.exitCode).toBe(3);
	});

	test("a machine that never calms down still runs the job after the wait, and says so", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const p = Bun.spawnSync([script, "sh", "-c", "echo ran"], {
			env: { ...process.env, ATLAS_HEAVY_LOCK: join(dir, "lock"), ATLAS_HEAVY_MAXLOAD: "0", ATLAS_HEAVY_WAIT: "0" },
			timeout: 10_000,
		});
		expect(p.exitCode).toBe(0);
		expect(p.stdout.toString().trim()).toBe("ran");
		expect(p.stderr.toString()).toContain("running anyway");
	});
});
