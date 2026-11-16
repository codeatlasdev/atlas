// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, expect, mock, test } from "bun:test";

// Regression: 1.5.0-alpha.26 first-user pass, the palette's "Recent commands" listed "Command palette" itself, the
// one command every user has just run to get there. It is still recorded (the "Try the command palette" checklist
// item ticks from it), but never listed.
// The palette module's graph reaches the theme store (Vite-only `import.meta.glob`) and the Tauri bridge.
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };
// Only the registry and the recents are under test: every other module the palette lists from is an empty fake.
const stub = (path, exports) => mock.module(path, () => exports);
stub("#lib/icons.ts", { fileIcon: () => "" });
stub("#lib/actions.svelte.ts", { openService() {}, runAction() {} });
stub("#lib/shell/editors.ts", { activeEditor: () => null });
stub("#lib/stores/agents.svelte.ts", { agents: { list: [] } });
stub("#lib/stores/files.svelte.ts", { worktreeFiles: async () => [], openFile() {} });
stub("#lib/stores/projects.svelte.ts", { store: { current: null, configs: {}, projects: [] } });
stub("#lib/stores/workspace.svelte.ts", { ws: { selected: null, doc: { recentTabs: {} } } });
stub("#lib/stores/worktrees.svelte.ts", { worktrees: {} });
stub("#lib/lsp/symbols.svelte.ts", { lspSymbols: async () => [] });
stub("#lib/symbols.ts", { ops: {} });
stub("#lib/shell/navigate.ts", { goTab() {}, goWorktree() {}, recentWorktrees: () => [], wtLabel: () => "" });
await (await import("../../../test/uiKit.ts")).installUiKit();
const { register, noteRun, recent, runCommand } = await import("#lib/keymap.svelte.ts");
const { commandItems } = await import("./palettes.svelte.ts");

const ran = [];
register([
	{ id: "command_palette::Toggle", label: "Command palette", run: () => ran.push("palette") },
	{ id: "test::Alpha", label: "Alpha", run: () => ran.push("alpha") },
	{ id: "test::Beta", label: "Beta", run: () => ran.push("beta") },
]);
beforeEach(() => {
	recent.ids = [];
});
const groupOf = (id) => commandItems().find((i) => i.id === id)?.group;

test("running the palette command does not list it under Recent commands, nor float it first", () => {
	runCommand("command_palette::Toggle");
	expect(ran).toContain("palette");
	expect(groupOf("command_palette::Toggle")).toBe("Commands");
	noteRun("test::Alpha");
	expect(groupOf("test::Alpha")).toBe("Recent commands");
	expect(commandItems()[0].id).toBe("test::Alpha");
});

test("it is still recorded as run, so the checklist can tick 'Try the command palette'", () => {
	runCommand("command_palette::Toggle");
	expect(recent.ids).toEqual(["command_palette::Toggle"]);
});
