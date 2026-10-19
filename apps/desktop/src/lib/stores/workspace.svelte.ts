import { invoke } from "@tauri-apps/api/core";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { errors } from "#lib/errors/errors.svelte.ts";
import { classify } from "#lib/errors/diagnostics.ts";
import { host, type PtySummary } from "#lib/host.ts";
import {
	closable,
	firstMovable,
	mruNext,
	pinned,
	pinnedFirst,
	pushClosed,
	snapshot,
	tabAt,
} from "#lib/stores/tabs.ts";
import { clampRatio, leaves, removeLeaf, splitAt, splitLeaf } from "#lib/stores/layout.ts";
import { rightNeighbor } from "#lib/stores/preview.ts";
import { ptyRequest } from "#lib/stores/ptySpawn.ts";
import { WorkspacePersist } from "#lib/stores/workspacePersist.svelte.ts";
import {
	emptyDoc,
	type Doc,
	type FeedEntry,
	type Layout,
	type Tab,
	type TabGroup,
	type TabKind,
	type Worktree,
} from "#lib/stores/workspaceTypes.ts";

export type { DiffComment, Layout, Tab, Worktree } from "#lib/stores/workspaceTypes.ts";
export { leaves } from "#lib/stores/layout.ts";

const RECENT_TABS = 6;
const FEED_MAX = 500;

export const uid = () => crypto.randomUUID();

class Workspace {
	doc = $state<Doc>(emptyDoc());
	loaded = $state(false);
	/** Live pty ids reported by hostd (session restore). */
	ptys = $state<Record<string, PtySummary>>({});
	/** Live titles per pty. */
	titles = $state<Record<string, string>>({});
	exited = $state<Record<string, number | null>>({});
	hostError = $state<string | null>(null);
	/** The terminal host did not answer the last `syncPtys`: `ptys` is what we knew, not what is true. */
	hostDown = $state(false);
	/** Main area: the selected worktree, or a global agents view. */
	view = $state<"worktree" | "agents" | "dashboard">("worktree");
	#persist = new WorkspacePersist(this);

	get selected(): Worktree | undefined {
		return this.doc.worktrees.find((w) => w.id === this.doc.selected);
	}

	worktree(id: string | null | undefined) {
		return this.doc.worktrees.find((w) => w.id === id);
	}

	tab(id: string | null | undefined) {
		return this.doc.tabs.find((t) => t.id === id);
	}

	group(id: string | null | undefined) {
		return this.doc.groups.find((g) => g.id === id);
	}

	tabsOf(worktreeId: string) {
		return this.doc.tabs.filter((t) => t.worktreeId === worktreeId);
	}

	init() {
		return this.#persist.init();
	}

	dispose() {
		this.#persist.dispose();
	}

	async syncPtys() {
		try {
			const list = await host.ptyList();
			this.ptys = Object.fromEntries(list.map((p) => [p.pty, p]));
			for (const p of list) {
				this.titles[p.pty] = p.title;
				if (p.exited) this.exited[p.pty] = p.exited.code;
			}
			this.hostError = null;
			this.hostDown = false;
		} catch (e) {
			// Keep `ptys`: an unreachable host says nothing about its terminals, and replacing the map
			// would show every tab as "Terminal is gone" (1.5.0-alpha.5). Report the outage once.
			this.hostError = errText(e);
			this.hostDown = true;
			void errors.captureHost(this.hostError);
		}
	}

	/** Tab whose pty is gone from hostd, or cold (restored after a hostd restart). */
	ended(t: Tab): "gone" | "exited" | null {
		if (!t.pty) return null;
		if (!this.ptys[t.pty]) return "gone";
		if (t.pty in this.exited) return "exited";
		return null;
	}

	// ---------- worktrees ----------

	upsertWorktree(w: Worktree) {
		const i = this.doc.worktrees.findIndex((x) => x.id === w.id);
		if (i < 0) this.doc.worktrees.push(w);
		else this.doc.worktrees[i] = { ...this.doc.worktrees[i], ...w };
	}

	removeWorktree(id: string) {
		for (const t of this.tabsOf(id)) if (t.pty) void host.ptyKill(t.pty).catch(() => {});
		this.doc.tabs = this.doc.tabs.filter((t) => t.worktreeId !== id);
		this.doc.groups = this.doc.groups.filter((g) => g.worktreeId !== id);
		delete this.doc.layouts[id];
		delete this.doc.focusedGroup[id];
		delete this.doc.recentTabs[id];
		delete this.doc.closed[id];
		delete this.doc.diffComments[id];
		this.doc.worktrees = this.doc.worktrees.filter((w) => w.id !== id);
		this.doc.recentWorktrees = this.doc.recentWorktrees.filter((x) => x !== id);
		if (this.doc.selected === id) this.doc.selected = this.doc.worktrees[0]?.id ?? null;
	}

	select(id: string) {
		this.doc.selected = id;
		this.doc.recentWorktrees = [id, ...this.doc.recentWorktrees.filter((x) => x !== id)].slice(0, 20);
		const w = this.worktree(id);
		if (w && document.hasFocus()) w.unread = false;
		this.ensureLayout(id);
		this.ensurePanel(id);
	}

	// ---------- layout ----------

	ensureLayout(worktreeId: string): Layout {
		let l = this.doc.layouts[worktreeId];
		if (!l || !leaves(l).every((g) => this.group(g))) {
			const g = this.newGroup(worktreeId);
			l = { type: "leaf", group: g.id };
			this.doc.layouts[worktreeId] = l;
			this.doc.focusedGroup[worktreeId] = g.id;
		}
		return l;
	}

	private newGroup(worktreeId: string): TabGroup {
		const g: TabGroup = { id: uid(), worktreeId, activeTab: null, order: [] };
		this.doc.groups.push(g);
		return this.doc.groups[this.doc.groups.length - 1];
	}

	/** Active tab of the focused group, without creating anything: safe inside `$derived`. */
	focusedTab(worktreeId: string): Tab | undefined {
		const l = this.doc.layouts[worktreeId];
		if (!l) return undefined;
		const id = this.doc.focusedGroup[worktreeId];
		const g = this.group(
			id && (leaves(l).includes(id) || id === this.doc.bottomGroup[worktreeId]) ? id : leaves(l)[0],
		);
		return g ? this.tab(g.activeTab) : undefined;
	}

	focusedGroup(worktreeId: string): TabGroup {
		this.ensureLayout(worktreeId);
		const id = this.doc.focusedGroup[worktreeId];
		const l = this.doc.layouts[worktreeId];
		if (id && id === this.doc.bottomGroup[worktreeId] && this.group(id)) return this.group(id)!;
		return this.group(leaves(l).includes(id) ? id : leaves(l)[0])!;
	}

	focusGroup(groupId: string) {
		const g = this.group(groupId);
		if (g) this.doc.focusedGroup[g.worktreeId] = g.id;
	}

	/** The bottom dock's tab group, created on first use. */
	bottomGroup(worktreeId: string): TabGroup {
		const g = this.group(this.doc.bottomGroup[worktreeId]);
		if (g) return g;
		const ng = this.newGroup(worktreeId);
		this.doc.bottomGroup[worktreeId] = ng.id;
		return ng;
	}

	/** Focus the split tree again (the bottom dock was hidden). */
	focusTree(worktreeId: string) {
		const l = this.ensureLayout(worktreeId);
		if (!leaves(l).includes(this.doc.focusedGroup[worktreeId]))
			this.doc.focusedGroup[worktreeId] = leaves(l)[0];
	}

	/** Split `groupId`; the new group goes right (`h`) or below (`v`). Returns it. */
	split(groupId: string, dir: "h" | "v", moveTab?: string): TabGroup | undefined {
		const g = this.group(groupId);
		if (!g || this.doc.bottomGroup[g.worktreeId] === groupId) return;
		if (moveTab && pinned(this.tab(moveTab))) return;
		const ng = this.newGroup(g.worktreeId);
		this.doc.layouts[g.worktreeId] = splitLeaf(this.doc.layouts[g.worktreeId], groupId, ng.id, dir);
		if (moveTab) this.moveTab(moveTab, ng.id);
		this.focusGroup(ng.id);
		return ng;
	}

	/**
	 * The pane "to the side" of the focused one: the group right of it (never the bottom dock) when the layout has one,
	 * so repeated opens do not keep splitting; else a new group split to its right.
	 */
	sideGroup(worktreeId: string): string | undefined {
		const from = this.focusedGroup(worktreeId);
		const bottom = this.doc.bottomGroup[worktreeId];
		return (
			rightNeighbor(this.doc.layouts[worktreeId], from.id, (g) => g === bottom) ??
			this.split(from.id, "h")?.id
		);
	}

	setRatio(path: number[], worktreeId: string, ratio: number) {
		const node = splitAt(this.doc.layouts[worktreeId], path);
		if (node) node.ratio = clampRatio(ratio);
	}

	private dropGroupIfEmpty(groupId: string) {
		const g = this.group(groupId);
		if (!g || g.order.length) return;
		// The bottom dock keeps its (possibly empty) group.
		if (this.doc.bottomGroup[g.worktreeId] === groupId) return;
		const l = this.doc.layouts[g.worktreeId];
		if (!l || leaves(l).length <= 1) return;
		this.doc.layouts[g.worktreeId] = removeLeaf(l, groupId)!;
		this.doc.groups = this.doc.groups.filter((x) => x.id !== groupId);
		if (this.doc.focusedGroup[g.worktreeId] === groupId)
			this.doc.focusedGroup[g.worktreeId] = leaves(this.doc.layouts[g.worktreeId])[0];
	}

	// ---------- tabs ----------

	activate(tabId: string) {
		const t = this.tab(tabId);
		const g = this.group(t?.groupId);
		if (!t || !g) return;
		g.activeTab = t.id;
		this.focusGroup(g.id);
		const r = this.doc.recentTabs[t.worktreeId] ?? [];
		this.doc.recentTabs[t.worktreeId] = [t.id, ...r.filter((x) => x !== t.id)].slice(0, RECENT_TABS);
	}

	/**
	 * The Panel is pinned: exactly one per worktree, first tab of the first tree group.
	 * Recreated when missing, pulled back when it ended up elsewhere (old docs, bottom dock).
	 */
	ensurePanel(worktreeId: string) {
		const first = this.group(leaves(this.ensureLayout(worktreeId))[0]);
		if (!first) return;
		const panels = this.tabsOf(worktreeId).filter(pinned);
		const [keep, ...extra] = panels;
		for (const p of extra) this.closeTab(p.id, true, true);
		if (!keep) {
			this.openTab(worktreeId, { kind: "services", label: "Panel" }, first.id);
			return;
		}
		const from = this.group(keep.groupId);
		if (from && from.id !== first.id) {
			from.order = from.order.filter((x) => x !== keep.id);
			if (from.activeTab === keep.id) from.activeTab = from.order[0] ?? null;
			keep.groupId = first.id;
			first.order.unshift(keep.id);
			this.dropGroupIfEmpty(from.id);
		} else first.order = pinnedFirst(first.order, (id) => this.tab(id));
	}

	openTab(
		worktreeId: string,
		partial: Omit<Tab, "id" | "worktreeId" | "groupId" | "createdAt"> & { id?: string },
		groupId?: string,
	): Tab {
		const g = this.group(groupId) ?? this.focusedGroup(worktreeId);
		const t: Tab = { ...partial, id: partial.id ?? uid(), worktreeId, groupId: g.id, createdAt: Date.now() };
		this.doc.tabs.push(t);
		const floor = firstMovable(g.order, (id) => this.tab(id));
		const at = pinned(t)
			? 0
			: Math.max(floor, g.activeTab ? g.order.indexOf(g.activeTab) + 1 : g.order.length);
		g.order.splice(at, 0, t.id);
		this.activate(t.id);
		return this.tab(t.id)!;
	}

	/** Focus an existing tab matching `pred`, else open one. */
	reveal(worktreeId: string, pred: (t: Tab) => boolean, partial: Parameters<Workspace["openTab"]>[1]): Tab {
		const existing = this.tabsOf(worktreeId).find(pred);
		if (existing) {
			this.activate(existing.id);
			return existing;
		}
		return this.openTab(worktreeId, partial);
	}

	/** Pinned tabs stay put: they never leave their group and nothing slots in before them. */
	moveTab(tabId: string, toGroup: string, index?: number) {
		const t = this.tab(tabId);
		const from = this.group(t?.groupId);
		const to = this.group(toGroup);
		if (!t || !from || !to || pinned(t)) return;
		from.order = from.order.filter((x) => x !== tabId);
		if (from.activeTab === tabId) from.activeTab = from.order[from.order.length - 1] ?? null;
		to.order = to.order.filter((x) => x !== tabId);
		const floor = firstMovable(to.order, (id) => this.tab(id));
		const at = Math.max(floor, index === undefined ? to.order.length : Math.min(index, to.order.length));
		to.order.splice(at, 0, tabId);
		t.groupId = to.id;
		this.activate(tabId);
		if (from.id !== to.id) this.dropGroupIfEmpty(from.id);
	}

	/**
	 * Close a tab. The pinned Panel refuses (`false`); `force` is for dropping stray
	 * duplicates. A reopenable tab is remembered for ⌘⇧T.
	 */
	closeTab(tabId: string, killPty = true, force = false): boolean {
		const t = this.tab(tabId);
		if (!t) return false;
		if (pinned(t) && !force) return false;
		if (killPty && t.pty) void host.ptyKill(t.pty).catch(() => {});
		if (t.kind === "browser") void invoke("browser_close", { tab: t.id }).catch(() => {});
		const g = this.group(t.groupId);
		const i = g ? g.order.indexOf(tabId) : -1;
		const snap = snapshot(t, i);
		if (snap) this.doc.closed[t.worktreeId] = pushClosed(this.doc.closed[t.worktreeId] ?? [], snap);
		if (g) {
			g.order = g.order.filter((x) => x !== tabId);
			if (g.activeTab === tabId) g.activeTab = g.order[Math.min(i, g.order.length - 1)] ?? null;
		}
		this.doc.tabs = this.doc.tabs.filter((x) => x.id !== tabId);
		const r = this.doc.recentTabs[t.worktreeId];
		if (r) this.doc.recentTabs[t.worktreeId] = r.filter((x) => x !== tabId);
		if (g) this.dropGroupIfEmpty(g.id);
		return true;
	}

	/** Close every tab of `groupId` except `keep` and pinned ones; returns the closed ids. */
	closeMany(groupId: string, keep?: string): string[] {
		const ids = closable(this.group(groupId)?.order ?? [], (id) => this.tab(id), keep);
		for (const id of ids) this.closeTab(id);
		return ids;
	}

	/** Reopen the most recently closed tab of the worktree. Terminals restart in their cwd. */
	async reopenClosed(worktreeId: string): Promise<Tab | undefined> {
		const stack = this.doc.closed[worktreeId] ?? [];
		const c = stack[stack.length - 1];
		if (!c) return;
		this.doc.closed[worktreeId] = stack.slice(0, -1);
		const groupId = this.focusedGroup(worktreeId).id;
		let t: Tab | undefined;
		if (c.kind === "terminal") {
			const { argv, env, label, agent, chat, cwd } = c;
			t = await this.newTerminal(worktreeId, { argv, env, label, agent, chat, groupId, cwd });
		} else {
			const { index: _index, ...rest } = c;
			t = this.openTab(worktreeId, { ...rest, kind: rest.kind as TabKind }, groupId);
		}
		const g = this.group(groupId);
		if (t && g && c.index >= 0) {
			const at = Math.max(
				firstMovable(g.order, (id) => this.tab(id)),
				Math.min(c.index, g.order.length - 1),
			);
			g.order = g.order.filter((x) => x !== t.id);
			g.order.splice(at, 0, t.id);
		}
		return t;
	}

	/** ⌘N in the focused group: 1…8 by position, 9 = last. */
	activateNth(worktreeId: string, n: number) {
		const id = tabAt(this.focusedGroup(worktreeId).order, n);
		if (id) this.activate(id);
	}

	/** Tabs of the worktree, most recently used first (⌃Tab order). */
	mru(worktreeId: string): string[] {
		return (this.doc.recentTabs[worktreeId] ?? []).filter((id) => this.tab(id));
	}

	/** Activate the next/previous tab in MRU order relative to `from`. */
	mruStep(worktreeId: string, from: string | null, step: 1 | -1): string | undefined {
		const id = mruNext(this.mru(worktreeId), from, step);
		if (id) this.activate(id);
		return id;
	}

	cycle(worktreeId: string, delta: 1 | -1) {
		const g = this.focusedGroup(worktreeId);
		if (!g.order.length) return;
		const i = g.activeTab ? g.order.indexOf(g.activeTab) : 0;
		this.activate(g.order[(i + delta + g.order.length) % g.order.length]);
	}

	/** Spawn a pty in the worktree and open it as a terminal tab. */
	async newTerminal(
		worktreeId: string,
		opts: {
			tabId?: string;
			argv?: string[];
			env?: Record<string, string>;
			label?: string;
			agent?: string;
			groupId?: string;
			chat?: boolean;
			/** Start in this folder (absolute) instead of the worktree root. */
			cwd?: string;
		} = {},
	): Promise<Tab | undefined> {
		const w = this.worktree(worktreeId);
		if (!w) return;
		const cwd = opts.cwd ?? w.path;
		const tabId = opts.tabId ?? uid();
		try {
			const { pty } = await host.ptyCreate(
				ptyRequest({
					id: tabId,
					worktreeId: w.id,
					cwd,
					argv: opts.argv,
					env: opts.env,
					chat: opts.chat,
				}),
			);
			this.ptys[pty] = {
				pty,
				cwd,
				argv: opts.argv ?? [],
				scope: {},
				title: "",
				created_at: Date.now() / 1000,
				exited: null,
				pid: null,
				fg: null,
			};
			return this.openTab(
				worktreeId,
				{
					id: tabId,
					kind: "terminal",
					label: opts.label ?? "Terminal",
					pty,
					argv: opts.argv ?? [],
					cwd,
					env: opts.env,
					agent: opts.agent,
					chat: opts.chat,
				},
				opts.groupId,
			);
		} catch (e) {
			this.fail(e);
		}
	}

	/** A host failure folds into the one outage toast; anything else is an ordinary error toast. */
	fail(e: unknown) {
		const m = errText(e);
		if (classify(m) === "host") void errors.captureHost(m);
		else toast.error(m);
	}

	/** Recreate the pty of an ended terminal tab with its original argv/cwd. */
	async restart(tabId: string) {
		const t = this.tab(tabId);
		if (!t || t.kind !== "terminal") return;
		// A Restart on a host that is down can only fail the same way: say so once, change nothing.
		if (this.hostDown) {
			void errors.captureHost(this.hostError ?? "Terminal host not reachable.");
			return;
		}
		try {
			if (t.pty) await host.ptyKill(t.pty).catch(() => {});
			const { pty } = await host.ptyCreate(
				ptyRequest({ ...t, cwd: t.cwd ?? this.worktree(t.worktreeId)?.path ?? "/" }),
			);
			await this.syncPtys();
			t.pty = pty;
		} catch (e) {
			this.fail(e);
		}
	}

	pushFeed(e: FeedEntry) {
		this.doc.feed = [e, ...this.doc.feed].slice(0, FEED_MAX);
	}
}

export const ws = new Workspace();
