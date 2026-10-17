// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { candidates, cleanupPrefs, RESTORE_MS, restorable } from "./autoCleanup.ts";

const DAY = 86_400_000;
const now = 100 * DAY;
const wt = (id, over = {}) => ({ id, isMain: false, pinned: false, lastActivityAt: now, ...over });

describe("auto clean up (#58)", () => {
	test("is off unless settings.json says merged or idle; days are clamped to 1..365, default 14", () => {
		expect(cleanupPrefs({})).toEqual({ mode: "off", days: 14 });
		expect(cleanupPrefs({ autoCleanup: "yes" }).mode).toBe("off");
		expect(cleanupPrefs({ autoCleanup: "idle", autoCleanupDays: "0" }).days).toBe(14);
		expect(cleanupPrefs({ autoCleanup: "idle", autoCleanupDays: "9999" }).days).toBe(365);
		expect(cleanupPrefs({ autoCleanup: "merged", autoCleanupDays: 3 })).toEqual({ mode: "merged", days: 3 });
	});

	test("never picks the main checkout, a pinned worktree or the one on screen", () => {
		const list = [wt("main", { isMain: true }), wt("pin", { pinned: true }), wt("sel"), wt("ok")];
		expect(candidates(list, { mode: "merged", days: 14 }, "sel", now).map((w) => w.id)).toEqual(["ok"]);
		expect(candidates(list, { mode: "off", days: 14 }, null, now)).toEqual([]);
	});

	test("idle mode picks a worktree exactly at N days without activity, not a minute before", () => {
		const list = [
			wt("at", { lastActivityAt: now - 14 * DAY }),
			wt("before", { lastActivityAt: now - 14 * DAY + 60_000 }),
		];
		expect(candidates(list, { mode: "idle", days: 14 }, null, now).map((w) => w.id)).toEqual(["at"]);
	});

	// A shell or dev server with no agent leaves lastActivityAt old, and a merged PR does not mean the user stopped
	// using the worktree: a running stack is never stopped by auto clean up, in either mode.
	for (const mode of ["idle", "merged"])
		test(`${mode} mode skips a worktree whose stack is running`, () => {
			const list = [wt("up", { lastActivityAt: 0 }), wt("down", { lastActivityAt: 0 })];
			const busy = new Set(["up"]);
			expect(candidates(list, { mode, days: 14 }, null, now, busy).map((w) => w.id)).toEqual(["down"]);
		});

	test("the restore list drops entries older than 7 days and malformed ones", () => {
		const ok = { repoId: "r", name: "n", branch: "b", at: now - RESTORE_MS + 1 };
		const old = { ...ok, at: now - RESTORE_MS };
		expect(restorable([ok, old, { name: "x" }, null], now)).toEqual([ok]);
		expect(restorable("nope", now)).toEqual([]);
	});
});
