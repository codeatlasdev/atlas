// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, mock, test } from "bun:test";

const toasts: string[] = [];
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@codeatlasdev/ui", () => ({ toast: { info: (m: string) => toasts.push(m) } }));
globalThis.localStorage = { getItem: () => null, setItem() {} };
globalThis.$state = (v) => v;

const { gpu, applyHostStatus } = await import("./gpu.svelte.ts");

test("an outdated host that cannot be replaced turns on the classic-renderer notice", () => {
	applyHostStatus({ status: "outdated", detail: "x" });
	expect(gpu.hostTooOld).toBe(true);
	expect(toasts).toEqual([]);
});

test("a live upgrade clears the notice and toasts once, however often the status is re-read", () => {
	applyHostStatus({ status: "upgraded", detail: { handed_off: 2 } });
	applyHostStatus({ status: "upgraded", detail: { handed_off: 2 } });
	expect(gpu.hostTooOld).toBe(false);
	expect(toasts).toEqual(["Terminal host updated"]);
});

test("no status yet changes nothing", () => {
	gpu.hostTooOld = true;
	applyHostStatus(null);
	expect(gpu.hostTooOld).toBe(true);
	applyHostStatus({ status: "current", detail: {} });
	expect(gpu.hostTooOld).toBe(false);
});
