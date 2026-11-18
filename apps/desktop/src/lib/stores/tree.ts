import type { FsEntry } from "#lib/host.ts";

export interface TreeRow {
	rel: string;
	name: string;
	dir: boolean;
	/** Folders above it (0 at the top level): the row's indent. */
	depth: number;
}

/** Visible rows in display order (folders descend only when expanded and loaded). */
export function visibleRows(
	children: Record<string, FsEntry[]>,
	expanded: Record<string, boolean>,
): TreeRow[] {
	const out: TreeRow[] = [];
	const walk = (dir: string, depth: number) => {
		for (const e of children[dir] ?? []) {
			const rel = dir ? `${dir}/${e.name}` : e.name;
			const isDir = e.kind === "dir";
			out.push({ rel, name: e.name, dir: isDir, depth });
			if (isDir && expanded[rel]) walk(rel, depth + 1);
		}
	};
	walk("", 0);
	return out;
}

/** Folders above `rel`, outermost first (`a/b/c.ts` → `a`, `a/b`). */
export function ancestorsOf(rel: string): string[] {
	const parts = rel.split("/").slice(0, -1);
	return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}

export const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

/** Folder part of a relative path (`a/b/c.ts` → `a/b`, `c.ts` → ``). */
export const parentOf = (rel: string) => rel.split("/").slice(0, -1).join("/");

interface StatusEntry {
	path: string;
	x: string;
	y: string;
	conflict?: boolean;
}

/**
 * Change state per path from `git status --ignored` entries. Changed files mark every folder above them `modified`;
 * ignored ones (`x`/`y` = `!`, a collapsed folder arrives as `dir/`) are `ignored` and mark nothing above them.
 */
export function statusMap(entries: StatusEntry[]): Record<string, string> {
	const map: Record<string, string> = {};
	for (const e of entries) {
		if (e.x === "!") {
			map[e.path.replace(/\/$/, "")] ??= "ignored";
			continue;
		}
		map[e.path] = e.x === "?" ? "untracked" : e.x === "D" || e.y === "D" ? "deleted" : "modified";
		const parts = e.path.split("/");
		for (let i = 1; i < parts.length; i++) map[parts.slice(0, i).join("/")] ??= "modified";
	}
	return map;
}

/**
 * State of `rel`: its own entry, else `ignored` when a folder above it is (git reports a wholly ignored folder as one
 * entry, the explorer lists what is inside).
 */
export function statusOf(map: Record<string, string>, rel: string): string | undefined {
	if (map[rel]) return map[rel];
	return ancestorsOf(rel).some((a) => map[a] === "ignored") ? "ignored" : undefined;
}

/** Source-control groups; ignored entries (`!`) belong to none of them. */
export function statusGroups<T extends StatusEntry>(entries: T[]) {
	const tracked = entries.filter((e) => e.x !== "!");
	return {
		staged: tracked.filter((e) => e.x !== "." && e.x !== "?" && !e.conflict),
		changes: tracked.filter((e) => e.y !== "." && e.y !== "?" && !e.conflict),
		untracked: tracked.filter((e) => e.x === "?"),
		conflicts: tracked.filter((e) => e.conflict),
	};
}

/** What a handled navigation key asks for; every field is optional (a handled key may do nothing). */
interface NavMove {
	/** Move the cursor to this row. */
	focus?: string;
	/** Fold/unfold this folder. */
	toggle?: string;
	/** Activate this row: open the file or toggle the folder. */
	open?: TreeRow;
	/** Start renaming this row. */
	rename?: string;
}

/**
 * Keyboard navigation (WAI-ARIA tree): arrows move or fold, Home/End jump, Enter opens, F2 renames.
 * `null` means the key is not a navigation key and keeps its default behavior.
 */
export function navMove(
	key: string,
	rows: TreeRow[],
	cursor: string,
	expanded: Record<string, boolean>,
): NavMove | null {
	const i = rows.findIndex((r) => r.rel === cursor);
	const cur = rows[i];
	switch (key) {
		case "ArrowDown":
			return { focus: rows[Math.min(rows.length - 1, i + 1)]?.rel };
		case "ArrowUp":
			return { focus: rows[Math.max(0, i - 1)]?.rel };
		case "Home":
			return { focus: rows[0]?.rel };
		case "End":
			return { focus: rows[rows.length - 1]?.rel };
		case "ArrowRight":
			if (cur?.dir && !expanded[cur.rel]) return { toggle: cur.rel };
			return { focus: cur?.dir ? rows[i + 1]?.rel : undefined };
		case "ArrowLeft":
			if (cur?.dir && expanded[cur.rel]) return { toggle: cur.rel };
			return { focus: cur ? parentOf(cur.rel) || undefined : undefined };
		case "Enter":
			return { open: cur };
		case "F2":
			return { rename: cur?.rel };
		default:
			return null;
	}
}
