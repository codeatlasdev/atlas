// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, expect, mock, test } from "bun:test";

mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../test/uiKit.ts")).installUiKit();
mock.module("#lib/api.ts", () => ({ errText: String }));
let ptyList: () => Promise<unknown[]> = async () => [];
const created = [];
mock.module("#lib/host.ts", () => ({
	host: {
		ptyKill: async () => {},
		ptyList: () => ptyList(),
		ptyCreate: async (a) => (created.push(a), { pty: "p1" }),
	},
}));
// The real `errors` store (not a mock of it: a process-wide module mock would leak into
// errors/hostErrors.test.ts): its toasts are what a user would see.
// Stubs live only for this file: bun runs every test file in one process, so leaking the stub `document`
// broke every DOM suite that ran after this one (see test/dom-isolation.test.ts).
// Save descriptors, not values: once Svelte's client runtime is loaded, reading `globalThis.$state`
// hits its throwing getter (rune_outside_svelte), which broke CI when another suite loaded first.
const keys = ["document", "$state", "$effect"];
const saved = Object.fromEntries(keys.map((k) => [k, Object.getOwnPropertyDescriptor(globalThis, k)]));
const put = (k, value) => Object.defineProperty(globalThis, k, { value, configurable: true, writable: true });
put("document", { hasFocus: () => true });
// Runes are compiled by Svelte; plain objects are enough for the rules under test.
put("$state", (v) => v);
put(
	"$effect",
	Object.assign(() => {}, { root: (f) => f() }),
);
afterAll(() => {
	for (const k of keys) {
		if (saved[k]) Object.defineProperty(globalThis, k, saved[k]);
		else delete globalThis[k];
	}
});

const { ws } = await import("./workspace.svelte.ts");
const { errors } = await import("#lib/errors/errors.svelte.ts");

const W = "w1";
ws.doc.worktrees.push({ id: W, repoId: "r", path: "/tmp/w", name: "w", isMain: true });

function fresh() {
	ws.doc.tabs = [];
	ws.doc.groups = [];
	ws.doc.layouts = {};
	ws.doc.closed = {};
	ws.doc.recentTabs = {};
	ws.select(W);
}
const order = () => ws.focusedGroup(W).order.map((id) => ws.tab(id).kind);

test("select recreates the Panel as the only, first tab", () => {
	fresh();
	expect(order()).toEqual(["services"]);
	ws.select(W);
	expect(order()).toEqual(["services"]);
});

test("the Panel cannot be closed, even through close-all / close-others", () => {
	fresh();
	const panel = ws.tabsOf(W)[0];
	ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	ws.openTab(W, { kind: "logs", label: "Logs" });
	expect(ws.closeTab(panel.id)).toBe(false);
	ws.closeMany(ws.focusedGroup(W).id);
	expect(order()).toEqual(["services"]);
	ws.openTab(W, { kind: "editor", label: "b", path: "b.ts" });
	ws.closeMany(ws.focusedGroup(W).id, ws.focusedGroup(W).activeTab);
	expect(order()).toEqual(["services", "editor"]);
});

test("new tabs open after the Panel and a stray Panel is pulled back first", () => {
	fresh();
	ws.focusedGroup(W).activeTab = null;
	const e = ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	expect(order()).toEqual(["services", "editor"]);
	// Panel dragged away (old document) and a duplicate appears.
	const g2 = ws.split(ws.focusedGroup(W).id, "h");
	const panel = ws.tabsOf(W).find((t) => t.kind === "services");
	panel.groupId = g2.id;
	g2.order.push(panel.id);
	ws.focusedGroup(W).order = ws.focusedGroup(W).order.filter((id) => id !== panel.id);
	ws.ensurePanel(W);
	expect(ws.tabsOf(W).filter((t) => t.kind === "services").length).toBe(1);
	const first = ws.group(ws.doc.layouts[W].first?.group ?? ws.doc.layouts[W].group);
	expect(ws.tab(first.order[0]).kind).toBe("services");
	void e;
});

test("the Panel cannot be moved or split out, and nothing lands before it", () => {
	fresh();
	const g = ws.focusedGroup(W);
	const panel = ws.tabsOf(W)[0];
	const e = ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	const other = ws.split(g.id, "h");
	ws.moveTab(panel.id, other.id);
	expect(ws.tab(panel.id).groupId).toBe(g.id);
	expect(ws.split(g.id, "v", panel.id)).toBeUndefined();
	ws.moveTab(e.id, g.id, 0);
	expect(g.order[0]).toBe(panel.id);
});

test("⌘T-style reopen restores editors in place; terminals and singletons behave", async () => {
	fresh();
	const a = ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	ws.openTab(W, { kind: "editor", label: "b", path: "b.ts" });
	ws.closeTab(a.id);
	expect(ws.doc.closed[W].length).toBe(1);
	const back = await ws.reopenClosed(W);
	expect(back.path).toBe("a.ts");
	expect(order()).toEqual(["services", "editor", "editor"]);
	expect(ws.doc.closed[W].length).toBe(0);
	expect(await ws.reopenClosed(W)).toBeUndefined();
	const t = await ws.newTerminal(W, { argv: ["zsh"] });
	ws.closeTab(t.id);
	const re = await ws.reopenClosed(W);
	expect(re.kind).toBe("terminal");
	expect(re.argv).toEqual(["zsh"]);
});

// Feature (#64, "Open in terminal here"): a terminal can start in a folder of the worktree, and Restart / reopen keep it
// there instead of falling back to the worktree root.
test("a terminal opened in a subfolder spawns there and remembers it for restart and reopen", async () => {
	fresh();
	created.length = 0;
	const t = await ws.newTerminal(W, { cwd: "/repo/src/api" });
	expect(created[0].cwd).toBe("/repo/src/api");
	expect(t.cwd).toBe("/repo/src/api");
	const root = await ws.newTerminal(W);
	expect(created[1].cwd).toBe(ws.worktree(W).path);
	expect(root.cwd).toBe(ws.worktree(W).path);
	ws.closeTab(t.id);
	const back = await ws.reopenClosed(W);
	expect(back.cwd).toBe("/repo/src/api");
});

// Feature (#64, "Open to the side"): the side group is the pane right of the focused one, created once and then reused.
test("sideGroup splits right the first time and reuses that pane afterwards", () => {
	fresh();
	const from = ws.focusedGroup(W).id;
	const first = ws.sideGroup(W);
	expect(first).not.toBe(from);
	ws.focusGroup(from);
	expect(ws.sideGroup(W)).toBe(first);
	expect(ws.doc.groups.filter((g) => g.worktreeId === W).length).toBe(2);
});

test("sideGroup never lands in the bottom dock", () => {
	fresh();
	const from = ws.focusedGroup(W).id;
	ws.doc.bottomGroup[W] = ws.newGroup(W).id;
	const side = ws.sideGroup(W);
	expect(side).not.toBe(ws.doc.bottomGroup[W]);
	expect(side).not.toBe(from);
});

test("activateNth follows the focused pane order, ⌘9 the last", () => {
	fresh();
	ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	ws.openTab(W, { kind: "logs", label: "Logs" });
	ws.activateNth(W, 1);
	expect(ws.tab(ws.focusedGroup(W).activeTab).kind).toBe("services");
	ws.activateNth(W, 9);
	expect(ws.tab(ws.focusedGroup(W).activeTab).kind).toBe("logs");
	ws.activateNth(W, 5);
	expect(ws.tab(ws.focusedGroup(W).activeTab).kind).toBe("logs");
});

test("focusedTab is a pure read: a worktree without a layout yields nothing and creates nothing (state_unsafe_mutation in $derived)", () => {
	fresh();
	ws.doc.layouts = {};
	const groups = ws.doc.groups.length;
	expect(ws.focusedTab(W)).toBeUndefined();
	expect(ws.doc.layouts[W]).toBeUndefined();
	expect(ws.doc.groups.length).toBe(groups);
	ws.select(W);
	const t = ws.openTab(W, { kind: "editor", label: "a", path: "a.ts" });
	expect(ws.focusedTab(W)?.id).toBe(t.id);
});

/**
 * Incident 1.5.0-alpha.5: while the local terminal host was unreachable, `host_pty_list` answered `[]`
 * and every terminal tab became "Terminal is gone" with a Restart that could not work. Now the backend
 * errors instead, and the store must keep what it knew (the terminals are very likely still alive) and
 * report the outage once.
 */
test("a failing host does not make known terminals 'gone'", async () => {
	fresh();
	ptyList = async () => [{ pty: "p1", title: "zsh", exited: null }];
	await ws.syncPtys();
	const t = ws.openTab(W, { kind: "terminal", label: "zsh" });
	t.pty = "p1";
	expect(ws.ended(t)).toBeNull();

	ptyList = async () => {
		throw "Terminal host not reachable. Terminals cannot start or reconnect until it is back.";
	};
	errors.toasts = [];
	await ws.syncPtys();
	expect(ws.ended(t)).toBeNull();
	expect(Object.keys(ws.ptys)).toEqual(["p1"]);
	expect(errors.toasts).toHaveLength(1);
	expect(ws.hostDown).toBe(true);

	ptyList = async () => [{ pty: "p1", title: "zsh", exited: null }];
	await ws.syncPtys();
	expect(ws.hostDown).toBe(false);
});

test("a terminal the host really dropped is still 'gone' once the host answers", async () => {
	fresh();
	ptyList = async () => [];
	const t = ws.openTab(W, { kind: "terminal", label: "zsh" });
	t.pty = "old";
	await ws.syncPtys();
	expect(ws.ended(t)).toBe("gone");
});

test("restart on a down host does not try to create a terminal on it", async () => {
	fresh();
	const t = ws.openTab(W, { kind: "terminal", label: "zsh" });
	t.pty = "p1";
	ptyList = async () => {
		throw "Terminal host not reachable.";
	};
	await ws.syncPtys();
	errors.toasts = [];
	await ws.restart(t.id);
	expect(t.pty).toBe("p1");
	expect(errors.toasts).toHaveLength(1);
});
