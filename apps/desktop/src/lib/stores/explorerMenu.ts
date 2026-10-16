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

const NEW = [
	{ action: "new-file", label: "New file" },
	{ action: "new-folder", label: "New folder" },
] as const satisfies MenuItem[];

/**
 * The one item list behind both the `…` button and the right-click / Shift+F10 menu, so they cannot drift.
 * Git items show only for a changed, non-ignored file (`status` as in `statusMap`); the root offers just
 * "New …" and "Reveal in Finder" because it can be neither renamed nor deleted from the tree.
 */
export function menuItems(t: MenuTarget): MenuItem[] {
	if (t.kind === "root") {
		return [
			...NEW.map((i) => ({ ...i, group: 0 })),
			{ action: "terminal", label: "Open in terminal", group: 1 },
			{ action: "reveal", label: "Reveal in Finder", group: 1 },
			{ action: "copy-path", label: "Copy path", group: 1 },
		];
	}
	const items: MenuItem[] = [];
	if (t.kind === "file") {
		items.push(
			{ action: "open", label: "Open", group: 0 },
			{ action: "open-side", label: "Open to the side", group: 0 },
		);
	} else items.push(...NEW.map((i) => ({ ...i, group: 0 })));
	items.push(
		{ action: "rename", label: "Rename", group: 1 },
		{ action: "duplicate", label: "Duplicate", group: 1 },
		{ action: "copy-path", label: "Copy path", group: 2 },
		{ action: "copy-relative-path", label: "Copy relative path", group: 2 },
		{ action: "terminal", label: "Open in terminal", group: 2 },
		{ action: "reveal", label: "Reveal in Finder", group: 2 },
	);
	const changed = t.kind === "file" && t.status && t.status !== "ignored";
	if (changed) {
		items.push({ action: "stage", label: "Stage", group: 3 });
		if (t.status !== "untracked") {
			items.push({ action: "discard", label: "Discard changes", destructive: true, group: 3 });
		}
	}
	items.push({ action: "delete", label: "Delete", destructive: true, group: 4 });
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
