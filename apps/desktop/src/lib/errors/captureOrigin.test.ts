// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeEach, expect, mock, test } from "bun:test";

mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../test/uiKit.ts")).installUiKit();

const saved = Object.getOwnPropertyDescriptor(globalThis, "$state");
Object.defineProperty(globalThis, "$state", { value: (v) => v, configurable: true, writable: true });
afterAll(() => {
	if (saved) Object.defineProperty(globalThis, "$state", saved);
	else delete globalThis.$state;
});

const { errors, startErrorHandler } = await import("./errors.svelte.ts");
const { headline } = await import("./report.ts");

beforeEach(() => {
	errors.toasts = [];
});

// Regression: 1.5.0-alpha.26, an uncaught timer error and a rejected promise both showed "An action didn't finish".
test("window error and unhandled rejection reach the toast with different titles", async () => {
	const stop = startErrorHandler();
	try {
		window.dispatchEvent(new ErrorEvent("error", { error: new Error("e2e: uncaught error from a timer") }));
		const rejection = new Event("unhandledrejection");
		rejection.reason = new Error("e2e: rejected promise");
		window.dispatchEvent(rejection);
		await new Promise((r) => setTimeout(r, 20));
	} finally {
		stop();
	}
	const byMessage = Object.fromEntries(
		errors.toasts.map((t) => [t.report.message, headline(t.report).title]),
	);
	expect(byMessage["e2e: uncaught error from a timer"]).toBe("Unexpected error");
	expect(byMessage["e2e: rejected promise"]).toBe("A background task failed");
});
