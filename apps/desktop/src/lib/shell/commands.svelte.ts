/**
 * Shell command registry: docks, panes/tabs, spaces, palettes and the numbered chords,
 * composed with the project commands (`projectCommands.svelte.ts`) in palette order.
 */
import { toast } from "@codeatlasdev/ui";
import type { Command } from "#lib/keymap.svelte.ts";
import { focusSearch } from "#lib/search/search.svelte.ts";
import { closeActive } from "#lib/shell/closeTab.ts";
import { activeEditor, saveActive } from "#lib/shell/editors.ts";
import { goWorktree, recentWorktrees } from "#lib/shell/navigate.ts";
import { openQuick, palettes } from "#lib/shell/palettes.svelte.ts";
import { projectCommands } from "#lib/shell/projectCommands.svelte.ts";
import { shell } from "#lib/shell/shell.svelte.ts";
import { switchTab } from "#lib/shell/switcher.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import {
	docks,
	toggleAllDocks,
	toggleDock,
	toggleLeftPanel,
	toggleSidebar,
} from "#lib/stores/docks.svelte.ts";
import { openDiff } from "#lib/stores/files.svelte.ts";
import { isHtml } from "#lib/stores/preview.ts";
import { previewHtml } from "#lib/stores/preview.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { worktrees } from "#lib/stores/worktrees.svelte.ts";
import { stepZoom } from "#lib/stores/zoom.svelte.ts";
import { editorShellCommands } from "#lib/editor/shellCommands.ts";
import { lspCommands } from "#lib/lsp/commands.ts";
import { saveAllEditors } from "#lib/symbols.ts";

const hasWt = () => !!ws.selected;

/** Bottom dock: show it (with a terminal if empty) and focus it; hide it when focused. */
async function toggleTerminalPanel() {
	const w = ws.selected;
	if (!w) return;
	const g = ws.bottomGroup(w.id);
	if (docks.bottom && ws.focusedGroup(w.id).id === g.id) {
		docks.bottom = false;
		ws.focusTree(w.id);
		return;
	}
	docks.bottom = true;
	docks.zoomed = false;
	ws.focusGroup(g.id);
	if (!g.order.length) await ws.newTerminal(w.id, { groupId: g.id });
}

function dockCommands(): Command[] {
	return [
		{
			id: "workspace::ToggleLeftDock",
			label: "Toggle left dock",
			run: () => ((docks.left = !docks.left), (docks.zoomed = false)),
		},
		{ id: "workspace::ToggleRightDock", label: "Toggle right sidebar", run: () => toggleDock("right") },
		{
			id: "workspace::ToggleBottomDock",
			label: "Toggle bottom dock",
			run: () => {
				docks.bottom = !docks.bottom;
				docks.zoomed = false;
				if (!docks.bottom && ws.selected) ws.focusTree(ws.selected.id);
			},
		},
		{ id: "workspace::ToggleAllDocks", label: "Toggle all docks", run: toggleAllDocks },
		{
			id: "workspace::ToggleZoom",
			label: "Toggle pane zoom",
			run: () => (docks.zoomed = !docks.zoomed),
			when: hasWt,
		},
		{
			id: "workspace::ToggleCenteredLayout",
			label: "Toggle centered layout",
			run: () => (docks.centered = !docks.centered),
		},
	];
}

function panelCommands(): Command[] {
	return [
		{
			id: "project_panel::ToggleFocus",
			label: "Toggle project panel",
			run: () => toggleLeftPanel("explorer"),
		},
		{ id: "git_panel::ToggleFocus", label: "Toggle git panel", run: () => toggleLeftPanel("scm") },
		{
			id: "terminal_panel::ToggleFocus",
			label: "Toggle terminal panel",
			run: () => void toggleTerminalPanel(),
			when: hasWt,
		},
	];
}

function paletteCommands(): Command[] {
	return [
		{ id: "command_palette::Toggle", label: "Command palette", run: () => (palettes.commands = true) },
		{ id: "atlas::Jump", label: "Jump to worktree or terminal", run: () => (palettes.jump = true) },
		{ id: "file_finder::Toggle", label: "Go to file…", run: () => void openQuick(), when: hasWt },
		{
			id: "outline::Toggle",
			label: "Go to symbol…",
			run: () => {
				const v = activeEditor();
				if (v) void import("#lib/lsp/symbolsLoad.ts").then((m) => m.loadSymbols(v));
				palettes.symbols = true;
			},
			when: () => !!activeEditor(),
		},
	];
}

/** Tabs and panes of the selected worktree. */
function tabCommands(): Command[] {
	const split = (dir: "h" | "v") => () => {
		const g = ws.focusedGroup(ws.selected!.id);
		if (g.activeTab) ws.split(g.id, dir, g.activeTab);
	};
	return [
		{
			id: "workspace::NewTerminal",
			label: "New terminal",
			run: () => void ws.newTerminal(ws.selected!.id),
			when: hasWt,
		},
		{
			id: "atlas::NewAgentSession",
			label: "New agent session",
			run: () => void agents.launch(ws.selected!.id),
			when: hasWt,
		},
		{
			id: "atlas::NewOmpChat",
			label: "New omp chat",
			run: () => void agents.launch(ws.selected!.id, "omp", { chat: true }),
			when: hasWt,
		},
		{
			id: "atlas::NewBrowserTab",
			label: "New browser tab",
			run: () => ws.openTab(ws.selected!.id, { kind: "browser", label: "Browser" }),
			when: hasWt,
		},
		{
			id: "atlas::PreviewHtml",
			label: "Open HTML preview",
			run: () => {
				const w = ws.selected!;
				void previewHtml(w.id, ws.tab(ws.focusedGroup(w.id).activeTab)!.path!);
			},
			when: () => {
				const w = ws.selected;
				const t = w && ws.tab(ws.focusedGroup(w.id).activeTab);
				return t?.kind === "editor" && isHtml(t.path);
			},
		},
		{ id: "pane::CloseActiveItem", label: "Close tab", run: () => void closeActive(), when: hasWt },
		{
			id: "pane::CloseOtherItems",
			label: "Close other tabs",
			run: () => {
				const g = ws.focusedGroup(ws.selected!.id);
				ws.closeMany(g.id, g.activeTab ?? undefined);
			},
			when: hasWt,
		},
		{
			id: "pane::CloseAllItems",
			label: "Close all tabs in pane",
			run: () => ws.closeMany(ws.focusedGroup(ws.selected!.id).id),
			when: hasWt,
		},
		{
			id: "pane::ReopenClosedItem",
			label: "Reopen closed tab",
			run: () => void ws.reopenClosed(ws.selected!.id),
			when: () => !!ws.selected && !!ws.doc.closed[ws.selected.id]?.length,
		},
		{ id: "pane::ActivateRecentItem", label: "Switch to recent tab", run: () => switchTab(1), when: hasWt },
		{
			id: "pane::ActivateOlderRecentItem",
			label: "Switch to older recent tab",
			run: () => switchTab(-1),
			when: hasWt,
		},
		{
			id: "pane::ActivateLastItem",
			label: "Go to last tab",
			run: () => ws.activateNth(ws.selected!.id, 9),
			when: hasWt,
		},
		{
			id: "atlas::CloseEditors",
			label: "Close all editors",
			run: () =>
				ws
					.tabsOf(ws.selected!.id)
					.filter((t) => t.kind === "editor")
					.forEach((t) => ws.closeTab(t.id)),
			when: hasWt,
		},
		{ id: "pane::SplitRight", label: "Split right", run: split("h"), when: hasWt },
		{ id: "pane::SplitDown", label: "Split down", run: split("v"), when: hasWt },
		{ id: "pane::ActivateNextItem", label: "Next tab", run: () => ws.cycle(ws.selected!.id, 1), when: hasWt },
		{
			id: "pane::ActivatePreviousItem",
			label: "Previous tab",
			run: () => ws.cycle(ws.selected!.id, -1),
			when: hasWt,
		},
		{ id: "git::Diff", label: "Changes", run: () => openDiff(ws.selected!.id), when: hasWt },
	];
}

/** ⌘1…⌘9-style chords: spaces, tab positions and recent worktrees. */
function numberedCommands(): Command[] {
	const list: Command[] = [];
	for (let n = 1; n <= 9; n++)
		list.push({
			id: `shell::Space${n}`,
			label: `Go to space ${n}`,
			run: () => shell.spaceAt(n),
			when: () => !!shell.spaces[n - 1],
		});
	for (let n = 1; n <= 8; n++)
		list.push({
			id: `pane::ActivateItem${n}`,
			label: `Go to tab ${n}`,
			run: () => ws.activateNth(ws.selected!.id, n),
			when: hasWt,
		});
	for (let n = 1; n <= 6; n++)
		list.push({
			id: `atlas::RecentWorktree${n}`,
			label: `Recent worktree ${n}`,
			run: () => {
				const r = recentWorktrees()[n - 1];
				if (r) goWorktree(r.id);
			},
		});
	return list;
}

export function shellCommands(): Command[] {
	return [
		{
			id: "search::Toggle",
			label: "Search in project",
			run: () => {
				if (!(docks.left && docks.leftPanel === "search")) toggleLeftPanel("search");
				focusSearch();
			},
			when: hasWt,
		},
		{ id: "shell::ToggleSidebar", label: "Toggle sidebar", run: toggleSidebar },
		{ id: "workspace::Save", label: "Save", run: () => void saveActive() },
		{
			id: "workspace::SaveAll",
			label: "Save all",
			run: () =>
				void saveAllEditors().then((n) => {
					if (!n) toast.info("Nothing to save");
				}),
		},
		{ id: "shell::Back", label: "Go back", run: () => shell.go(-1), when: () => shell.canGo(-1) },
		{ id: "shell::Forward", label: "Go forward", run: () => shell.go(1), when: () => shell.canGo(1) },
		{ id: "shell::PreviousSpace", label: "Previous space", run: () => shell.cycleSpace(-1) },
		{ id: "shell::NextSpace", label: "Next space", run: () => shell.cycleSpace(1) },
		{ id: "shell::ZoomIn", label: "Zoom in", run: () => void stepZoom("in") },
		{ id: "shell::ZoomOut", label: "Zoom out", run: () => void stepZoom("out") },
		{ id: "shell::ZoomReset", label: "Reset zoom", run: () => void stepZoom("reset") },
		...dockCommands(),
		...paletteCommands(),
		...panelCommands(),
		{
			id: "agent::Toggle",
			label: "Agents",
			run: () => (ws.view = ws.view === "agents" ? "worktree" : "agents"),
		},
		{ id: "atlas::Dashboard", label: "Dashboard", run: () => (ws.view = "dashboard") },
		{
			id: "atlas::NewWorktree",
			label: "New worktree…",
			run: () => (worktrees.newDialog = { open: true, repoId: null }),
		},
		...tabCommands(),
		...projectCommands(),
		...lspCommands(),
		...editorShellCommands(),
		...numberedCommands(),
	];
}
