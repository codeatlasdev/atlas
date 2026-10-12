// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, mock, test } from "bun:test";

mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@codeatlasdev/ui", () => ({ toast: { error() {}, info() {} } }));
mock.module("#lib/api.ts", () => ({ errText: String }));
mock.module("#lib/host.ts", () => ({
	host: { ptyKill: async () => {}, ptyList: async () => [], ptyCreate: async () => ({ pty: "p1" }) },
}));
globalThis.document = { hasFocus: () => true };
// Runes are compiled by Svelte; plain objects are enough for the rules under test.
globalThis.$state = (v) => v;
globalThis.$effect = Object.assign(() => {}, { root: (f) => f() });

const { ws } = await import("./workspace.svelte.ts");

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
