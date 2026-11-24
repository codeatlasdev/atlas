/**
 * One process rebuilds a shared generated folder; the others wait for it.
 *
 * `bun test --parallel` runs the preload of every test file in its own process, and the preload recompiles
 * `src/lib/paraglide` when the catalogs changed. The compiler clears the folder before it writes it, so a second process
 * compiling (or just importing) at that moment found `#lib/paraglide/messages/_index.js` missing: the first run after any
 * catalog edit failed with "Cannot find module", the next one passed.
 */
import { existsSync, mkdirSync, rmSync, statSync } from "node:fs";

/** A lock older than this was left by a process that died mid-build (a compile takes seconds). */
const ABANDONED_MS = 120_000;

/**
 * Run `build` when `stale()` says the output is out of date, in at most one process at a time. Returns once the output
 * is current: a process that finds the lock taken waits for its owner (even if the output looks current), then checks `stale()` again (usually false, so
 * the build is not repeated). `lock` is a directory (`mkdir` is atomic).
 */
export async function withCompileLock(
	lock: string,
	stale: () => boolean,
	build: () => Promise<void>,
): Promise<void> {
	for (;;) {
		// A build in progress is waited for even when the output already looks current: the compiler writes its marker
		// files (what `stale()` reads) before the rest of the folder. `stale()` comes first: if the output looks current,
		// its marker was written under the lock, and the lock goes only after the whole folder, so no lock after that
		// means done. The other order let a process see no lock, then a new owner write the marker, then read "current"
		// and return mid-build.
		if (!stale() && !existsSync(lock)) return;
		try {
			mkdirSync(lock);
		} catch (e) {
			if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
			try {
				if (Date.now() - statSync(lock).mtimeMs > ABANDONED_MS)
					rmSync(lock, { recursive: true, force: true });
			} catch {}
			await new Promise((r) => setTimeout(r, 50));
			continue;
		}
		try {
			// Re-checked under the lock: the previous owner may have just finished it.
			if (stale()) await build();
		} finally {
			rmSync(lock, { recursive: true, force: true });
		}
		return;
	}
}
