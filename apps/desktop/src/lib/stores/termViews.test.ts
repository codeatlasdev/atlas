// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { hasTermView, registerTermView, reloadTermView } from "./termViews.ts";

test("reload reaches only the tab asked for and reports tabs without a native view", async () => {
	const calls: string[] = [];
	const a = registerTermView("a", { reload: async () => void calls.push("a") });
	const b = registerTermView("b", { reload: async () => void calls.push("b") });
	expect(await reloadTermView("b")).toBe(true);
	expect(calls).toEqual(["b"]);
	expect(await reloadTermView("xterm-tab")).toBe(false);
	expect(await reloadTermView(null)).toBe(false);
	a();
	b();
	expect(hasTermView("a")).toBe(false);
});

test("a stale unmount does not unregister the view that replaced it", async () => {
	const calls: string[] = [];
	const old = registerTermView("t", { reload: async () => void calls.push("old") });
	registerTermView("t", { reload: async () => void calls.push("new") })();
	old();
	// the new one unregistered itself; the old cleanup must not have resurrected or removed anything
	expect(hasTermView("t")).toBe(false);
	const first = registerTermView("t2", { reload: async () => void calls.push("first") });
	const second = registerTermView("t2", { reload: async () => void calls.push("second") });
	first();
	expect(await reloadTermView("t2")).toBe(true);
	expect(calls).toEqual(["second"]);
	second();
});

test("a failing reload rejects so the caller can toast it", async () => {
	const off = registerTermView("f", {
		reload: async () => {
			throw new Error("no terminal for tab f");
		},
	});
	await expect(reloadTermView("f")).rejects.toThrow("no terminal for tab f");
	off();
});
