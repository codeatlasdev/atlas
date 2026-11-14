// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../../__fixtures__/crew_browsers_status.json" with { type: "json" };
import { canInstall, errorView, needsNode, nodeProblem, parseProblem, viewOf } from "./browsers.ts";

// `crew_browsers_status.json` is what the Rust command serializes for the REAL output of `atlas crew browsers status`
// (a host with Node 18.19.1 and nothing installed; one that is ready), so a rename on either side fails here.

describe("viewOf the real status", () => {
	test("a host with Node 18 and nothing installed lists every missing piece, Node first", () => {
		const v = viewOf(fixture.missing);
		expect(v.phase).toBe("missing");
		expect(v.problems.map((p) => p.kind)).toEqual(["node", "runner", "playwright", "browsers", "browsers"]);
		expect(v.needsNode).toBe(true);
	});

	test("the CLI's own 'run atlas crew browsers install' tail is dropped: the Install button is that instruction", () => {
		const v = viewOf(fixture.missing);
		expect(v.problems.map((p) => p.text)).toEqual([
			"node: Node 18.19.1 found, need >= 20 (install a newer Node.js, for example from nodesource)",
			"runner: missing",
			"playwright: package missing",
			"browsers: chromium missing",
			"browsers: webkit missing",
		]);
	});

	test("the Node requirement is worded from the version the server reported", () => {
		const v = viewOf(fixture.missing);
		expect(nodeProblem(v.problems)).toBe(
			"Node 18.19.1 found, need >= 20 (install a newer Node.js, for example from nodesource)",
		);
	});

	test("a ready host is ready, and has nothing to install", () => {
		const v = viewOf(fixture.ready);
		expect(v).toEqual({ phase: "ready" });
		expect(canInstall(v)).toBe(false);
	});
});

describe("canInstall", () => {
	test("Install is offered when only the browsers are missing, not while Node is missing or too old", () => {
		const onlyBrowsers = viewOf({
			ready: false,
			problems: ["runner: missing, run atlas crew browsers install"],
		});
		expect(onlyBrowsers.needsNode).toBe(false);
		expect(canInstall(onlyBrowsers)).toBe(true);
		expect(canInstall(viewOf(fixture.missing))).toBe(false);
	});

	test("no Install while checking or after an error", () => {
		expect(canInstall({ phase: "checking" })).toBe(false);
		expect(canInstall(errorView("cannot reach ubuntu@srv"))).toBe(false);
	});
});

describe("parseProblem", () => {
	test("kinds follow the CLI's prefixes; anything else is `other`", () => {
		expect(parseProblem("npm: missing (it ships with Node.js)").kind).toBe("npm");
		expect(
			parseProblem("playwright: 1.40.0 installed, atlas wants 1.65.1: run atlas crew browsers install"),
		).toEqual({ kind: "playwright", text: "playwright: 1.40.0 installed, atlas wants 1.65.1" });
		expect(parseProblem("deps: webkit does not start (libx): run atlas crew browsers install").kind).toBe(
			"deps",
		);
		expect(parseProblem("something new").kind).toBe("other");
	});

	test("a missing npm also needs Node.js first", () => {
		expect(needsNode([parseProblem("npm: missing (it ships with Node.js from nodesource)")])).toBe(true);
		expect(needsNode([parseProblem("runner: missing")])).toBe(false);
	});
});
