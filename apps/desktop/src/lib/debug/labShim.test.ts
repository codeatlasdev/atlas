// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { allowed, sameOrigin } from "../../../scripts/lab-bridge.ts";
import { deliver, drainBody, invokeBody, listenBody } from "./labShim.ts";

// The bodies run in the agent instance with full IPC: browser-supplied strings must stay data.
const run = (body: string, internals: object) =>
	new Function("window", `return (async () => { ${body} })();`)({ __TAURI_INTERNALS__: internals });

describe("invokeBody", () => {
	it("passes command and args as data, even when they look like code", async () => {
		const calls: unknown[] = [];
		const evil = `x"); globalThis.pwned = 1; ("`;
		const out = await run(invokeBody(evil, { path: "`${1}`", n: 2 }), {
			invoke: async (c, a) => (calls.push([c, a]), "ok"),
		});
		expect(out).toBe("ok");
		expect(calls).toEqual([[evil, { path: "`${1}`", n: 2 }]]);
		expect(globalThis.pwned).toBeUndefined();
	});

	it("sends {} when the caller gave no args", async () => {
		let got;
		await run(invokeBody("c", undefined), { invoke: async (_c, a) => (got = a) });
		expect(got).toEqual({});
	});
});

describe("listen → drain → deliver", () => {
	it("routes instance events to the browser handler that asked for them, per client", async () => {
		const win = { __TAURI_INTERNALS__: null };
		let fire;
		win.__TAURI_INTERNALS__ = {
			transformCallback: (cb) => ((fire = cb), 99),
			invoke: async (cmd, a) => (expect(cmd).toBe("plugin:event|listen"), expect(a.handler).toBe(99), 7),
		};
		const exec = (b) => new Function("window", `return (async () => { ${b} })();`)(win);
		expect(await exec(listenBody("A", 5, { event: "bots://changed", target: { kind: "Any" } }))).toBe(7);
		fire({ event: "bots://changed", payload: 1 });
		fire({ event: "bots://changed", payload: 2 });
		expect(await exec(drainBody("B", 0))).toEqual([]); // another tab does not steal A's events
		const events = await exec(drainBody("A", 0));
		expect(await exec(drainBody("A", 0))).toEqual([]); // drained once

		const seen = [];
		const dropped = deliver(events, new Map([[5, (e) => seen.push(e.payload)]]));
		expect(seen).toEqual([1, 2]);
		expect(dropped).toBe(0);
		expect(deliver(events, new Map())).toBe(2); // unlistened handler: counted, not thrown
	});

	it("a drain waits for the first event instead of returning empty", async () => {
		const win = { __TAURI_INTERNALS__: {} };
		const exec = (b) => new Function("window", `return (async () => { ${b} })();`)(win);
		const pending = exec(drainBody("A", 2000)); // runs up to its first wait: queue created and empty
		win.__atlasLab.A.push({ h: 1, e: "late" });
		expect(await pending).toEqual([{ h: 1, e: "late" }]);
	});
});

describe("sameOrigin", () => {
	it("accepts only this dev server as the Origin", () => {
		expect(sameOrigin("http://localhost:1421", "localhost:1421")).toBe(true);
		for (const [o, h] of [
			["http://evil.test", "localhost:1421"],
			["http://localhost:1420", "localhost:1421"],
			["null", "localhost:1421"],
			[undefined, "localhost:1421"],
			["file:///x", "localhost:1421"],
			["http://localhost:1421", undefined],
		]) {
			expect(sameOrigin(o, h)).toBe(false);
		}
	});
});

describe("allowed", () => {
	const ok = {
		origin: "http://localhost:1421",
		host: "localhost:1421",
		"x-atlas-lab": "t0k",
		"content-type": "application/json",
	};
	it("lets only this page with the run's token and a JSON body run code in the instance", () => {
		expect(allowed(ok, "t0k")).toBe(true);
		expect(allowed({ ...ok, "x-atlas-lab": undefined }, "t0k")).toBe(false);
		expect(allowed({ ...ok, "x-atlas-lab": "guess" }, "t0k")).toBe(false);
		expect(allowed({ ...ok, origin: "http://evil.test" }, "t0k")).toBe(false);
		// a form post from another site needs no preflight: text/plain must not pass
		expect(allowed({ ...ok, "content-type": "text/plain" }, "t0k")).toBe(false);
		expect(allowed({ ...ok, "x-atlas-lab": ["t0k", "t0k"] }, "t0k")).toBe(false);
	});
});
