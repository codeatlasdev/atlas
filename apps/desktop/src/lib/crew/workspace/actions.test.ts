// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, expect, mock, test } from "bun:test";

mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../../test/uiKit.ts")).installUiKit();

const saved = Object.getOwnPropertyDescriptor(globalThis, "$state");
Object.defineProperty(globalThis, "$state", { value: (v) => v, configurable: true, writable: true });
afterAll(() => {
	if (saved) Object.defineProperty(globalThis, "$state", saved);
	else delete globalThis.$state;
});

const { ask } = await import("#lib/stores/ask.svelte.ts");
const { crew } = await import("#lib/stores/crew.svelte.ts");
const { handleContextAction } = await import("./actions.svelte.ts");

// Regression: 1.5.0-alpha.27 (#671). The delete question was `confirm()`, which the dialog plugin rewires to a
// command the capability refuses; the refusal made `confirm()` return a truthy Promise, so the crewmate was deleted
// without anyone being asked. The question is the app's own dialog now, and "no" must delete nothing.
test("deleting a crewmate from the list asks in the app first and deletes nothing on no", async () => {
	const deleted = [];
	crew.deleteCrewmate = async (host, id) => void deleted.push([host, id]);
	const pending = handleContextAction("h1", "c1", "delete");
	await Promise.resolve();
	expect(ask.current, "the question is open and nothing is deleted yet").not.toBeNull();
	expect(ask.current.destructive).toBe(true);
	expect(deleted).toEqual([]);
	ask.current.resolve(false);
	await pending;
	expect(deleted).toEqual([]);

	const again = handleContextAction("h1", "c1", "delete");
	await Promise.resolve();
	ask.current.resolve(true);
	await again;
	expect(deleted).toEqual([["h1", "c1"]]);
});
