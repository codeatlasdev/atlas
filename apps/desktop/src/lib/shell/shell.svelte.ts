/**
 * Shell state that is not a dock: spaces (one per repo), the direction of the last space
 * switch (drives the content slide) and back/forward history over (worktree, tab) visits.
 */
import { colorScheme } from "@codeatlasdev/ui";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { accentOf, directionOf, neighbour, spreadHues, tintOf } from "#lib/shell/space.ts";
import { hueFor } from "#lib/shell/identity.ts";
import { emptyHistory, step, visit, type History, type Visit } from "#lib/shell/history.ts";

class Shell {
	/** 1: the new space enters from the right; -1: from the left. Read by the content wrapper. */
	direction = $state<1 | -1>(1);
	/** Bumped on every space switch so the content wrapper can re-key its transition. */
	epoch = $state(0);
	history = $state<History>(emptyHistory());
	/** Entry a back/forward step is landing on; the next recorded visit for it is not new history. */
	#arriving: Visit | null = null;

	/** Repos in sidebar order: the spaces. */
	spaces = $derived(store.sorted.filter((p) => !p.parent));
	spaceIds = $derived(this.spaces.map((s) => s.id));
	/** Spread derived hues, overridden by the user's color choice (`ProjectView.identity.hue`). */
	hues = $derived.by(() => {
		const spread = spreadHues(this.spaceIds);
		return Object.fromEntries(this.spaces.map((s) => [s.id, hueFor(s.id, s.identity, spread[s.id])]));
	});

	/** Repo id of the selected worktree (the active space). */
	activeId = $derived.by(() => {
		const cur = store.current;
		return cur ? (cur.parent ?? cur.id) : null;
	});
	active = $derived(this.spaces.find((s) => s.id === this.activeId) ?? null);

	get dark() {
		return colorScheme.resolved === "dark";
	}

	accent(spaceId: string) {
		return accentOf(spaceId, this.dark, this.hues[spaceId]);
	}
	tint(spaceId: string) {
		return tintOf(spaceId, this.dark, this.hues[spaceId]);
	}

	/** Worktree to land on when entering a space: the one last used there, else its main checkout. */
	#landing(spaceId: string): string {
		const kids = new Set([spaceId, ...store.projects.filter((p) => p.parent === spaceId).map((p) => p.id)]);
		const last = ws.doc.recentWorktrees.find((id) => kids.has(id) && ws.worktree(id));
		return last ?? spaceId;
	}

	/** Switch to a space (animated in the direction of its position). */
	switchTo(spaceId: string) {
		if (spaceId === this.activeId) {
			ws.view = "worktree";
			return;
		}
		this.direction = directionOf(this.spaceIds, this.activeId, spaceId);
		this.epoch++;
		this.select(this.#landing(spaceId));
	}

	/** Previous / next space, wrapping. */
	cycleSpace(delta: 1 | -1) {
		const to = neighbour(this.spaceIds, this.activeId, delta);
		if (to) this.switchTo(to);
	}

	/** Space `n` (1-based) of the sidebar. */
	spaceAt(n: number) {
		const s = this.spaces[n - 1];
		if (s) this.switchTo(s.id);
	}

	select(worktreeId: string) {
		ws.view = "worktree";
		store.selected = worktreeId;
		ws.select(worktreeId);
	}

	/** Record where the user is now. The arrival of a back/forward step is not a new visit. */
	note(v: Visit) {
		const target = this.#arriving;
		if (target) {
			this.#arriving = null;
			if (target.worktreeId === v.worktreeId) return;
		}
		this.history = visit(this.history, v);
	}

	#alive = (v: Visit) => !!ws.worktree(v.worktreeId) && (!v.tabId || !!ws.tab(v.tabId));

	canGo(delta: 1 | -1) {
		return step(this.history, delta, this.#alive) !== null;
	}

	/** ⌘[ / ⌘]: move through visited (worktree, tab) pairs, skipping the ones that no longer exist. */
	/** Where Back / Forward would land, for the arrow tooltips. */
	target(delta: 1 | -1): Visit | null {
		return step(this.history, delta, this.#alive)?.to ?? null;
	}

	go(delta: 1 | -1) {
		const r = step(this.history, delta, this.#alive);
		if (!r) return;
		// `history.at` already points at the target, so the visit the layout effect records next is a no-op.
		this.history = r.history;
		const tabNow = ws.selected ? ws.focusedGroup(ws.selected.id).activeTab : null;
		const unchanged = ws.selected?.id === r.to.worktreeId && (!r.to.tabId || r.to.tabId === tabNow);
		this.#arriving = unchanged ? null : r.to;
		const fromSpace = this.activeId;
		const w = ws.worktree(r.to.worktreeId);
		if (w && w.repoId !== fromSpace) {
			this.direction = directionOf(this.spaceIds, fromSpace, w.repoId);
			this.epoch++;
		}
		this.select(r.to.worktreeId);
		if (r.to.tabId && ws.tab(r.to.tabId)) ws.activate(r.to.tabId);
	}
}

export const shell = new Shell();
