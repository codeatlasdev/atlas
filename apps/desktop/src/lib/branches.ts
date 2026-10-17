/**
 * Branch picker model: parsing of what `git_branches` / `git_switch` return (written by
 * `crates/atlas-desktop/src/git_branch.rs`, whose serialization is the fixture the tests read) and the rows the
 * picker shows. Pure: no Svelte, no IPC.
 */
import { fuzzy } from "#lib/fuzzy.ts";

export interface BranchInfo {
	/** `feat/x` or `origin/feat/x`. */
	name: string;
	kind: "local" | "remote";
	current: boolean;
	/** Committer date of the tip, ISO 8601. */
	date: string;
	/** Checked out by another worktree of the repo. */
	worktree: { path: string } | null;
	/** Remote branch that already has a local branch of the same name. */
	local: string | null;
}
export interface Branches {
	current: string | null;
	branches: BranchInfo[];
}
export type Switched =
	| { status: "ok"; branch: string; stashed: boolean }
	| { status: "dirty"; message: string }
	| { status: "worktree"; path: string };

export interface BranchRow {
	id: string;
	name: string;
	label: string;
	description: string;
	current: boolean;
	/** Path of the worktree that owns the branch: selecting jumps there instead of switching. */
	jump?: string;
	/** `local` | `remote` | `create`. */
	kind: "local" | "remote" | "create";
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;

/** Validate the Rust payload; a shape drift throws here instead of rendering an empty picker. */
export function parseBranches(v: unknown): Branches {
	if (!isObj(v) || !Array.isArray(v.branches)) throw new Error("git_branches: not a branch list");
	const branches = v.branches.map((b): BranchInfo => {
		if (!isObj(b) || typeof b.name !== "string" || (b.kind !== "local" && b.kind !== "remote"))
			throw new Error(`git_branches: bad branch ${JSON.stringify(b)}`);
		return {
			name: b.name,
			kind: b.kind,
			current: b.current === true,
			date: typeof b.date === "string" ? b.date : "",
			worktree: isObj(b.worktree) && typeof b.worktree.path === "string" ? { path: b.worktree.path } : null,
			local: typeof b.local === "string" ? b.local : null,
		};
	});
	return { current: typeof v.current === "string" ? v.current : null, branches };
}

export function parseSwitched(v: unknown): Switched {
	if (isObj(v)) {
		if (v.status === "ok" && typeof v.branch === "string")
			return { status: "ok", branch: v.branch, stashed: v.stashed === true };
		if (v.status === "dirty" && typeof v.message === "string") return { status: "dirty", message: v.message };
		if (v.status === "worktree" && typeof v.path === "string") return { status: "worktree", path: v.path };
	}
	throw new Error(`git_switch: unexpected result ${JSON.stringify(v)}`);
}

/** `now`, `30m ago`, `5h ago`, `3d ago`, `3mo ago`, `3y ago`; empty for an unreadable date. */
export function relDate(iso: string, now = Date.now()): string {
	const t = Date.parse(iso);
	if (Number.isNaN(t)) return "";
	const s = Math.max(0, (now - t) / 1000);
	if (s < 60) return "now";
	const steps: [number, string][] = [
		[60, "m"],
		[3600, "h"],
		[86_400, "d"],
		[2_592_000, "mo"],
		[31_536_000, "y"],
	];
	let out = "";
	for (const [size, unit] of steps) if (s >= size) out = `${Math.floor(s / size)}${unit} ago`;
	return out;
}

/**
 * Rows for `query`: Rust's order (current, recent locals, remotes) with no query; with one, a fuzzy match over every
 * name, best first. A remote with a local twin is dropped while typing so one branch is not offered twice.
 */
export function branchRows(b: Branches, query: string, opts: { now?: number } = {}): BranchRow[] {
	const q = query.trim();
	const rows = b.branches
		.filter((x) => !(q && x.kind === "remote" && x.local))
		.map((x) => {
			const parts: string[] = [];
			if (x.current) parts.push("current");
			if (x.worktree)
				parts.push(`checked out in ${x.worktree.path.split("/").filter(Boolean).pop() ?? x.worktree.path}`);
			const age = relDate(x.date, opts.now);
			if (age) parts.push(age);
			return {
				row: {
					id: `${x.kind}:${x.name}`,
					name: x.name,
					label: x.name,
					description: parts.join(" · "),
					current: x.current,
					jump: x.worktree?.path,
					kind: x.kind,
				} satisfies BranchRow,
				score: q ? fuzzy(q, x.name) : 1,
			};
		})
		.filter((x) => x.score > 0);
	if (q) rows.sort((a, c) => c.score - a.score);
	return rows.map((x) => x.row);
}

/** Names git would reject (`git check-ref-format --branch`), the ones that matter while typing. */
function validName(n: string): boolean {
	return (
		!!n &&
		!n.startsWith("-") &&
		!n.startsWith("/") &&
		!n.endsWith("/") &&
		!n.endsWith(".") &&
		!n.endsWith(".lock") &&
		!n.includes("..") &&
		!n.includes("//") &&
		!n.includes("@{") &&
		!/[\s~^:?*[\\\x00-\x1f\x7f]/.test(n)
	);
}

/** "Create branch <q>" from the current HEAD; null when the name is unusable or already a branch. */
export function createRow(b: Branches, query: string, from: string | null): BranchRow | null {
	const n = query.trim();
	if (!validName(n) || b.branches.some((x) => x.name === n)) return null;
	return {
		id: `create:${n}`,
		name: n,
		label: `Create branch “${n}”`,
		description: from ? `from ${from}` : "from the current commit",
		current: false,
		kind: "create",
	};
}
