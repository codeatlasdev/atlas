/** Adding, relinking and cleaning up projects in the sidebar. */

import { toast } from "@codeatlasdev/ui";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { api } from "#lib/api.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { discovery } from "#lib/stores/discovery.svelte.ts";
import { addPlan, removeDescription } from "./addProject.ts";
import { store } from "#lib/stores/projects.svelte.ts";

/** Pick a folder and add it. */
export async function addProject() {
	const dir = await pickFolder({ directory: true, title: "Add project" });
	if (typeof dir === "string") await addFolder(dir);
}

/**
 * Add one folder (picked or dropped) to the sidebar and select it. No `atlas.yaml` is needed: a folder without one is
 * added with its stack detected in memory, and nothing is written (the project header offers "Save as atlas.yaml").
 */
export async function addFolder(dir: string) {
	const probe = await store.run("probe", () => api.probe(dir));
	if (!probe) return;
	const plan = addPlan(probe);
	if (plan.kind === "select") {
		store.selected = plan.id;
		toast.info(`${probe.name} is already in Atlas`);
		return;
	}
	if (plan.inside) {
		const ok = await confirmDialog(`Add ${probe.name}?`, {
			description: `${probe.picked} is inside the project at ${plan.root}, so that project is added.`,
			confirmLabel: "Add project",
		});
		if (!ok) return;
	}
	const p = await store.run("add", () => api.add(plan.root));
	if (!p) return;
	await store.refresh();
	store.selected = p.id;
}

/** Open "Find projects": the dialog fills in while the scan runs. */
export function showDiscover() {
	return discovery.show();
}

/** Register the chosen paths, then fall back to the first project when nothing is selected. */
export async function addPaths(paths: string[]) {
	for (const p of paths) await store.run("add", () => api.add(p));
	await store.refresh();
	if (!store.selected) store.selected = store.sorted[0]?.id ?? null;
}

/** Re-check every project on disk and say what changed. */
export async function refreshProjects() {
	const before = new Set(store.missing.map((p) => p.id));
	await store.refresh();
	const now = store.missing;
	const fresh = now.filter((p) => !before.has(p.id)).length;
	const back = [...before].filter((id) => store.projects.some((p) => p.id === id && !p.missing)).length;
	if (!now.length && !back) toast.info("All projects are where Atlas expects them");
	else if (back && !fresh) toast.success(`${back} project${back > 1 ? "s" : ""} found again`);
	else if (now.length)
		toast.warning(`${now.length} project${now.length > 1 ? "s" : ""} not found on this computer`);
}

/** Pick the new folder of a missing project. */
export async function locateProject(id: string) {
	const dir = await pickFolder({ directory: true, title: "Locate the project folder" });
	if (typeof dir === "string") await relinkProject(id, dir);
}

export async function relinkProject(id: string, dir: string) {
	const p = await store.run("relink", () => api.relink(id, dir));
	if (!p) return;
	await store.refresh();
	toast.success(`${p.name} now points to ${dir}`);
}

/** Take one project out of Atlas: stops its stack if it runs; never touches the folder. */
export async function removeProject(id: string) {
	const p = store.projects.find((x) => x.id === id);
	if (!p) return;
	const linked = store.projects.filter((x) => x.parent === id).length;
	const ok = await confirmDialog(`Remove ${p.name} from Atlas?`, {
		description: removeDescription(p, linked),
		confirmLabel: "Remove",
		destructive: true,
	});
	if (!ok) return;
	await store.run("remove", () => api.remove(id));
	await store.refresh();
}

/** Remove every project whose folder is gone, after confirming. */
export async function removeAllMissing() {
	const gone = store.missing;
	if (!gone.length) return;
	const ok = await confirmDialog(`Remove ${gone.length} missing project${gone.length > 1 ? "s" : ""}?`, {
		description: `${gone.map((p) => p.name).join(", ")}\nOnly the Atlas entries go; nothing on disk is touched.`,
		confirmLabel: "Remove all",
		destructive: true,
	});
	if (!ok) return;
	const n = await store.run("remove-missing", () => api.removeMissing());
	await store.refresh();
	if (n) toast.success(`Removed ${n} missing project${n > 1 ? "s" : ""}`);
}
