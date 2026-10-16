// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { addPlan, droppedOn, initDescription, removeDescription } from "./addProject.ts";
// The REAL serialization of `project_probe` (crates/atlas-desktop/src/cmds/projects.rs, probe_fixture_is_the_real_serialization).
import probeFixture from "#lib/__fixtures__/project-probe.json";

// Incident (#65): picking a folder without atlas.yaml ended in "no atlas.yaml in <dir>". The plan for the real Rust
// payload of such a folder must be "create the config", never "add".
test("the real probe of a folder without atlas.yaml plans an init of that folder", () => {
	expect(addPlan(probeFixture)).toEqual({ kind: "init", dir: probeFixture.picked });
});

const probe = (over) => ({ ...probeFixture, ...over });

test("a folder with a config plans an add of its root; a subfolder of a project says so", () => {
	expect(addPlan(probe({ has_config: true, root: probeFixture.picked }))).toEqual({
		kind: "add",
		root: probeFixture.picked,
		inside: false,
	});
	expect(addPlan(probe({ has_config: true, root: "/projects" }))).toEqual({
		kind: "add",
		root: "/projects",
		inside: true,
	});
});

test("an already registered folder is selected, whatever else is true of it", () => {
	expect(addPlan(probe({ has_config: false, registered: "abc" }))).toEqual({ kind: "select", id: "abc" });
	expect(addPlan(probe({ has_config: true, registered: "abc", root: "/up" }))).toEqual({
		kind: "select",
		id: "abc",
	});
});

test("the init confirmation shows what was detected and the file to be created", () => {
	const text = initDescription("/p/shop", {
		yaml: "name: shop\n\nservices:\n  dev:\n    command: vite\n    port: 5173\n",
		detected: "typescript (vite)",
		services: ["dev"],
	});
	expect(text).toContain("/p/shop");
	expect(text).toContain("Detected typescript (vite): dev.");
	expect(text).toContain("name: shop");
	expect(initDescription("/p", { yaml: "name: p\n", detected: null, services: [] })).toContain(
		"Nothing recognised",
	);
});

// Acceptance (#65): removing says it never deletes files, that a running stack is stopped, and that worktrees follow.
test("the remove confirmation promises the files stay and names what else happens", () => {
	const idle = removeDescription({ root: "/p/a", running: false }, 0);
	expect(idle).toContain("/p/a");
	expect(idle).toContain("no file or folder on disk is touched");
	expect(idle).not.toContain("stopped");
	expect(idle).not.toContain("worktree");
	const busy = removeDescription({ root: "/p/a", running: true }, 2);
	expect(busy).toContain("running stack is stopped first");
	expect(busy).toContain("2 linked worktrees are removed");
	expect(removeDescription({ root: "/p/a", running: false }, 1)).toContain("1 linked worktree is removed");
});

// Tauri reports the drop point in physical pixels: on a 2x display a point over the sidebar (200 css px wide) is at
// x < 400, so comparing it raw against the css rect would accept drops over the editor and the sidebar's lower half.
test("a drop is over the sidebar when its physical position, scaled by the pixel ratio, is inside the css rect", () => {
	const rect = { left: 0, right: 240, top: 0, bottom: 800 };
	expect(droppedOn(rect, { x: 200, y: 400 }, 2)).toBe(true);
	expect(droppedOn(rect, { x: 600, y: 400 }, 2)).toBe(false);
	expect(droppedOn(rect, { x: 200, y: 1700 }, 2)).toBe(false);
	expect(droppedOn(rect, { x: 200, y: 400 }, 1)).toBe(true);
	expect(droppedOn(rect, { x: 300, y: 400 }, 1)).toBe(false);
});
