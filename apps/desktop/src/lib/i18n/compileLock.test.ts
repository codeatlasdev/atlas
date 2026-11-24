// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	mkdtempSync,
	mkdirSync,
	readFileSync,
	rmSync,
	writeFileSync,
	existsSync,
	readdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Incident (LinuxFixes, PR #743): `bun test --parallel` runs the preload of every test file in its own process, and the
// preload recompiles `src/lib/paraglide` when the catalogs changed. Each process cleared and rewrote the same folder, so
// while one compiled the others found `#lib/paraglide/messages/_index.js` missing: "3 fail, 3 errors" (Cannot find
// module) on the first run after ANY catalog edit, and green on the next. The compile must happen once, and every
// other process must wait for it instead of reading a folder that is half written.
const LOCK = join(import.meta.dir, "../../../scripts/compile-lock.ts");

// A child process: stale until `out/done` exists; "compiling" empties `out`, writes `done` FIRST (the compiler's
// messages.js / runtime.js, which is all `isStale` looks at) and the rest of the folder 300 ms later (messages/_index.js).
const CHILD = (dir: string) => `
import { withCompileLock } from ${JSON.stringify(LOCK)};
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { join } from "node:path";
const dir = ${JSON.stringify(dir)}, out = join(dir, "out");
await withCompileLock(join(dir, "lock"), () => !existsSync(join(out, "done")), async () => {
  appendFileSync(join(dir, "builds"), "x");
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out);
  writeFileSync(join(out, "done"), "1");
  await Bun.sleep(300);
  writeFileSync(join(out, "rest"), "1");
});
// The contract: when it returns, the output is complete and nobody is rewriting it.
process.exit(existsSync(join(out, "done")) && existsSync(join(out, "rest")) ? 0 : 7);
`;

test("six processes that find the output stale compile it once, and none returns before it is complete", async () => {
	const dir = mkdtempSync(join(tmpdir(), "compile-lock-"));
	const code = CHILD(dir);
	writeFileSync(join(dir, "child.ts"), code);
	const procs = Array.from({ length: 6 }, () =>
		Bun.spawn(["bun", join(dir, "child.ts")], { stdout: "pipe", stderr: "pipe" }),
	);
	const exits = await Promise.all(procs.map((p) => p.exited));
	expect(exits).toEqual([0, 0, 0, 0, 0, 0]);
	expect(readFileSync(join(dir, "builds"), "utf8")).toBe("x");
	expect(existsSync(join(dir, "lock"))).toBe(false);
});

// Regression (the six-process test above failed once under a loaded `bun run check`, 2026-11-24): a process saw no lock,
// then another took it and wrote its marker, and the first read "not stale" and returned while the rest of the folder
// was still being written. Here `stale()` plays that other process on its first call.
test("an output that turns current while another process holds the lock is waited for, not returned", async () => {
	const dir = mkdtempSync(join(tmpdir(), "compile-lock-"));
	const { withCompileLock } = await import(LOCK);
	// A real timer on purpose: `withCompileLock` polls a real lock directory; the 200 ms only makes the wait observable.
	const lock = join(dir, "lock");
	let released = false;
	let calls = 0;
	const stale = () => {
		if (calls++ === 0) {
			mkdirSync(lock); // the other process took the lock and wrote the marker `stale()` reads...
			setTimeout(() => ((released = true), rmSync(lock, { recursive: true })), 200); // ...and finishes later
		}
		return false;
	};
	await withCompileLock(lock, stale, async () => {
		throw new Error("nothing is stale: no build");
	});
	expect(released).toBe(true);
});

test("a lock left behind by a killed process does not block forever", async () => {
	const dir = mkdtempSync(join(tmpdir(), "compile-lock-"));
	const { withCompileLock } = await import(LOCK);
	const { mkdirSync, utimesSync } = await import("node:fs");
	mkdirSync(join(dir, "lock"));
	const old = new Date(Date.now() - 10 * 60_000);
	utimesSync(join(dir, "lock"), old, old);
	let built = false;
	await withCompileLock(
		join(dir, "lock"),
		() => !built,
		async () => void (built = true),
	);
	expect(built).toBe(true);
});

test("nothing stale: no lock taken, no build", async () => {
	const dir = mkdtempSync(join(tmpdir(), "compile-lock-"));
	const { withCompileLock } = await import(LOCK);
	let built = 0;
	await withCompileLock(
		join(dir, "lock"),
		() => false,
		async () => void built++,
	);
	expect(built).toBe(0);
	expect(readdirSync(dir)).toEqual([]);
});
