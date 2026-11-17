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
