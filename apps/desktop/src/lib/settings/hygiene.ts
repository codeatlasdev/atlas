/**
 * Types and pure helpers of the Hygiene panel (Settings > Workspaces). The shapes are the REAL serialization of
 * `atlas_core::hygiene::Report` (`__fixtures__/hygiene-report.json`, written by a Rust test), snake_case included.
 */
import { formatNumber, list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

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

/** `1.5 GB`, `340 MB`, `12 KB`, `0 B`: binary units, one decimal from MB up, the decimal mark of the language. */
export function bytes(n: number): string {
	if (n < 1024) return `${formatNumber(n, { useGrouping: false })} B`;
	const units = ["KB", "MB", "GB", "TB"];
	let v = n / 1024;
	let i = 0;
	while (v >= 1024 && i < units.length - 1) {
		v /= 1024;
		i++;
	}
	return i === 0
		? `${formatNumber(Math.round(v), { useGrouping: false })} KB`
		: `${formatNumber(v, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} ${units[i]}`;
}

/** The kind of an orphaned process, as `atlas_core::hygiene` names it. */
export function kindText(kind: string): string {
	switch (kind) {
		case "terminal":
			return m.settings_general_hygiene_kind_terminal();
		case "service":
			return m.settings_general_hygiene_kind_service();
		case "agent":
			return m.settings_general_hygiene_kind_agent();
		case "browser":
			return m.settings_general_hygiene_kind_browser();
		case "other":
			return m.settings_general_hygiene_kind_other();
		default:
			return kind;
	}
}

/** The kind of a folder the hygiene report tracks, as `atlas_core::hygiene` names it. */
export function dirKindText(kind: string): string {
	switch (kind) {
		case "temp":
			return m.settings_general_hygiene_dir_temp();
		case "browser_profile":
			return m.settings_general_hygiene_dir_browser_profile();
		case "cache":
			return m.settings_general_hygiene_dir_cache();
		case "instance":
			return m.settings_general_hygiene_dir_instance();
		default:
			return kind;
	}
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

/** What Fix would do, as a list of actions ("end 2 orphaned processes, free 3 MB"); `null` when there is nothing to do. */
export function fixSummary(r: HygieneReport): string | null {
	const parts: string[] = [];
	if (r.orphans.length) parts.push(m.settings_general_hygiene_todo_end({ count: r.orphans.length }));
	if (r.freeable.bytes > 0)
		parts.push(m.settings_general_hygiene_todo_free({ size: bytes(r.freeable.bytes) }));
	const idx = indexedDirs(r).length;
	if (idx) parts.push(m.settings_general_hygiene_todo_spotlight({ count: idx }));
	return parts.length ? list(parts) : null;
}

/** What a Fix did, for the toast. */
export function fixedText(f: HygieneFixed): string {
	const parts = [
		m.settings_general_hygiene_done_ended({ count: f.reaped }),
		m.settings_general_hygiene_done_freed({ size: bytes(f.freed.bytes) }),
	];
	if (f.marked.length) parts.push(m.settings_general_hygiene_done_marked({ count: f.marked.length }));
	if (f.survivors.length)
		parts.push(m.settings_general_hygiene_done_survivors({ count: f.survivors.length }));
	return list(parts, "unit");
}
