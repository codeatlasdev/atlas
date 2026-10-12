import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
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
	type ClosedTab,
} from "#lib/stores/tabs.ts";

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

interface Doc {
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

const RECENT_TABS = 6;
const FEED_MAX = 500;
const SAVE_DEBOUNCE_MS = 300;

export const uid = () => crypto.randomUUID();

function emptyDoc(): Doc {
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

/** Leaf groups of a layout, in visual order. */
export function leaves(l: Layout): string[] {
	return l.type === "leaf" ? [l.group] : [...leaves(l.first), ...leaves(l.second)];
}

function mapLayout(l: Layout, f: (leaf: { type: "leaf"; group: string }) => Layout | null): Layout | null {
	if (l.type === "leaf") return f(l);
	const a = mapLayout(l.first, f);
	const b = mapLayout(l.second, f);
	if (!a) return b;
	if (!b) return a;
	return { ...l, first: a, second: b };
}

class Workspace {
	doc = $state<Doc>(emptyDoc());
	loaded = $state(false);
	/** Live pty ids reported by hostd (session restore). */
	ptys = $state<Record<string, PtySummary>>({});
	/** Live titles per pty. */
	titles = $state<Record<string, string>>({});
	exited = $state<Record<string, number | null>>({});
	hostError = $state<string | null>(null);
	/** Main area: the selected worktree, or a global agents view. */
	view = $state<"worktree" | "agents" | "dashboard">("worktree");
	private saveTimer: number | undefined;
	private unlisten: UnlistenFn[] = [];

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

	async init() {
		this.unlisten.push(
			await listen<{ ev: string; pty?: string; title?: string; code?: number | null }>(
				"host://event",
				(e) => {
					const p = e.payload;
					if (p.ev === "pty_title" && p.pty) this.titles[p.pty] = p.title ?? "";
					if (p.ev === "pty_exit" && p.pty) this.exited[p.pty] = p.code ?? null;
				},
			),
			await listen<string>("host://error", (e) => (this.hostError = e.payload)),
			await listen("host://reconnected", () => {
				this.hostError = null;
				void this.syncPtys();
			}),
		);
		const raw = await invoke<Doc | null>("workspace_load");
		this.doc = { ...emptyDoc(), ...(raw ?? {}) };
		this.loaded = true;
		await this.syncPtys();
		$effect.root(() => {
			$effect(() => {
				const snapshot = JSON.stringify(this.doc);
				clearTimeout(this.saveTimer);
				this.saveTimer = window.setTimeout(() => {
					invoke("workspace_save", { doc: JSON.parse(snapshot) }).catch((e) => toast.error(errText(e)));
				}, SAVE_DEBOUNCE_MS);
			});
		});
	}

	dispose() {
		this.unlisten.forEach((u) => u());
		this.unlisten = [];
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
		} catch (e) {
			this.hostError = errText(e);
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
		const l = this.doc.layouts[g.worktreeId];
		this.doc.layouts[g.worktreeId] = mapLayout(l, (leaf) =>
			leaf.group === groupId
				? { type: "split", dir, first: leaf, second: { type: "leaf", group: ng.id }, ratio: 0.5 }
				: leaf,
		)!;
		if (moveTab) this.moveTab(moveTab, ng.id);
		this.focusGroup(ng.id);
		return ng;
	}

	setRatio(path: number[], worktreeId: string, ratio: number) {
		let l = this.doc.layouts[worktreeId];
		for (const step of path) {
			if (l.type !== "split") return;
			l = step === 0 ? l.first : l.second;
		}
		if (l.type === "split") l.ratio = Math.min(0.9, Math.max(0.1, ratio));
	}

	private dropGroupIfEmpty(groupId: string) {
		const g = this.group(groupId);
		if (!g || g.order.length) return;
		// The bottom dock keeps its (possibly empty) group.
		if (this.doc.bottomGroup[g.worktreeId] === groupId) return;
		const l = this.doc.layouts[g.worktreeId];
		if (!l || leaves(l).length <= 1) return;
		this.doc.layouts[g.worktreeId] = mapLayout(l, (leaf) => (leaf.group === groupId ? null : leaf))!;
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
			const { argv, env, label, agent, chat } = c;
			t = await this.newTerminal(worktreeId, { argv, env, label, agent, chat, groupId });
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
		} = {},
	): Promise<Tab | undefined> {
		const w = this.worktree(worktreeId);
		if (!w) return;
		const tabId = opts.tabId ?? uid();
		try {
			const { pty } = await host.ptyCreate({
				cwd: w.path,
				argv: opts.argv ?? [],
				env: { ATLAS_TAB_ID: tabId, ATLAS_WORKTREE_ID: w.id, ...(opts.env ?? {}) },
				scope: { worktree_id: w.id, tab_id: tabId, pane_key: tabId },
				mode: opts.chat ? "pipe" : "pty",
			});
			this.ptys[pty] = {
				pty,
				cwd: w.path,
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
					cwd: w.path,
					env: opts.env,
					agent: opts.agent,
					chat: opts.chat,
				},
				opts.groupId,
			);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	/** Recreate the pty of an ended terminal tab with its original argv/cwd. */
	async restart(tabId: string) {
		const t = this.tab(tabId);
		if (!t || t.kind !== "terminal") return;
		try {
			if (t.pty) await host.ptyKill(t.pty).catch(() => {});
			const { pty } = await host.ptyCreate({
				cwd: t.cwd ?? this.worktree(t.worktreeId)?.path ?? "/",
				argv: t.argv ?? [],
				env: { ATLAS_TAB_ID: t.id, ATLAS_WORKTREE_ID: t.worktreeId, ...(t.env ?? {}) },
				scope: { worktree_id: t.worktreeId, tab_id: t.id, pane_key: t.id },
				mode: t.chat ? "pipe" : "pty",
			});
			await this.syncPtys();
			t.pty = pty;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	pushFeed(e: FeedEntry) {
		this.doc.feed = [e, ...this.doc.feed].slice(0, FEED_MAX);
	}
}

export const ws = new Workspace();
