// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { EDGE_HINT_MS, expireHint, hintPending, markHintSeen } from "./edgeHint.ts";

const memory = () => {
	const m = new Map();
	return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v) };
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Regression: 1.5.0-alpha.26 first-run pass, "Workspaces Ctrl+Shift+B" hung over the sidebar on every screen because
// it only went away once the rail was opened. It now expires on its own and stays gone.
test("the hint expires on its own, is remembered, and does not come back", async () => {
	const store = memory();
	let shown = true;
	expect(hintPending(store, "k")).toBe(true);
	expireHint(store, "k", () => (shown = false), 10);
	await sleep(40);
	expect(shown).toBe(false);
	expect(hintPending(store, "k")).toBe(false);
});

test("an unmounted hint does not fire, nor mark itself seen", async () => {
	const store = memory();
	let shown = true;
	const cancel = expireHint(store, "k", () => (shown = false), 10);
	cancel();
	await sleep(40);
	expect(shown).toBe(true);
	expect(hintPending(store, "k")).toBe(true);
});

test("storage that throws never breaks the hint", () => {
	const boom = {
		getItem: () => {
			throw new Error("blocked");
		},
		setItem: () => {
			throw new Error("full");
		},
	};
	expect(hintPending(boom, "k")).toBe(true);
	expect(() => markHintSeen(boom, "k")).not.toThrow();
	expect(EDGE_HINT_MS).toBeLessThanOrEqual(10_000);
});
