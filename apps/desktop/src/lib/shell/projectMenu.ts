/** The project actions of a space dot (right-click): the sidebar row menu's identity, pin, reveal and remove. */

import { api, type ProjectView } from "#lib/api.ts";
import type { PointerMenuItem } from "#lib/components/PointerMenu.svelte";
import { removeProject } from "#lib/components/sidebar/projects.ts";
import { identities } from "#lib/stores/identity.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";

/** What a right-click on a project's icon offers. Remove and the icon editor are for the repo itself, never a worktree. */
export function projectMenuItems(p: ProjectView): PointerMenuItem[] {
	const items: PointerMenuItem[] = [];
	if (!p.parent)
		items.push({ label: "Change icon and color…", group: 0, run: () => (identities.editing = p.id) });
	items.push({
		label: p.pinned ? "Unpin" : "Pin to top",
		group: 0,
		run: () => void store.run("pin", () => api.pin(p.id, !p.pinned)),
	});
	if (!p.missing) {
		items.push({
			label: "Reveal in Finder",
			group: 0,
			run: () => void store.run("reveal", () => api.reveal(p.id)),
		});
	}
	if (!p.parent) {
		items.push({
			label: "Remove from Atlas…",
			group: 1,
			destructive: true,
			run: () => void removeProject(p.id),
		});
	}
	return items;
}
