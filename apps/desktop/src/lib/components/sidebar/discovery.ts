/** Pure helpers behind the "Find projects" dialog: grouping, filtering, relink suggestions. */

/** One folder reported by the Rust scan (`discover://event` `found`). */
export interface Found {
	path: string;
	root: string;
	name: string;
	services: number;
	error: string | null;
	branch: string | null;
	remote: string | null;
	/** Seconds since the epoch. */
	modified: number | null;
	has_config: boolean;
	/** A package inside another project (monorepo). */
	nested: boolean;
	stack: string | null;
}

export interface Group {
	root: string;
	items: Found[];
}

/** Case-insensitive match on name, path, branch and remote. */
export function matchesQuery(f: Found, query: string): boolean {
	const q = query.trim().toLowerCase();
	return !q || [f.name, f.path, f.branch ?? "", f.remote ?? ""].some((x) => x.toLowerCase().includes(q));
}

/** Group key of folders Spotlight found outside every scanned root. */
export const SPOTLIGHT = "";

/**
 * Group by scan root, newest first inside each group; Spotlight hits outside every root share
 * one `SPOTLIGHT` group, last. Roots keep first-seen order so streaming never reshuffles
 * groups the user is reading.
 */
export function groupFound(found: Found[], query = ""): Group[] {
	const groups = new Map<string, Found[]>();
	for (const f of found) {
		if (!matchesQuery(f, query)) continue;
		const list = groups.get(f.root);
		if (list) list.push(f);
		else groups.set(f.root, [f]);
	}
	const ordered = [...groups].sort(([a], [b]) => Number(a === SPOTLIGHT) - Number(b === SPOTLIGHT));
	return ordered.map(([root, items]) => ({
		root,
		items: items.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0) || a.name.localeCompare(b.name)),
	}));
}

/** Projects with `atlas.yaml` vs. folders that merely could use Atlas. */
export function splitFound(found: Found[]): { projects: Found[]; candidates: Found[] } {
	return { projects: found.filter((f) => f.has_config), candidates: found.filter((f) => !f.has_config) };
}

/** Folders that can be added: valid-or-not config, not already in the sidebar. */
export function addable(found: Found[], registered: ReadonlySet<string>): Found[] {
	return found.filter((f) => f.has_config && !registered.has(f.path));
}

/** `git@github.com:Org/Repo.git` and `https://github.com/org/repo/` are the same remote. */
export function normalizeRemote(url: string): string {
	let u = url
		.trim()
		.replace(/\/+$/, "")
		.replace(/\.git$/, "");
	u = u.replace(/^[a-z+]+:\/\//, "").replace(/^[^@/]*@/, "");
	return u.replace(":", "/").toLowerCase();
}

/**
 * Which discovered folder is the moved copy of a missing project: same remote first, then the
 * same name. Only an unambiguous match counts, and only folders that are not registered.
 */
export function movedCopy(
	missing: { name: string; remote?: string | null },
	found: Found[],
	registered: ReadonlySet<string>,
): Found | null {
	const pool = addable(found, registered);
	const only = (xs: Found[]) => (xs.length === 1 ? xs[0] : null);
	if (missing.remote) {
		const r = normalizeRemote(missing.remote);
		const hit = only(pool.filter((f) => f.remote && normalizeRemote(f.remote) === r));
		if (hit) return hit;
	}
	return only(pool.filter((f) => f.name === missing.name));
}

/** "just now", "3 days ago"… for the last-modified column. */
export function ago(seconds: number | null, now = Date.now() / 1000): string {
	if (seconds == null) return "";
	const d = Math.max(0, now - seconds);
	const units: [number, string][] = [
		[365 * 86400, "y"],
		[30 * 86400, "mo"],
		[7 * 86400, "w"],
		[86400, "d"],
		[3600, "h"],
		[60, "min"],
	];
	for (const [s, label] of units) if (d >= s) return `${Math.floor(d / s)}${label} ago`;
	return "just now";
}

/** The saved scan preferences. Folders without an atlas.yaml are included until the user turns them off: most people
 * have none, and a scan that hides them looks empty. */
export function parsePrefs(raw: string | null): { roots: string[]; candidates: boolean } {
	try {
		const v = JSON.parse(raw ?? "{}");
		return {
			roots: Array.isArray(v.roots) ? v.roots.filter((r: unknown) => typeof r === "string") : [],
			candidates: v.candidates !== false,
		};
	} catch {
		return { roots: [], candidates: true };
	}
}
