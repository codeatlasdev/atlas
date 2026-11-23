// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import views from "../__fixtures__/project-views.json";

// Incident (#289, docs/perf/linux-webkit-round2.md): a running stack costs 2.5-3 % of a core in the web process on an
// otherwise still app, and the JSC sampler put 159 of 177 samples under Svelte `flush` and zrender `update`: the health
// panel redraws its charts (a line chart and a pair of sparklines per service) for every metrics sample, every 3 s,
// whether or not anyone can see them (the panel's tab behind another tab, or the window behind another window).
// Maintainer decision: the charts redraw only while the panel is visible and the window focused, and keep the 3 s
// cadence while it is. The real `Panel`, `ServiceGrid`, `ServiceCard`, `CardSpark`, `Charts` and metrics store; only the
// kit's canvas charts are a probe that records every draw they are asked for (the kit draws when its data prop changes).
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
const { default: NamedStub } = await import("../../../test/NamedStub.svelte");
const { default: ChartProbe } = await import("../../../test/ChartProbe.svelte");
const stub = (name: string) => ({ default: (anchor, props) => NamedStub(anchor, { ...props, name }) });
// A Proxy, not a spread: the parent's props are getters over its state, and a copy would freeze them at the first draw.
const probe = (kind: string) => (anchor, props) =>
	ChartProbe(anchor, new Proxy(props, { get: (p, k) => (k === "kind" ? kind : p[k]) }));
mock.module("@codeatlasdev/ui/charts", () => ({
	LineChart: probe("line"),
	BarChart: probe("bar"),
	Sparkline: probe("spark"),
}));
for (const f of ["PanelHeader", "PanelKpis", "QuickLinks", "StartInvite"])
	mock.module(`./${f}.svelte`, () => stub(f));
await (await import("../../../test/uiKit.ts")).useRealKit("Button", "Spinner", "EmptyState");
const { flushSync, mount, unmount } = await import("svelte");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { metrics } = await import("./metrics.svelte.ts");
const { default: PanelHost } = await import("../../../test/PanelHost.svelte");

let done: (() => void)[] = [];
afterEach(() => (done.forEach((f) => f()), (done = [])));

const NAMES = ["api", "web"];
const row = (name: string, cpu: number) => ({
	name,
	state: "running",
	exit_code: null,
	restarts: 0,
	note: null,
	cpu,
	mem_mb: 10,
});

let clock = 1_700_000_000_000;
/** The runner's 3 s tick: new rows land in the store, the metrics sampler takes a sample, the page flushes. */
function tick(cpu: number) {
	clock += 3000;
	store.live.shop.services = NAMES.map((n) => row(n, cpu));
	metrics.sample(clock);
	flushSync();
}

/** A running two-service project on screen: its panel open, its window focused. */
function open() {
	const project = {
		...views.detected,
		id: "shop",
		name: "shop",
		root: "/projects/shop",
		running: true,
		services_total: NAMES.length,
	};
	store.projects = [project];
	store.configs.shop = { services: NAMES.map((name) => ({ name, enabled: true, port: null, depends_on: [] })) };
	store.live.shop = { services: NAMES.map((n) => row(n, 1)), mode: "up", tunnel: null, events: [], operation: null };
	globalThis.__draws = [];
	window.dispatchEvent(new Event("focus"));
	metrics.sample(clock);
	const target = document.createElement("div");
	document.body.append(target);
	const host = flushSync(() => mount(PanelHost, { target, props: { project } }));
	done.push(() => (unmount(host), target.remove(), delete globalThis.__draws));
	tick(1);
	return host;
}

const draws = (kind: string) => globalThis.__draws.filter((d) => d.kind === kind);
/** The point at which the test starts counting: what the page has drawn so far. */
const mark = () => ({ line: draws("line").length, spark: draws("spark").length });
const since = (m: { line: number; spark: number }) => ({
	line: draws("line").length - m.line,
	spark: draws("spark").length - m.spark,
});
/** A service's cpu and memory sparkline, and one line chart: what a sample redraws on a stack of two services. */
const ONE_SAMPLE = { line: 1, spark: 4 };

test("a visible panel in a focused window redraws its charts for every sample (the 3 s cadence is kept)", () => {
	open();
	const m = mark();
	tick(2);
	expect(since(m)).toEqual(ONE_SAMPLE);
	tick(3);
	expect(since(m)).toEqual({ line: 2, spark: 8 });
});

test("a panel whose tab is behind another tab draws nothing for new samples, then catches up with the latest once shown", () => {
	const host = open();
	host.setVisible(false);
	flushSync();
	const m = mark();
	for (const cpu of [2, 3, 4]) tick(cpu);
	expect(since(m)).toEqual({ line: 0, spark: 0 });
	host.setVisible(true);
	flushSync();
	expect(since(m)).toEqual(ONE_SAMPLE);
	// The one catch-up draw is the newest sample: its axis ends at the last slot sampled, not at the one it was hidden at.
	const last = draws("line").at(-1);
	expect(JSON.parse(last.categories).length).toBe(100);
	expect(last.categories).not.toBe(draws("line").at(-2).categories);
	// And the cadence resumes.
	tick(5);
	expect(since(m)).toEqual({ line: 2, spark: 8 });
});

test("a panel in a window that lost focus draws nothing for new samples, then catches up once focus returns", () => {
	open();
	window.dispatchEvent(new Event("blur"));
	flushSync();
	const m = mark();
	for (const cpu of [2, 3, 4]) tick(cpu);
	expect(since(m)).toEqual({ line: 0, spark: 0 });
	window.dispatchEvent(new Event("focus"));
	flushSync();
	expect(since(m)).toEqual(ONE_SAMPLE);
	tick(5);
	expect(since(m)).toEqual({ line: 2, spark: 8 });
});

test("both gates must be open: a visible panel stays still while blurred, a focused window while the panel is behind a tab", () => {
	const host = open();
	host.setVisible(false);
	window.dispatchEvent(new Event("blur"));
	flushSync();
	const m = mark();
	tick(2);
	window.dispatchEvent(new Event("focus"));
	flushSync();
	tick(3);
	expect(since(m)).toEqual({ line: 0, spark: 0 });
	host.setVisible(true);
	flushSync();
	expect(since(m)).toEqual(ONE_SAMPLE);
});

// The runner re-sends every row on its own schedule, between the metrics samples: a new `service` object, same series.
test("rows re-sent between two samples redraw no sparkline (only a new sample does)", () => {
	open();
	const m = mark();
	for (const cpu of [7, 8, 9]) {
		store.apply("shop", { t: "rows", services: NAMES.map((n) => row(n, cpu)), tunnel: null });
		flushSync();
	}
	expect(since(m)).toEqual({ line: 0, spark: 0 });
	tick(10);
	expect(since(m)).toEqual(ONE_SAMPLE);
});
