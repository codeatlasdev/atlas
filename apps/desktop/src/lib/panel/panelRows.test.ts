// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import views from "../__fixtures__/project-views.json";

// Incident (#289, docs/perf/linux-webkit-study.md): the runner re-sends every service row every few seconds, and the
// dashboard answered each time with a new array of service names, which rebuilt the charts' series (a canvas redraw per
// chart) and the timeline's lanes although nothing had changed: ~6 ms of the web process per event, on a stack that was
// otherwise still. The real `Panel` and `StateTimeline`; the charts (the kit's canvas) are a probe that records `names`.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
const { default: NamedStub } = await import("../../../test/NamedStub.svelte");
const { default: ChartsProbe } = await import("../../../test/ChartsProbe.svelte");
const stub = (name: string) => ({ default: (anchor, props) => NamedStub(anchor, { ...props, name }) });
mock.module("./Charts.svelte", () => ({ default: ChartsProbe }));
for (const f of ["PanelHeader", "PanelKpis", "QuickLinks", "ServiceGrid", "StartInvite"])
	mock.module(`./${f}.svelte`, () => stub(f));
await (await import("../../../test/uiKit.ts")).useRealKit("Button", "Spinner", "EmptyState");
const { flushSync, mount, unmount } = await import("svelte");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { metrics } = await import("./metrics.svelte.ts");
const { default: Panel } = await import("./Panel.svelte");

let done: (() => void)[] = [];
afterEach(() => (done.forEach((f) => f()), (done = [])));

const row = (name: string, cpu = 1) => ({
	name,
	state: "running",
	exit_code: null,
	restarts: 0,
	note: null,
	cpu,
	mem_mb: 10,
});

/** A running project with the given services, one metrics sample taken (so the timeline has bands). */
function running(names: string[], id = "shop") {
	const project = {
		...views.detected,
		id,
		name: id,
		root: `/projects/${id}`,
		running: true,
		services_total: names.length,
	};
	store.projects = [project];
	store.configs[id] = { services: names.map((name) => ({ name, enabled: true, port: null })) };
	store.live[id] = {
		services: names.map((n) => row(n)),
		mode: "up",
		tunnel: null,
		events: [],
		operation: null,
	};
	metrics.sample(Date.now());
	return project;
}

function open(project) {
	globalThis.__chartNames = [];
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Panel, { target, props: { project } }));
	done.push(() => (unmount(app), target.remove(), delete globalThis.__chartNames));
	return target;
}

/** What the runner's `rows` event does, with the same values. */
const resend = (id: string, names: string[], cpu = 1) => {
	store.apply(id, { t: "rows", services: names.map((n) => row(n, cpu)), tunnel: null });
	flushSync();
};

test("the same rows sent again do not hand the charts a new set of names or move the timeline", async () => {
	const t = open(running(["api", "web"]));
	expect(t.querySelector(".tl")).not.toBeNull();
	expect(globalThis.__chartNames).toEqual([["api", "web"]]);
	const touched: string[] = [];
	const mo = new MutationObserver((rs) =>
		rs.forEach((r) => touched.push(`${r.type}:${r.attributeName ?? ""}`)),
	);
	mo.observe(t.querySelector(".tl"), {
		subtree: true,
		childList: true,
		attributes: true,
		characterData: true,
	});
	for (let i = 0; i < 3; i++) resend("shop", ["api", "web"], 1 + i);
	await Promise.resolve();
	expect(globalThis.__chartNames).toEqual([["api", "web"]]);
	expect(mo.takeRecords().length + touched.length).toBe(0);
	mo.disconnect();
});

test("a service that appears or goes does reach the charts and the timeline lanes", () => {
	const t = open(running(["api", "web"]));
	resend("shop", ["api", "web", "worker"]);
	expect(globalThis.__chartNames.at(-1)).toEqual(["api", "web", "worker"]);
	resend("shop", ["api", "worker"]);
	expect(globalThis.__chartNames.at(-1)).toEqual(["api", "worker"]);
	expect([...t.querySelectorAll(".tl .lane .label")].map((l) => l.textContent)).toEqual(["api", "worker"]);
});

test("a new sample still moves the timeline's right edge", () => {
	const t = open(running(["api"]));
	const band = () => t.querySelector(".tl .band")?.getAttribute("style");
	const before = band();
	metrics.sample(Date.now() + 6000);
	flushSync();
	expect(band()).not.toBe(before);
});
