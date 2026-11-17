// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import views from "../../lib/__fixtures__/project-views.json";

// Issue #499: opening `~/Downloads` showed "Auto-detected", "Save as atlas.yaml" and Start for a folder that is not a
// project. The header is the real component over the REAL serialization of both views (fixture written by the
// desktop's `projects.rs` tests), so the `kind` it branches on is the field Rust sends. Dynamic imports on purpose:
// the Svelte loader registered by test/setup.ts and the mocks must exist before these modules load.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
const { projectsMock } = await import("../../../test/projectsMock.ts");
mock.module("#lib/components/sidebar/projects.ts", () => projectsMock());
// The crumbs, the right-dock toggle and the activity buttons load the shell store and native layers, which bun cannot
// link (the theme store pulls the whole kit); they are replaced by marker elements and the header around them is real.
const { default: NamedStub } = await import("../../../test/NamedStub.svelte");
const stub = (name: string) => ({ default: (anchor, props) => NamedStub(anchor, { ...props, name }) });
mock.module("#lib/shell/Crumbs.svelte", () => stub("crumbs"));
mock.module("#lib/shell/RightToggle.svelte", () => stub("right-toggle"));
mock.module("#lib/panel/PanelOps.svelte", () => stub("panel-ops"));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (
	await import("../../../test/uiKit.ts")
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
const { flushSync, mount, unmount, tick } = await import("svelte");
const { api } = await import("#lib/api.ts");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { default: ProjectHeader } = await import("./ProjectHeader.svelte");

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
});

function header(p) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(ProjectHeader, { target, props: { p } }));
	done = () => (unmount(app), target.remove());
	return target;
}
const labels = (t) => [...t.querySelectorAll("button")].map((b) => (b.textContent ?? "").trim());
const settle = async () => (flushSync(), await tick());

test("a plain folder's header has no atlas.yaml prompt, no Start and no services chrome", () => {
	const t = header(views.folder);
	const text = t.textContent;
	expect(text).not.toContain("Auto-detected");
	expect(text).not.toContain("Save as atlas.yaml");
	expect(text).not.toContain("Stopped");
	expect(labels(t)).not.toContain("Start");
	expect(labels(t)).not.toContain("Stop");
	expect(t.querySelector('[data-stub="panel-ops"]')).toBeNull();
	// What it does say: it is a folder, and the one action is to make it a project.
	expect(text).toContain("Folder");
	expect(labels(t)).toContain("Use as project");
});

test("a project without atlas.yaml still shows the detected config, Save and Start", () => {
	const t = header(views.detected);
	expect(t.textContent).toContain("Auto-detected");
	expect(labels(t)).toContain("Save as atlas.yaml");
	expect(labels(t)).toContain("Start");
	expect(labels(t)).not.toContain("Use as project");
	expect(t.querySelector('[data-stub="panel-ops"]')).not.toBeNull();
});

test("Use as project promotes the folder through the backend, nothing else", async () => {
	const calls = [];
	const real = { promote: api.promote, saveConfig: api.saveConfig, start: api.start };
	api.promote = async (id) => void calls.push(["promote", id]);
	api.saveConfig = async (id) => void calls.push(["saveConfig", id]);
	api.start = async (id) => void calls.push(["start", id]);
	const refresh = store.refresh;
	store.refresh = async () => {};
	try {
		const t = header(views.folder);
		[...t.querySelectorAll("button")].find((b) => b.textContent.trim() === "Use as project").click();
		await settle();
		await settle();
		expect(calls).toEqual([["promote", "dl"]]);
	} finally {
		Object.assign(api, real);
		store.refresh = refresh;
	}
});
