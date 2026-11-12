// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { emptyPanel, lastLines, namesLabel, portsLabel, serviceRows, stackKind } from "./overview.ts";

const def = (name, extra = {}) => ({ name, enabled: true, port: null, depends_on: [], ...extra });

describe("serviceRows", () => {
	test("stopped stack lists declared services, disabled ones marked, most urgent first", () => {
		const rows = serviceRows([def("b"), def("a", { enabled: false })], undefined, false);
		expect(rows.map((r) => [r.name, r.state])).toEqual([
			["a", "disabled"],
			["b", "stopped"],
		]);
		expect(rows[0].def.name).toBe("a");
	});
	test("running stack uses the live rows and attaches their defs", () => {
		const live = [
			{ name: "api", state: "running" },
			{ name: "web", state: "failed" },
		];
		const rows = serviceRows([def("api")], live, true);
		expect(rows.map((r) => r.name)).toEqual(["web", "api"]);
		expect(rows[0].def).toBeUndefined();
		expect(rows[1].def.name).toBe("api");
	});
	test("a running stack with no live rows yet falls back to the declared ones", () => {
		expect(serviceRows([def("a")], [], true)[0].state).toBe("stopped");
	});
});

describe("labels", () => {
	test("ports: enabled only, first five, remainder counted", () => {
		const defs = [1, 2, 3, 4, 5, 6, 7].map((p) => def(`s${p}`, { port: 3000 + p }));
		defs.push(def("off", { port: 9, enabled: false }), def("none"));
		expect(portsLabel(defs)).toBe(":3001 :3002 :3003 :3004 :3005 +2");
		expect(portsLabel([def("x")])).toBe("");
	});
	test("names: first four then a count", () => {
		expect(namesLabel(["a", "b"])).toBe("a, b");
		expect(namesLabel(["a", "b", "c", "d", "e", "f"])).toBe("a, b, c, d, +2");
	});
});

describe("lastLines", () => {
	test("newest non-marker line per wanted service; stops once all are found", () => {
		const items = [
			{ name: "api", line: "old" },
			{ name: "web", line: "w" },
			{ name: "api", line: "new" },
			{ name: "api", line: "── restart", mark: "restart" },
			{ name: "other", line: "x" },
		];
		expect(lastLines(items, ["api", "web"])).toEqual({ api: "new", web: "w" });
		expect(lastLines(items, ["missing"])).toEqual({});
	});
});

// Incident (first run on Windows): every folder with zero services said "Atlas found nothing to run here. Declare
// services in atlas.yaml", also while the config was still loading and next to a broken atlas.yaml banner.
describe("emptyPanel: what the panel says when there are no service rows", () => {
	const project = (over = {}) => ({ config_source: "detected", error: null, ...over });
	const cfg = (over = {}) => ({ services: [], detected: null, ...over });

	test("services present: never the empty state", () => {
		expect(emptyPanel(project(), cfg({ services: [def("a")] }))).toEqual({ kind: "services" });
	});
	test("config not loaded yet: says nothing (no 'found nothing' flash)", () => {
		expect(emptyPanel(project(), undefined)).toEqual({ kind: "loading" });
	});
	test("a broken atlas.yaml defers to the banner, loaded or not", () => {
		const p = project({ config_source: "file", error: "services.api: missing command" });
		expect(emptyPanel(p, undefined)).toEqual({ kind: "broken" });
		expect(emptyPanel(p, cfg())).toEqual({ kind: "broken" });
	});
	test("a detected stack with no service names the stack and offers to set services up", () => {
		expect(emptyPanel(project(), cfg({ detected: "Rust project (Cargo.toml)" }))).toEqual({
			kind: "detected",
			detected: "Rust project (Cargo.toml)",
		});
	});
	test("nothing recognised and no file: its own state", () => {
		expect(emptyPanel(project(), cfg())).toEqual({ kind: "nothing" });
	});
	test("an atlas.yaml with no active service is an empty file, not an undetected folder", () => {
		expect(emptyPanel(project({ config_source: "file" }), cfg())).toEqual({ kind: "empty-file" });
	});
	// The store swallowed a failed `project_config`, so the panel stayed on a blank "loading" for good.
	test("a config that failed to load is the 'nothing' state with its button, not a blank forever", () => {
		expect(emptyPanel(project(), undefined, true)).toEqual({ kind: "nothing" });
		expect(emptyPanel(project(), undefined, false)).toEqual({ kind: "loading" });
		// A broken atlas.yaml still defers to its banner.
		expect(emptyPanel(project({ error: "x" }), undefined, true)).toEqual({ kind: "broken" });
	});
	test("the headline names the stack without its manifest detail", () => {
		expect(stackKind("Rust project (Cargo.toml, axum)")).toBe("Rust project");
		expect(stackKind("Go project")).toBe("Go project");
	});
});
