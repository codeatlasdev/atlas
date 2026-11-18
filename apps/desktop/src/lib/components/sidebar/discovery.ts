/** Pure helpers behind the "Find projects" dialog: grouping, filtering, relink suggestions. */
import type { DiscoverDone } from "#lib/api.ts";
import { formatNumber, relativeTime } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { STACK_MESSAGES } from "#lib/onboarding/flow.ts";

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

/** Everything a user can tick: folders with an atlas.yaml and, when the scan includes them, folders without one (their
 * stack is detected on add). Not already in the sidebar. */
export function pickable(found: Found[], registered: ReadonlySet<string>): Found[] {
	return found.filter((f) => !registered.has(f.path));
}

/** Ticked by default: a valid atlas.yaml. Folders without one are the user's explicit choice (the Add button on their
 * row, or a tick), and a broken config never is. */
export const tickedByDefault = (f: Found): boolean => f.has_config && !f.error;

/**
 * The empty line above the list. With no atlas.yaml anywhere, a plain "No projects found" would sit right over a full
 * "Could use Atlas" list: say what is true instead.
 */
export function noProjectsLine(query: string, candidates: number): string {
	if (query) return m.sidebar_discover_dialog_no_match({ query });
	return candidates ? m.sidebar_discover_dialog_no_config() : m.sidebar_discover_dialog_none();
}

/** A scan's time in seconds with one decimal, in the digits of the language. */
const seconds = (ms: number) => formatNumber(ms / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 });

/** The status line while the scan runs. */
export function scanningLine(dirs: number, elapsedMs: number, found: number): string {
	return m.sidebar_discover_dialog_scanning({
		count: dirs,
		amount: formatNumber(dirs),
		seconds: seconds(elapsedMs),
		found: formatNumber(found),
	});
}

/** The status line of a finished (or cancelled) scan, without the unreadable count. */
export function scannedLine(done: Pick<DiscoverDone, "dirs" | "elapsed_ms" | "cancelled">): string {
	const inputs = { count: done.dirs, amount: formatNumber(done.dirs), seconds: seconds(done.elapsed_ms) };
	return done.cancelled ? m.sidebar_discover_dialog_cancelled(inputs) : m.sidebar_discover_dialog_scanned(inputs);
}

/** The tag of a found folder's stack: the label of the id `stack_of` sent, or the id itself when this app does not know it. */
export function stackLabel(id: string): string {
	return Object.hasOwn(STACK_MESSAGES, id) ? STACK_MESSAGES[id]!() : id;
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

/** "now", "3 days ago"… for the last-modified column (unix seconds, empty when unknown). */
export function ago(seconds: number | null, now = Date.now() / 1000): string {
	return seconds == null ? "" : relativeTime(seconds * 1000, now * 1000);
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
