// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { createHostEvents } from "./hostEvents.ts";

// Incident (2026-10-26, Linux/Arch: "one terminal tab lags, two freeze the app"): every terminal and every chat tab
// registered its own `listen("host://event")` and `listen("host://reconnected")`. Each `listen` is an IPC round trip
// and a JS callback run for EVERY hostd event (`pty_activity`, `pty_title`, `fs_event`, `lsp_*`), whichever pty it was
// about. One listener per event name, routed by pty, makes the cost independent of how many terminals are mounted.

/** A fake Tauri `listen`: counts registrations and lets a test emit an event to whoever is registered. */
function fakeTauri() {
	const handlers = new Map<string, Set<(e: { payload: unknown }) => void>>();
	const t = {
		listens: 0,
		unlistens: 0,
		listen: async (name: string, cb: (e: { payload: unknown }) => void) => {
			t.listens++;
			(handlers.get(name) ?? handlers.set(name, new Set()).get(name)!).add(cb);
			return () => {
				t.unlistens++;
				handlers.get(name)!.delete(cb);
			};
		},
		emit: (name: string, payload: unknown = null) => handlers.get(name)?.forEach((cb) => cb({ payload })),
		registered: (name: string) => handlers.get(name)?.size ?? 0,
	};
	return t;
}
/** Let the fake `listen` promises settle: a few microtask turns, no clock. */
const tick = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe("hostEvents", () => {
	test("any number of terminals share one listener per event name", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		for (let i = 0; i < 20; i++) {
			ev.onDropped(`p${i}`, () => {});
			ev.onReconnected(() => {});
		}
		await tick();
		expect(t.listens).toBe(2);
		expect(t.registered("host://event")).toBe(1);
		expect(t.registered("host://reconnected")).toBe(1);
	});

	test("pty_dropped reaches only the terminal it is about", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		const hits: string[] = [];
		ev.onDropped("a", () => hits.push("a"));
		ev.onDropped("b", () => hits.push("b"));
		await tick();
		t.emit("host://event", { ev: "pty_dropped", pty: "b", reason: "slow_client" });
		expect(hits).toEqual(["b"]);
	});

	test("events that are not pty_dropped (activity, title, fs, lsp, junk) wake nobody", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		let n = 0;
		ev.onDropped("a", () => n++);
		await tick();
		for (const p of [
			{ ev: "pty_activity", pty: "a" },
			{ ev: "pty_title", pty: "a", title: "x" },
			{ ev: "fs_event", id: "a" },
			{ ev: "lsp_status", servers: [] },
			null,
			"str",
			42,
		])
			t.emit("host://event", p);
		expect(n).toBe(0);
	});

	test("a reconnect reaches every subscriber", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		let n = 0;
		for (let i = 0; i < 5; i++) ev.onReconnected(() => n++);
		await tick();
		t.emit("host://reconnected", { host: "local" });
		expect(n).toBe(5);
	});

	test("unsubscribing one terminal leaves the others and keeps the listener; the last one releases it", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		const hits: string[] = [];
		const offA = ev.onDropped("a", () => hits.push("a"));
		const offB = ev.onDropped("b", () => hits.push("b"));
		await tick();
		offA();
		t.emit("host://event", { ev: "pty_dropped", pty: "a" });
		t.emit("host://event", { ev: "pty_dropped", pty: "b" });
		expect(hits).toEqual(["b"]);
		expect(t.unlistens).toBe(0);
		offB();
		await tick();
		expect(t.unlistens).toBe(1);
		expect(t.registered("host://event")).toBe(0);
	});

	test("two subscribers on the same pty both hear it, and each leaves on its own", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		let n = 0;
		const off1 = ev.onDropped("a", () => n++);
		ev.onDropped("a", () => n++);
		await tick();
		t.emit("host://event", { ev: "pty_dropped", pty: "a" });
		expect(n).toBe(2);
		off1();
		t.emit("host://event", { ev: "pty_dropped", pty: "a" });
		expect(n).toBe(3);
	});

	test("subscribing again after everyone left registers a fresh listener (a remount cycle)", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		ev.onDropped("a", () => {})();
		await tick();
		expect(t.registered("host://event")).toBe(0);
		let n = 0;
		ev.onDropped("a", () => n++);
		await tick();
		t.emit("host://event", { ev: "pty_dropped", pty: "a" });
		expect(n).toBe(1);
		expect(t.listens).toBe(2);
	});

	// A terminal that mounts and unmounts before Tauri answers `listen` (a fast tab switch) must not leak the
	// listener the answer brings.
	test("leaving before the listener is registered does not leak it", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		const off = ev.onDropped("a", () => {});
		off();
		await tick();
		await tick();
		expect(t.registered("host://event")).toBe(0);
	});

	test("a subscriber that throws does not stop the others", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		let n = 0;
		ev.onReconnected(() => {
			throw new Error("boom");
		});
		ev.onReconnected(() => n++);
		await tick();
		t.emit("host://reconnected", {});
		expect(n).toBe(1);
	});

	test("unsubscribing twice is harmless", async () => {
		const t = fakeTauri();
		const ev = createHostEvents(t.listen);
		const off = ev.onDropped("a", () => {});
		ev.onDropped("b", () => {});
		await tick();
		off();
		off();
		expect(t.registered("host://event")).toBe(1);
	});
});
