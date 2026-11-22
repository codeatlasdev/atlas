/** Workspace document model: worktrees, tabs, groups, layouts and the persisted `Doc`. */

import type { ClosedTab } from "#lib/stores/tabs.ts";

export type TabKind =
	"terminal" | "editor" | "diff" | "browser" | "services" | "logs" | "config" | "pr" | "dashboard";

export interface Tab {
	id: string;
	worktreeId: string;
	groupId: string;
	kind: TabKind;
	label: string;
	/** Custom label set by Rename; wins over the terminal title. */
	customLabel?: boolean;
	pty?: string;
	/** argv/cwd the pty was created with (Restart after the process ended). */
	argv?: string[];
	cwd?: string;
	env?: Record<string, string>;
	path?: string;
	url?: string;
	agent?: string;
	prNumber?: number;
	/** Pipe-mode agent (omp rpc-ui): rendered as native chat. */
	chat?: boolean;
	/** Chat tab currently showing the raw JSONL terminal instead. */
	rawView?: boolean;
	/** Unsent composer text (chat tabs). */
	draft?: string;
	/** First prompt of a chat tab, sent once omp reports `ready`. */
	pendingPrompt?: string;
	createdAt: number;
}

export interface TabGroup {
	id: string;
	worktreeId: string;
	activeTab: string | null;
	order: string[];
}

export type Layout =
	| { type: "leaf"; group: string }
	| { type: "split"; dir: "h" | "v"; first: Layout; second: Layout; ratio: number };

export interface Worktree {
	id: string;
	repoId: string;
	path: string;
	branch: string;
	baseRef: string;
	name: string;
	isMain: boolean;
	archived: boolean;
	pinned: boolean;
	unread: boolean;
	agent?: string;
	portOffset: number;
	createdAt: number;
	lastActivityAt: number;
	linkedPr?: number;
	/** `local` or `ssh:<alias>`. */
	hostId?: string;
	/** A plain folder (`ProjectView.kind === "folder"`): no Panel tab, no worktree crumb, explorer and editor only. */
	plain?: boolean;
	comment?: string;
}

export interface DiffComment {
	id: string;
	path: string;
	line: number;
	side: "old" | "new";
	text: string;
	resolved: boolean;
	sent?: boolean;
	createdAt: number;
}

export interface FeedEntry {
	ts: number;
	worktreeId: string;
	tabId: string;
	agent: string;
	state: string;
	text?: string;
}

export interface Doc {
	version: 1;
	worktrees: Worktree[];
	tabs: Tab[];
	groups: TabGroup[];
	layouts: Record<string, Layout>;
	selected: string | null;
	focusedGroup: Record<string, string>;
	recentTabs: Record<string, string[]>;
	recentWorktrees: string[];
	diffComments: Record<string, DiffComment[]>;
	feed: FeedEntry[];
	urlHistory: Record<string, string[]>;
	/** Per worktree: the tab group shown in the bottom dock (outside the split tree). */
	bottomGroup: Record<string, string>;
	/** Per worktree: recently closed tabs, newest last (⌘⇧T). */
	closed: Record<string, ClosedTab[]>;
}

export function emptyDoc(): Doc {
	return {
		version: 1,
		worktrees: [],
		tabs: [],
		groups: [],
		layouts: {},
		selected: null,
		focusedGroup: {},
		recentTabs: {},
		recentWorktrees: [],
		diffComments: {},
		feed: [],
		urlHistory: {},
		bottomGroup: {},
		closed: {},
	};
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
	typeof v === "object" && v !== null && !Array.isArray(v);

/** Top-level keys that hold a list of objects (each with a string `id`, when `id`), a list of strings, a record of
 * objects, of strings or of lists. Enough to tell a workspace from any other JSON; the rest is read leniently. */
const LISTS_OF_OBJECTS = { worktrees: true, tabs: true, groups: true, feed: false } as const;
const LISTS_OF_STRINGS = ["recentWorktrees"] as const;
const RECORDS_OF = {
	layouts: "object",
	focusedGroup: "string",
	bottomGroup: "string",
	recentTabs: "list",
	diffComments: "list",
	urlHistory: "list",
	closed: "list",
} as const;

/**
 * Why `raw` (the parsed workspace.json) is not a workspace, or null when it can be used. `null`/absent is "no
 * document". Valid JSON of the wrong shape (a hand edit, another tool's file, a newer schema) otherwise loads as a
 * half-document and crashes the first screen that reads a tab list; the caller sets the file aside instead.
 */
export function docShapeError(raw: unknown): string | null {
	if (raw === null || raw === undefined) return null;
	if (!isRecord(raw)) return "expected a JSON object";
	if ("version" in raw && typeof raw.version !== "number") return "version: expected a number";
	if ("selected" in raw && raw.selected !== null && typeof raw.selected !== "string")
		return "selected: expected a string or null";
	for (const [key, needsId] of Object.entries(LISTS_OF_OBJECTS)) {
		if (!(key in raw)) continue;
		const v = raw[key];
		if (!Array.isArray(v)) return `${key}: expected a list`;
		if (v.some((x) => !isRecord(x) || (needsId && typeof x.id !== "string")))
			return `${key}: expected a list of objects${needsId ? " with an id" : ""}`;
	}
	for (const key of LISTS_OF_STRINGS) {
		if (
			key in raw &&
			(!Array.isArray(raw[key]) || (raw[key] as unknown[]).some((x) => typeof x !== "string"))
		)
			return `${key}: expected a list of strings`;
	}
	for (const [key, of] of Object.entries(RECORDS_OF)) {
		if (!(key in raw)) continue;
		const v = raw[key];
		if (!isRecord(v)) return `${key}: expected an object`;
		const bad = Object.values(v).some((x) =>
			of === "list" ? !Array.isArray(x) : of === "string" ? typeof x !== "string" : !isRecord(x),
		);
		if (bad)
			return `${key}: expected an object of ${of === "list" ? "lists" : of === "string" ? "strings" : "objects"}`;
	}
	return null;
}
