// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import views from "./__fixtures__/project-views.json";
import { infiniteAnimations, loadStyles } from "../../test/motion.ts";

// Incident (#289, docs/perf/linux-webkit-study.md): a project whose services are all up kept the header's halo, the
// sidebar dot's ring and the space strip's breathing running for as long as the app was open, which holds WebKitGTK's
// compositing pipeline at 60 fps (27.8 % of a core on a GPU, 58 % on llvmpipe). Healthy is a settled state: still.
// Real components, real compiled CSS.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
// The theme store globs bundled themes with Vite, which bun cannot do (Terminal.test.ts mocks it the same way).
mock.module("#lib/stores/theme.svelte.ts", () => ({ themes: { xterm: {} } }));
const { projectsMock } = await import("../../test/projectsMock.ts");
mock.module("#lib/components/sidebar/projects.ts", () => projectsMock());
const { default: NamedStub } = await import("../../test/NamedStub.svelte");
const stub = (name: string) => ({ default: (anchor, props) => NamedStub(anchor, { ...props, name }) });
mock.module("#lib/shell/Crumbs.svelte", () => stub("crumbs"));
mock.module("#lib/shell/RightToggle.svelte", () => stub("right-toggle"));
mock.module("#lib/panel/PanelOps.svelte", () => stub("panel-ops"));
await (
	await import("../../test/uiKit.ts")
).useRealKit(
	"Button",
	"Spinner",
	"Kbd",
	"Tooltip",
	"DropdownMenu",
	"DropdownMenu/DropdownMenuItem",
	"DropdownMenu/DropdownMenuLabel",
	"DropdownMenu/DropdownMenuSeparator",
	"NotificationBell",
);
const { flushSync, mount, unmount } = await import("svelte");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { default: ProjectHeader } = await import("../routes/(app)/ProjectHeader.svelte");
const { default: ProjectHead } = await import("./components/sidebar/ProjectHead.svelte");
const { default: SpaceDots } = await import("./shell/SpaceDots.svelte");

const src = (p: string) => new URL(p, import.meta.url).pathname;
let done: (() => void)[] = [];
afterEach(() => (done.forEach((f) => f()), (done = [])));

function up(states: string[], id = "shop") {
	const p = {
		...views.detected,
		id,
		name: id,
		root: `/projects/${id}`,
		running: true,
		services_total: states.length,
	};
	store.live[id] = {
		services: states.map((state, i) => ({ name: `s${i}`, state })),
		mode: "up",
		tunnel: null,
		events: [],
		operation: null,
	};
	return p;
}

async function render(component, props, css: string[]) {
	done.push(await loadStyles(css.map(src), [src("../app.css")]));
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(component, { target, props }));
	done.push(() => (unmount(app), target.remove()));
	return target;
}

test("a project header whose services are all up is still", async () => {
	const p = up(["running", "running"]);
	store.projects = [p];
	const t = await render(ProjectHeader, { p }, ["../routes/(app)/ProjectHeader.svelte"]);
	expect(t.querySelector(".dot.pulse")).not.toBeNull();
	expect(infiniteAnimations(t)).toEqual([]);
});

test("a healthy project's row in the sidebar is still, a starting one pulses (the loop detector sees it)", async () => {
	const ok = up(["running"], "ok");
	const starting = up(["running", "starting"], "go");
	store.projects = [ok, starting];
	const css = ["./components/sidebar/ProjectHead.svelte"];
	const row = (r) => render(ProjectHead, { r, active: false }, css);
	expect(infiniteAnimations(await row(ok))).toEqual([]);
	expect(infiniteAnimations(await row(starting)).join()).toContain("pulse");
});

test("the space strip does not breathe for a running stack", async () => {
	store.projects = [up(["running"], "a"), up(["running"], "b")];
	const t = await render(SpaceDots, {}, ["./shell/SpaceDots.svelte"]);
	expect(t.querySelector(".space.up")).not.toBeNull();
	expect(infiniteAnimations(t)).toEqual([]);
});
