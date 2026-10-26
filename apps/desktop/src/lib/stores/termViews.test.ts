// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { createNativeSurface } from "#lib/native/surface.ts";
import { hasTermView, reloadTermView } from "./termViews.ts";

// A terminal's NativeView is a surface of kind "terminal"; these tests use bare ones.
const open: { dispose(): Promise<void> }[] = [];
function term(tab: string, reload: () => Promise<void>) {
	const el = document.createElement("div");
	el.getBoundingClientRect = () => new DOMRect(0, 0, 100, 100);
	document.body.append(el);
	const s = createNativeSurface({
		kind: "terminal",
		id: tab,
		slot: () => el,
		visible: () => true,
		open: async () => {},
		place: async () => {},
		release: async () => {},
		reload,
	});
	open.push(s);
	return s;
}
afterEach(async () => {
	await Promise.all(open.splice(0).map((s) => s.dispose()));
});

test("reload reaches only the tab asked for and reports tabs without a native view", async () => {
	const calls: string[] = [];
	const a = term("a", async () => void calls.push("a"));
	const b = term("b", async () => void calls.push("b"));
	await a.sync();
	await b.sync();
	expect(await reloadTermView("b")).toBe(true);
	expect(calls).toEqual(["b"]);
	expect(await reloadTermView("xterm-tab")).toBe(false);
	expect(await reloadTermView(null)).toBe(false);
	await a.dispose();
	await b.dispose();
	expect(hasTermView("a")).toBe(false);
});

test("a stale unmount does not unregister the view that replaced it", async () => {
	const calls: string[] = [];
	const old = term("t2", async () => void calls.push("first"));
	const next = term("t2", async () => void calls.push("second"));
	await next.sync();
	await old.dispose(); // the old layer is torn down after the new one mounted
	expect(hasTermView("t2")).toBe(true);
	expect(await reloadTermView("t2")).toBe(true);
	expect(calls).toEqual(["second"]);
});

test("reload before the view is open is a no-op, and a failing reload rejects so the caller can toast it", async () => {
	const calls: string[] = [];
	const t = term("early", async () => void calls.push("x"));
	expect(await reloadTermView("early")).toBe(true);
	expect(calls).toEqual([]); // not open yet
	const f = term("f", async () => {
		throw new Error("no terminal for tab f");
	});
	await f.sync();
	await expect(reloadTermView("f")).rejects.toThrow("no terminal for tab f");
	await t.dispose();
});
