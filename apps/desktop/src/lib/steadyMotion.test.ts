// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { infiniteAnimations, loadStyles } from "../../test/motion.ts";

// Incident (#289, docs/perf/linux-webkit-study.md): a workspace whose stack is up kept four `infinite` CSS animations
// running (the sidebar dot's ring, the header's halo and one halo per service card). One running animation holds
// WebKitGTK's web process -> UI process -> compositor pipeline at 60 fps: 27.8 % of a core on a GPU, 58 % on llvmpipe,
// against 0.5 % for the same screen without them. A SETTLED state (service up, project healthy, agent idle or asleep)
// must be still; only something that is changing (a service starting, a crewmate working) may loop.
// Real components with their real compiled CSS: the check is on what the elements would run, not on the source text.
const { flushSync, mount, unmount } = await import("svelte");
const { default: CardHead } = await import("./panel/CardHead.svelte");
const { default: StatusRing } = await import("./components/StatusRing.svelte");
const { default: CrewmateFace } = await import("./crew/CrewmateFace.svelte");

const src = (p: string) => new URL(p, import.meta.url).pathname;
let done: (() => void)[] = [];
afterEach(() => (done.forEach((f) => f()), (done = [])));

async function render(component, props, css: string[]) {
	done.push(await loadStyles(css.map(src), [src("../app.css")]));
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(component, { target, props }));
	done.push(() => (unmount(app), target.remove()));
	return target;
}

const service = (state: string) => ({ name: "api", state, exit_code: null, restarts: 0, note: null });

test("a running service card is still: its halo does not loop", async () => {
	const t = await render(
		CardHead,
		{ service: service("running"), def: undefined, busy: false, onopen() {} },
		["./panel/CardHead.svelte", "./components/StatusRing.svelte"],
	);
	expect(t.querySelector(".ring.pulse")).not.toBeNull();
	expect(infiniteAnimations(t)).toEqual([]);
});

test("the loop detector sees a service that is starting (its ring spins)", async () => {
	const t = await render(StatusRing, { state: "starting" }, ["./components/StatusRing.svelte"]);
	expect(infiniteAnimations(t).join()).toContain("spin");
});

test("an idle and a sleeping crewmate are still, a working one with a hat loops", async () => {
	const css = ["./crew/CrewmateFace.svelte", "./crew/CrewmateHat.svelte"];
	const look = { hat: "beanie" };
	expect(infiniteAnimations(await render(CrewmateFace, { seed: "a", state: "idle", look }, css))).toEqual([]);
	expect(infiniteAnimations(await render(CrewmateFace, { seed: "a", state: "offline", look }, css))).toEqual(
		[],
	);
	expect(
		infiniteAnimations(await render(CrewmateFace, { seed: "a", state: "working", look }, css)).join(),
	).toContain("pom");
});
