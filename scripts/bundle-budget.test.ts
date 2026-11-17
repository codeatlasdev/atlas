import { describe, expect, test } from "bun:test";
import {
	analyze,
	evaluate,
	matches,
	mainRouteNodes,
	packageOf,
	ratchet,
	type Budget,
	type Chunk,
} from "./bundle-budget.ts";

const chunk = (name: string, p: Partial<Chunk> = {}): Chunk => ({
	file: `_app/${name.replace("/", "-")}.js`,
	name,
	imports: [],
	modules: [],
	css: [],
	size: 1000,
	gzip: 400,
	...p,
});
const m = (path: string) => `/repo/apps/desktop/node_modules/${path}`;

// Shape of what the real build gives: entries, a shared chunk, one lazy chunk reached by `import()` only.
const shared = chunk("shared", {
	gzip: 100,
	modules: [m("svelte/src/index.js"), "/repo/apps/desktop/src/lib/a.ts"],
});
const lazy = chunk("terminal", {
	gzip: 5000,
	modules: [m("@xterm/xterm/lib/xterm.js")],
});
const start = chunk("entry/start", { imports: [shared.file] });
const node2 = chunk("nodes/2", {
	imports: [shared.file],
	css: ["_app/2.css"],
	modules: [m("@codemirror/state/dist/index.js")],
});
const all = [start, node2, shared, lazy];
const assets = [
	{ file: "_app/2.css", gzip: 70 },
	{ file: "_app/other.css", gzip: 9999 },
];

const budget: Budget = {
	jsGzip: 1000,
	cssGzip: 100,
	slack: 50,
	forbidden: ["@codemirror/*", "@xterm/*", "yaml"],
	known: [],
};

describe("analyze", () => {
	test("follows static imports from the roots and not dynamic ones", () => {
		const r = analyze(all, assets, ["entry/start", "nodes/2"]);
		expect(r.chunks.map((c) => c.name).sort()).toEqual([
			"entry/start",
			"nodes/2",
			"shared",
		]);
		expect(r.jsGzip).toBe(400 + 400 + 100); // the shared chunk once, the lazy terminal chunk not at all
		expect(r.packages["@xterm/xterm"]).toBeUndefined();
		expect(r.packages["@codemirror/state"]).toEqual(["nodes/2"]);
	});
	test("counts the stylesheets of the chunks inside, not every stylesheet of the build", () => {
		expect(analyze(all, assets, ["nodes/2"]).cssGzip).toBe(70);
	});
	test("a root that is not in the build is an error, not an empty report", () => {
		expect(() => analyze(all, assets, ["nodes/9"])).toThrow(
			/no chunk named "nodes\/9"/,
		);
	});
});

describe("packages", () => {
	test("packageOf names the package, scoped or not, under the last node_modules", () => {
		expect(packageOf(m("@codemirror/state/dist/index.js"))).toBe(
			"@codemirror/state",
		);
		expect(packageOf(m("yaml/browser/dist/index.js"))).toBe("yaml");
		expect(
			packageOf(
				"/r/node_modules/.bun/x@1/node_modules/marked/lib/marked.esm.js",
			),
		).toBe("marked");
		expect(packageOf(m("@xterm/xterm/css/xterm.css?inline"))).toBe(
			"@xterm/xterm",
		);
		expect(packageOf("/repo/apps/desktop/src/lib/a.ts")).toBeNull();
	});
	test("a scope pattern matches its packages and not a look-alike", () => {
		expect(matches("@codemirror/*", "@codemirror/view")).toBe(true);
		expect(matches("@codemirror/*", "@codemirror-x/view")).toBe(false);
		expect(matches("yaml", "yaml")).toBe(true);
		expect(matches("yaml", "yaml-ast")).toBe(false);
	});
});

describe("evaluate", () => {
	const report = analyze(all, assets, ["entry/start", "nodes/2"]);

	test("a forbidden package reachable from startup fails and names the chunk", () => {
		const p = evaluate(report, { ...budget, jsGzip: 900 });
		expect(p).toHaveLength(1);
		expect(p[0]).toMatch(
			/@codemirror\/state is reachable from startup \(in nodes\/2\)/,
		);
	});
	test("a known violation passes; once it is gone from startup the entry must go too", () => {
		const known = [{ package: "@codemirror/*", why: "crew chat" }];
		expect(evaluate(report, { ...budget, jsGzip: 900, known })).toEqual([]);
		const clean = analyze(
			[start, chunk("nodes/2", { imports: [shared.file] }), shared],
			assets,
			["entry/start", "nodes/2"],
		);
		expect(
			evaluate(clean, { ...budget, jsGzip: 900, cssGzip: 0, known }),
		).toEqual([
			expect.stringMatching(/known violation "@codemirror\/\*" is gone/),
		]);
	});
	test("going over the JS or CSS budget fails", () => {
		const known = [{ package: "@codemirror/*", why: "x" }];
		expect(evaluate(report, { ...budget, jsGzip: 899, known })[0]).toMatch(
			/startup JS is .* over the budget/,
		);
		expect(
			evaluate(report, { ...budget, jsGzip: 900, cssGzip: 69, known })[0],
		).toMatch(/startup CSS is .* over the budget/);
	});
	test("a budget well above the measured size asks to be lowered, a little above does not", () => {
		const known = [{ package: "@codemirror/*", why: "x" }];
		expect(
			evaluate(report, {
				...budget,
				jsGzip: 900 + 50,
				cssGzip: 70 + 50,
				known,
			}),
		).toEqual([]);
		expect(
			evaluate(report, { ...budget, jsGzip: 900 + 51, cssGzip: 70, known }),
		).toEqual([expect.stringMatching(/jsGzip is .* lower it/)]);
	});
});

describe("ratchet", () => {
	const report = analyze(all, assets, ["entry/start", "nodes/2"]);
	test("lowers the sizes to the measure (to a KiB) and drops known entries that are gone", () => {
		const next = ratchet(report, {
			...budget,
			jsGzip: 10 * 1024,
			cssGzip: 5 * 1024,
			known: [
				{ package: "@codemirror/*", why: "x" },
				{ package: "marked", why: "y" },
			],
		});
		expect(next.jsGzip).toBe(1024);
		expect(next.cssGzip).toBe(1024);
		expect(next.known.map((k) => k.package)).toEqual(["@codemirror/*"]);
	});
	test("never raises a budget", () => {
		expect(
			ratchet(report, { ...budget, jsGzip: 500, cssGzip: 10 }),
		).toMatchObject({ jsGzip: 500, cssGzip: 10 });
	});
});

describe("mainRouteNodes", () => {
	// From SvelteKit's generated client manifest (apps/desktop/.svelte-kit/generated/build/client-optimized/app.js).
	const appJs = `export const dictionary = {
		"/(app)": [6,[2]],
		"/crew-demo": [8],
		"/dock": [9,[3]],
	};`;
	test("root layout, the (app) layouts, then its page", () => {
		expect(mainRouteNodes(appJs)).toEqual(["nodes/0", "nodes/2", "nodes/6"]);
		expect(mainRouteNodes(`"/(app)": [6,[2, 4]]`)).toEqual([
			"nodes/0",
			"nodes/2",
			"nodes/4",
			"nodes/6",
		]);
	});
	test("a manifest without the route fails loudly", () => {
		expect(() => mainRouteNodes(`"/dock": [9,[3]]`)).toThrow(
			/no "\/\(app\)" route/,
		);
	});
});
