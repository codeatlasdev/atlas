import type { FsEntry } from "#lib/host.ts";

export interface TreeRow {
	rel: string;
	dir: boolean;
}

/** Visible rows in display order (folders descend only when expanded and loaded). */
export function visibleRows(
	children: Record<string, FsEntry[]>,
	expanded: Record<string, boolean>,
): TreeRow[] {
	const out: TreeRow[] = [];
	const walk = (dir: string) => {
		for (const e of children[dir] ?? []) {
			const rel = dir ? `${dir}/${e.name}` : e.name;
			const isDir = e.kind === "dir";
			out.push({ rel, dir: isDir });
			if (isDir && expanded[rel]) walk(rel);
		}
	};
	walk("");
	return out;
}

/** Folders above `rel`, outermost first (`a/b/c.ts` → `a`, `a/b`). */
export function ancestorsOf(rel: string): string[] {
	const parts = rel.split("/").slice(0, -1);
	return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}
