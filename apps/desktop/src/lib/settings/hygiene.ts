/**
 * Types and pure helpers of the Hygiene panel (Settings > Workspaces). The shapes are the REAL serialization of
 * `atlas_core::hygiene::Report` (`__fixtures__/hygiene-report.json`, written by a Rust test), snake_case included.
 */

export type HygieneKind = "terminal" | "service" | "agent" | "browser" | "other";

export interface HygieneOrphan {
	pid: number;
	start: number;
	name: string;
	session: string;
	kind: HygieneKind | null;
	label: string | null;
	project: string | null;
}

export interface HygieneProject {
	root: string;
	name: string;
	target: number;
	node_modules: number;
	logs: number;
	/** `[name, bytes]` of each worktree under the project's `.atlas/worktrees`. */
	worktrees: [string, number][];
	/** `target/` and `node_modules` Spotlight indexes (macOS): Fix adds a `.metadata_never_index` marker. */
	indexed: string[];
}

export interface HygieneDir {
	path: string;
	kind: "temp" | "browser_profile" | "cache" | "instance";
	bytes: number;
	cap_bytes: number;
	orphaned: boolean;
}

export interface HygieneFreed {
	dirs_removed: number;
	entries_removed: number;
	bytes: number;
}

export interface HygieneReport {
	orphans: HygieneOrphan[];
	live: number;
	projects: HygieneProject[];
	dirs: HygieneDir[];
	freeable: HygieneFreed;
}

export interface HygieneFixed {
	reaped: number;
	survivors: number[];
	freed: HygieneFreed;
	marked: string[];
}

/** `1.5 GB`, `340 MB`, `12 KB`, `0 B`: binary units, one decimal from MB up. */
export function bytes(n: number): string {
	if (n < 1024) return `${n} B`;
	const units = ["KB", "MB", "GB", "TB"];
	let v = n / 1024;
	let i = 0;
	while (v >= 1024 && i < units.length - 1) {
		v /= 1024;
		i++;
	}
	return i === 0 ? `${Math.round(v)} KB` : `${v.toFixed(1)} ${units[i]}`;
}

/** Disk a project's heavy dirs hold, worktrees included. */
export function projectTotal(p: HygieneProject): number {
	return p.target + p.node_modules + p.logs + p.worktrees.reduce((a, [, b]) => a + b, 0);
}

/** Projects biggest first: the ones worth looking at lead. */
export function byDisk(projects: readonly HygieneProject[]): HygieneProject[] {
	return [...projects].sort((a, b) => projectTotal(b) - projectTotal(a));
}

/** Directories Spotlight indexes, across projects. */
export function indexedDirs(r: HygieneReport): string[] {
	return r.projects.flatMap((p) => p.indexed);
}

/** What Fix would do, in one line; `null` when there is nothing to do. */
export function fixSummary(r: HygieneReport): string | null {
	const parts: string[] = [];
	if (r.orphans.length)
		parts.push(`end ${r.orphans.length} orphaned process${r.orphans.length === 1 ? "" : "es"}`);
	if (r.freeable.bytes > 0) parts.push(`free ${bytes(r.freeable.bytes)}`);
	const idx = indexedDirs(r).length;
	if (idx) parts.push(`keep Spotlight out of ${idx} folder${idx === 1 ? "" : "s"}`);
	return parts.length ? parts.join(", ") : null;
}

/** What a Fix did, for the toast. */
export function fixedText(f: HygieneFixed): string {
	const parts = [`Ended ${f.reaped} process${f.reaped === 1 ? "" : "es"}`, `freed ${bytes(f.freed.bytes)}`];
	if (f.marked.length)
		parts.push(`marked ${f.marked.length} folder${f.marked.length === 1 ? "" : "s"} never-index`);
	if (f.survivors.length) parts.push(`${f.survivors.length} still running (stuck in the kernel)`);
	return parts.join(", ");
}
