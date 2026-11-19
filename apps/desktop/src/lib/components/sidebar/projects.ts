/** Adding, relinking and cleaning up projects in the sidebar. */

import { toast } from "@codeatlasdev/ui";
import { api, type ProjectView } from "#lib/api.ts";
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { discovery } from "#lib/stores/discovery.svelte.ts";
import { addedToast, addPlan, removeDescription } from "./addProject.ts";
import { store } from "#lib/stores/projects.svelte.ts";

/** Pick a folder and add it. */
export async function addProject() {
	const dir = await api.pickPath({ directory: true, title: m.sidebar_projects_add_title() });
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
		toast.info(m.sidebar_projects_already_added({ name: probe.name }));
		return;
	}
	if (plan.inside) {
		const ok = await confirmDialog(m.sidebar_projects_inside_title({ name: probe.name }), {
			description: m.sidebar_projects_inside_body({ picked: probe.picked, root: plan.root }),
			confirmLabel: m.sidebar_projects_add_title(),
		});
		if (!ok) return;
	}
	const p = await store.run("add", () => api.add(plan.root));
	if (!p) return;
	await store.refresh();
	store.selected = p.id;
	await announceAdded(p.id, p);
}

/** After adding, say what Atlas made of a folder that has no atlas.yaml (nothing for one that has). Every add path
 * goes through here: the sidebar, drag-drop, the Find projects dialog and onboarding. */
async function announceAdded(id: string, fallback: ProjectView) {
	// `refresh` loads configs without awaiting them: wait for this one so the toast can name the stack.
	await store.loadConfig(id);
	const added = store.projects.find((x) => x.id === id) ?? fallback;
	const line = addedToast(added, store.configs[id]?.detected ?? null);
	if (line) toast.info(line);
}

/** Open "Find projects": the dialog fills in while the scan runs. */
export function showDiscover() {
	return discovery.show();
}

/** Register the chosen paths, then fall back to the first project when nothing is selected. */
export async function addPaths(paths: string[]) {
	const before = new Set(store.projects.map((p) => p.id));
	for (const p of paths) await store.run("add", () => api.add(p));
	await store.refresh();
	if (!store.selected) store.selected = store.sorted[0]?.id ?? null;
	// One folder: name what was detected. Several: the sidebar shows them, a toast per folder would be noise.
	const fresh = store.projects.filter((p) => !before.has(p.id));
	if (fresh.length === 1) await announceAdded(fresh[0]!.id, fresh[0]!);
}

/** Re-check every project on disk and say what changed. */
export async function refreshProjects() {
	const before = new Set(store.missing.map((p) => p.id));
	await store.refresh();
	const now = store.missing;
	const fresh = now.filter((p) => !before.has(p.id)).length;
	const back = [...before].filter((id) => store.projects.some((p) => p.id === id && !p.missing)).length;
	if (!now.length && !back) toast.info(m.sidebar_projects_all_found());
	else if (back && !fresh) toast.success(m.sidebar_projects_found_again({ count: back }));
	else if (now.length) toast.warning(m.sidebar_projects_not_found({ count: now.length }));
}

/** Pick the new folder of a missing project. */
export async function locateProject(id: string) {
	const dir = await api.pickPath({ directory: true, title: m.sidebar_projects_locate_title() });
	if (typeof dir === "string") await relinkProject(id, dir);
}

export async function relinkProject(id: string, dir: string) {
	const p = await store.run("relink", () => api.relink(id, dir));
	if (!p) return;
	await store.refresh();
	toast.success(m.sidebar_projects_relinked({ name: p.name, dir }));
}

/** Take one project out of Atlas: stops its stack if it runs; never touches the folder. */
export async function removeProject(id: string) {
	const p = store.projects.find((x) => x.id === id);
	if (!p) return;
	const linked = store.projects.filter((x) => x.parent === id).length;
	const ok = await confirmDialog(m.sidebar_projects_remove_title({ name: p.name }), {
		description: removeDescription(p, linked),
		confirmLabel: m.common_action_remove(),
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
	const ok = await confirmDialog(m.sidebar_projects_remove_missing_title({ count: gone.length }), {
		description: `${list(gone.map((p) => p.name))}\n${m.sidebar_projects_remove_missing_note()}`,
		confirmLabel: m.sidebar_remove_all(),
		destructive: true,
	});
	if (!ok) return;
	const n = await store.run("remove-missing", () => api.removeMissing());
	await store.refresh();
	if (n) toast.success(m.sidebar_projects_removed_missing({ count: n }));
}
