import { m } from "#lib/i18n/m.ts";
import { revealLabel } from "#lib/keys.ts";

/** What a right-click or the `…` button was aimed at: a file, a folder, or the empty area (the worktree root). */
export type MenuTarget = { kind: "file" | "dir"; rel: string; status?: string } | { kind: "root" };

export type MenuAction =
	| "open"
	| "open-side"
	| "new-file"
	| "new-folder"
	| "rename"
	| "duplicate"
	| "copy-path"
	| "copy-relative-path"
	| "terminal"
	| "reveal"
	| "stage"
	| "discard"
	| "delete";

export interface MenuItem {
	action: MenuAction;
	label: string;
	destructive?: boolean;
	/** A separator is drawn before this item. */
	group?: number;
}

/** The two "New …" items; built when a menu opens so the labels follow the app's language. */
const newItems = () =>
	[
		{ action: "new-file", label: m.workbench_explorer_new_file() },
		{ action: "new-folder", label: m.workbench_explorer_new_folder() },
	] as const satisfies MenuItem[];

/**
 * The one item list behind both the `…` button and the right-click / Shift+F10 menu, so they cannot drift.
 * Git items show only for a changed, non-ignored file (`status` as in `statusMap`); the root offers just
 * "New …" and "Reveal in Finder" because it can be neither renamed nor deleted from the tree.
 */
export function menuItems(t: MenuTarget): MenuItem[] {
	if (t.kind === "root") {
		return [
			...newItems().map((i) => ({ ...i, group: 0 })),
			{ action: "terminal", label: m.workbench_explorer_menu_open_in_terminal(), group: 1 },
			{ action: "reveal", label: revealLabel(), group: 1 },
			{ action: "copy-path", label: m.workbench_explorer_menu_copy_path(), group: 1 },
		];
	}
	const items: MenuItem[] = [];
	if (t.kind === "file") {
		items.push(
			{ action: "open", label: m.common_action_open(), group: 0 },
			{ action: "open-side", label: m.workbench_explorer_menu_open_to_side(), group: 0 },
		);
	} else items.push(...newItems().map((i) => ({ ...i, group: 0 })));
	items.push(
		{ action: "rename", label: m.common_action_rename(), group: 1 },
		{ action: "duplicate", label: m.workbench_explorer_menu_duplicate(), group: 1 },
		{ action: "copy-path", label: m.workbench_explorer_menu_copy_path(), group: 2 },
		{ action: "copy-relative-path", label: m.workbench_explorer_menu_copy_relative_path(), group: 2 },
		{ action: "terminal", label: m.workbench_explorer_menu_open_in_terminal(), group: 2 },
		{ action: "reveal", label: revealLabel(), group: 2 },
	);
	const changed = t.kind === "file" && t.status && t.status !== "ignored";
	if (changed) {
		items.push({ action: "stage", label: m.workbench_explorer_menu_stage(), group: 3 });
		if (t.status !== "untracked") {
			items.push({ action: "discard", label: m.workbench_explorer_menu_discard_changes(), destructive: true, group: 3 });
		}
	}
	items.push({ action: "delete", label: m.common_action_delete(), destructive: true, group: 4 });
	return items;
}

/** `name.ts` → `name copy.ts`, `dir` → `dir copy`; a second copy is `name copy 2.ts`. Taken names are skipped. */
export function duplicateName(name: string, taken: ReadonlySet<string>): string {
	const dot = name.lastIndexOf(".");
	const [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
	for (let n = 1; ; n++) {
		const candidate = `${stem} copy${n > 1 ? ` ${n}` : ""}${ext}`;
		if (!taken.has(candidate)) return candidate;
	}
}
