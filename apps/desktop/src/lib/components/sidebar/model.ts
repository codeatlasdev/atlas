/** Pure sidebar rules: which worktrees pass the filter, child ordering, strongest agent state, row labels. */

import { m } from "#lib/i18n/m.ts";
import type { AgentState } from "#lib/stores/agents.svelte.ts";

/** What the rules need to know about a project row. */
interface Row {
	id: string;
	name: string;
	root: string;
	branch?: string | null;
	parent?: string | null;
	pinned: boolean;
}

/** Worktree facts the filter looks at. */
interface Wt {
	name: string;
	archived: boolean;
}

/** Linked worktrees of `repoId`, pinned first (stable otherwise). `sorted` is the store's order. */
export function childrenOf<P extends Row>(sorted: P[], repoId: string): P[] {
	return sorted.filter((p) => p.parent === repoId).sort((a, b) => Number(b.pinned) - Number(a.pinned));
}

/** Does the row survive the filter text and the "show archived" toggle? */
export function matches(p: Row, w: Wt | undefined, filter: string, showArchived: boolean): boolean {
	if (w?.archived && !showArchived) return false;
	const q = filter.trim().toLowerCase();
	return !q || [p.name, p.branch ?? "", w?.name ?? "", p.root].some((x) => x.toLowerCase().includes(q));
}

const GLYPH_ORDER: AgentState[] = ["blocked", "waiting", "working", "done", "idle"];

/** Strongest agent state among a set of states, or null when there are none. */
export function strongest(states: AgentState[]): AgentState | null {
	return GLYPH_ORDER.find((s) => states.includes(s)) ?? null;
}

/**
 * The progress step of a worktree being created (`worktree://progress`, `atlas-desktop/src/worktree/create.rs`), as the
 * line under its name. A step the list does not know shows as it came.
 */
export function stepLabel(step: string): string {
	switch (step) {
		case "fetch":
			return m.sidebar_step_fetch();
		case "add":
			return m.sidebar_step_add();
		case "shared":
			return m.sidebar_step_shared();
		case "seed":
			return m.sidebar_step_seed();
		default:
			return step;
	}
}

/** `state` of a pull request as `gh pr view --json state` reports it. A state this list does not know shows lower-cased. */
function prStateWord(state: string): string {
	switch (state) {
		case "OPEN":
			return m.sidebar_worktree_row_pr_open();
		case "MERGED":
			return m.sidebar_worktree_row_pr_merged();
		case "CLOSED":
			return m.sidebar_worktree_row_pr_closed();
		default:
			return state.toLowerCase();
	}
}

/** The chip of a linked pull request: its number, and its state once the view has loaded. */
export function prChip(number: number, state: string | undefined): string {
	return state === undefined
		? m.sidebar_worktree_row_pr({ number })
		: m.sidebar_worktree_row_pr_state({ number, state: prStateWord(state) });
}
