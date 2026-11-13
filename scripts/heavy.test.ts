// scripts/heavy.sh has two paths. With an atlas that has the machine-wide queue it hands the command to
// `atlas run --heavy` (the queue itself is tested in crates/atlas-cli/tests/heavy.rs against the real binary).
// With an installed atlas that predates the queue, or none, it runs the previous per-user lock + load wait, so the
// repo's atlas.yaml (whose actions call the script) works on any installed version. A job that overlaps another
// froze the machine (2026-10-15) and made timing tests flaky (PR #106); a lock that outlives its owner would block
// every job.
import { describe, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dir, "heavy.sh");

/**
 * A stand-in `atlas` that logs its argv one per line to `log`, then exits with `code`. `queue: false` makes it an
 * atlas from before the queue: `heavy` is an unknown subcommand (exit 2), as clap answers.
 */
function fakeAtlas(dir: string, name: string, log: string, { code = 0, queue = true } = {}): string {
	const p = join(dir, name);
	const unknown = queue ? "" : `[ "$1" = heavy ] && { echo "error: unrecognized subcommand 'heavy'" >&2; exit 2; }\n`;
	writeFileSync(
		p,
		`#!/bin/sh\n${unknown}for a in "$@"; do printf '%s\\n' "$a" >> ${JSON.stringify(log)}; done\nexit ${code}\n`,
	);
	chmodSync(p, 0o755);
	return p;
}

/** A PATH with only the basics, so a developer's real atlas never leaks into a test. */
const bare = "/usr/bin:/bin";
const run = (args: string[], env: Record<string, string>) =>
	Bun.spawnSync([script, ...args], { env: { PATH: bare, HOME: env.HOME ?? "/nonexistent", ...env }, timeout: 10_000 });

/** Env that keeps the fallback away from the real machine: its own lock, no load gate, a memory reading of its own. */
const fallbackEnv = (dir: string) => ({
	HOME: dir,
	ATLAS_HEAVY_LOCK: join(dir, "lock"),
	ATLAS_HEAVY_MAXLOAD: "100000",
	ATLAS_HEAVY_MEMFREE: "100",
});

describe("heavy.sh with an atlas that has the queue", () => {
	test("hands the command to `atlas run --heavy`, labelled by its program, arguments intact", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const log = join(dir, "argv");
		const atlas = fakeAtlas(dir, "atlas", log);
		const p = run(["bun", "run", "check:changed", "a b"], { ATLAS_BIN: atlas });
		expect(p.exitCode).toBe(0);
		// The probe (`heavy status --json`) is not logged by the queue-less branch of the fake; the run is.
		expect(readFileSync(log, "utf8").trimEnd().split("\n")).toEqual([
			"heavy", "status", "--json", "run", "--heavy", "--label", "bun", "--", "bun", "run", "check:changed", "a b",
		]);
	});

	test("the exit code of the CLI (the command's own) is the script's", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		// Answers the probe (`heavy status`) with 0 and `run` with 3, like the real CLI does for a failing command.
		const p = join(dir, "atlas");
		writeFileSync(p, '#!/bin/sh\n[ "$1" = heavy ] && exit 0\nexit 3\n');
		chmodSync(p, 0o755);
		expect(run(["true"], { ATLAS_BIN: p }).exitCode).toBe(3);
	});

	test("ATLAS_BIN wins over the atlas on PATH, and PATH wins over ~/.atlas/bin", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const home = join(dir, "home");
		mkdirSync(join(home, ".atlas/bin"), { recursive: true });
		const bin = join(dir, "bin");
		mkdirSync(bin);
		const ran = (which: string) => existsSync(join(dir, which));
		fakeAtlas(join(home, ".atlas/bin"), "atlas", join(dir, "home-bin"));
		fakeAtlas(bin, "atlas", join(dir, "on-path"));
		const override = fakeAtlas(dir, "override-atlas", join(dir, "override"));

		run(["x"], { ATLAS_BIN: override, PATH: `${bin}:${bare}`, HOME: home });
		expect([ran("override"), ran("on-path"), ran("home-bin")]).toEqual([true, false, false]);
		run(["x"], { PATH: `${bin}:${bare}`, HOME: home });
		expect([ran("on-path"), ran("home-bin")]).toEqual([true, false]);
		run(["x"], { HOME: home });
		expect(ran("home-bin")).toBe(true);
	});

	test("a queue-less atlas first in line is skipped for a later one that has the queue", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const bin = join(dir, "bin");
		mkdirSync(bin);
		const old = fakeAtlas(dir, "old-atlas", join(dir, "old"), { queue: false });
		fakeAtlas(bin, "atlas", join(dir, "new"));
		const p = run(["x"], { ATLAS_BIN: old, PATH: `${bin}:${bare}`, HOME: dir });
		expect(p.exitCode).toBe(0);
		expect(existsSync(join(dir, "old"))).toBe(false);
		expect(readFileSync(join(dir, "new"), "utf8")).toContain("--heavy");
	});
});

describe("heavy.sh with an atlas that predates the queue, or none", () => {
	test("never calls `run --heavy` on it: the previous implementation runs the command, exit code through", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const log = join(dir, "argv");
		const old = fakeAtlas(dir, "atlas", log, { queue: false });
		const p = run(["sh", "-c", "echo ran; exit 3"], { ATLAS_BIN: old, ...fallbackEnv(dir) });
		expect(p.exitCode).toBe(3);
		expect(p.stdout.toString().trim()).toBe("ran");
		expect(existsSync(log)).toBe(false); // the old atlas never saw `run`
	});

	test("two jobs started together run one after the other, and both run", async () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const log = join(dir, "log");
		const env = { PATH: bare, ...fallbackEnv(dir) };
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

	test("a finished job releases the lock, so the next one starts without waiting for a dead owner", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const env = fallbackEnv(dir);
		expect(run(["true"], env).exitCode).toBe(0);
		// shlock keeps a pid file, the mkdir fallback a directory: either way nothing may remain.
		expect(existsSync(env.ATLAS_HEAVY_LOCK) || existsSync(`${env.ATLAS_HEAVY_LOCK}.d`)).toBe(false);
	});

	test("a lock left by a job that died does not block the next one, and the exit code passes through", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const env = fallbackEnv(dir);
		writeFileSync(env.ATLAS_HEAVY_LOCK, "999999\n"); // shlock's pid file of a process that no longer exists
		const p = run(["sh", "-c", "exit 3"], env);
		expect(p.exitCode).toBe(3);
	});

	test("a machine that never calms down still runs the job after the wait, and says so", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const p = run(["sh", "-c", "echo ran"], { ...fallbackEnv(dir), ATLAS_HEAVY_MAXLOAD: "0", ATLAS_HEAVY_WAIT: "0" });
		expect(p.exitCode).toBe(0);
		expect(p.stdout.toString().trim()).toBe("ran");
		expect(p.stderr.toString()).toContain("running anyway");
	});

	test("memory pressure holds the job like load does, then it runs anyway after the wait, saying why", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const p = run(["sh", "-c", "echo ran"], {
			...fallbackEnv(dir),
			ATLAS_HEAVY_MEMFREE: "7",
			ATLAS_HEAVY_WAIT: "0",
		});
		expect(p.exitCode).toBe(0);
		expect(p.stdout.toString().trim()).toBe("ran");
		expect(p.stderr.toString()).toContain("only 7% of memory free (< 20%) after 0s; running anyway");
	});

	test("enough free memory, a lowered threshold or the gate off let the job start at once", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const quiet = (extra: Record<string, string>) => {
			const p = run(["sh", "-c", "echo ran"], { ...fallbackEnv(dir), ATLAS_HEAVY_WAIT: "0", ...extra });
			expect(p.stdout.toString().trim()).toBe("ran");
			return p.stderr.toString();
		};
		expect(quiet({ ATLAS_HEAVY_MEMFREE: "20" })).not.toContain("memory"); // exactly the threshold is enough
		expect(quiet({ ATLAS_HEAVY_MEMFREE: "7", ATLAS_HEAVY_MINMEM: "5" })).not.toContain("memory");
		expect(quiet({ ATLAS_HEAVY_MEMFREE: "0", ATLAS_HEAVY_MINMEM: "0" })).not.toContain("memory");
	});

	test("the load threshold defaults to 0.75 * cores and a job caps CARGO_BUILD_JOBS unless the user did", () => {
		const dir = mkdtempSync(join(tmpdir(), "heavy-"));
		const env = { ...fallbackEnv(dir), ATLAS_HEAVY_MEMFREE: "90", ATLAS_HEAVY_WAIT: "0" };
		delete env.ATLAS_HEAVY_MAXLOAD; // the default threshold
		// A load of 1000 is above 0.75 * any machine's cores: the wait message shows the threshold in use.
		const fakeBin = join(dir, "bin");
		mkdirSync(fakeBin);
		writeFileSync(join(fakeBin, "uptime"), "#!/bin/sh\necho ' 10:00  up 1 day, load averages: 1000.00 1000.00 1000.00'\n");
		chmodSync(join(fakeBin, "uptime"), 0o755);
		const cores = Number(Bun.spawnSync(["getconf", "_NPROCESSORS_ONLN"]).stdout.toString().trim());
		const p = run(["sh", "-c", "echo jobs=$CARGO_BUILD_JOBS"], { ...env, PATH: `${fakeBin}:${bare}` });
		expect(p.stderr.toString()).toContain(`load 1000 >= ${Math.max(1, Math.floor((cores * 3) / 4))}`);
		expect(p.stdout.toString().trim()).toBe(`jobs=${cores}`);
		const own = run(["sh", "-c", "echo jobs=$CARGO_BUILD_JOBS"], { ...env, CARGO_BUILD_JOBS: "3" });
		expect(own.stdout.toString().trim()).toBe("jobs=3");
	});
});
