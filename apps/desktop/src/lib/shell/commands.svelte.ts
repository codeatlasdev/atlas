/**
 * Shell command registry: docks, panes/tabs, spaces, palettes and the numbered chords,
 * composed with the project commands (`projectCommands.svelte.ts`) in palette order.
 */
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { IS_MAC } from "#lib/keys.ts";
import { named, type Command } from "#lib/keymap.svelte.ts";
import { focusSearch } from "#lib/search/search.svelte.ts";
import { closeActive } from "#lib/shell/closeTab.ts";
import { activeEditor, saveActive } from "#lib/shell/editors.ts";
import { goWorktree, recentWorktrees } from "#lib/shell/navigate.ts";
import { openQuick, palettes } from "#lib/shell/palettes.svelte.ts";
import { projectCommands } from "#lib/shell/projectCommands.svelte.ts";
import { shell } from "#lib/shell/shell.svelte.ts";
import { switchTab } from "#lib/shell/switcher.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import { replayCrewOnboarding } from "#lib/crew/crewOnboarding.svelte.ts";
import { crewView } from "#lib/crew/crewView.svelte.ts";
import {
	docks,
	toggleAllDocks,
	toggleDock,
	toggleLeftPanel,
	toggleSidebar,
} from "#lib/stores/docks.svelte.ts";
import { commitMessages, generateForSelected } from "#lib/stores/commitMessage.svelte.ts";
import { openBranchPicker } from "#lib/stores/branch.svelte.ts";
import { openDiff } from "#lib/stores/files.svelte.ts";
import { isHtml } from "#lib/stores/preview.ts";
import { previewHtml } from "#lib/stores/preview.svelte.ts";
import { hasTermView, reloadTermView } from "#lib/stores/termViews.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { worktrees } from "#lib/stores/worktrees.svelte.ts";
import { stepZoom } from "#lib/stores/zoom.svelte.ts";
import { editorShellCommands } from "#lib/editor/shellCommands.ts";
import { lspCommands } from "#lib/lsp/commands.ts";
import { saveAllEditors } from "#lib/symbols.ts";
import { m } from "#lib/i18n/m.ts";

const hasWt = () => !!ws.selected;

/** F11: the window's own full screen (the page has no title bar on Windows, so this is the only way in or out). */
async function toggleFullscreen() {
	const win = getCurrentWindow();
	await win.setFullscreen(!(await win.isFullscreen()));
}

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
	return named([
		{
			id: "workspace::ToggleLeftDock",
			label: () => m.shell_commands_toggle_left_dock(),
			run: () => ((docks.left = !docks.left), (docks.zoomed = false)),
		},
		{
			id: "workspace::ToggleRightDock",
			label: () => m.shell_commands_toggle_right_sidebar(),
			run: () => toggleDock("right"),
		},
		{
			id: "workspace::ToggleBottomDock",
			label: () => m.shell_commands_toggle_bottom_dock(),
			run: () => {
				docks.bottom = !docks.bottom;
				docks.zoomed = false;
				if (!docks.bottom && ws.selected) ws.focusTree(ws.selected.id);
			},
		},
		{
			id: "workspace::ToggleAllDocks",
			label: () => m.shell_commands_toggle_all_docks(),
			run: toggleAllDocks,
		},
		{
			id: "workspace::ToggleZoom",
			label: () => m.shell_commands_toggle_pane_zoom(),
			run: () => (docks.zoomed = !docks.zoomed),
			when: hasWt,
		},
		{
			id: "workspace::ToggleCenteredLayout",
			label: () => m.shell_commands_toggle_centered_layout(),
			run: () => (docks.centered = !docks.centered),
		},
	]);
}

function panelCommands(): Command[] {
	return named([
		{
			id: "project_panel::ToggleFocus",
			label: () => m.shell_commands_toggle_project_panel(),
			run: () => toggleLeftPanel("explorer"),
		},
		{
			id: "git_panel::ToggleFocus",
			label: () => m.shell_commands_toggle_git_panel(),
			run: () => toggleLeftPanel("scm"),
		},
		{
			id: "terminal_panel::ToggleFocus",
			label: () => m.shell_commands_toggle_terminal_panel(),
			run: () => void toggleTerminalPanel(),
			when: hasWt,
		},
	]);
}

function paletteCommands(): Command[] {
	return named([
		{
			id: "command_palette::Toggle",
			label: () => m.shell_commands_command_palette(),
			run: () => (palettes.commands = true),
		},
		{
			id: "atlas::Jump",
			label: () => m.shell_commands_jump(),
			// One ⌘K: in the Crew workspace it jumps between crewmates, elsewhere between worktrees and terminals.
			run: () => (ws.appMode === "crew" ? (palettes.crew = true) : (palettes.jump = true)),
		},
		{
			id: "file_finder::Toggle",
			label: () => m.shell_commands_go_to_file(),
			run: () => void openQuick(),
			when: hasWt,
		},
		{
			id: "outline::Toggle",
			label: () => m.shell_commands_go_to_symbol(),
			run: () => {
				const v = activeEditor();
				if (v) void import("#lib/lsp/symbolsLoad.ts").then((mod) => mod.loadSymbols(v));
				palettes.symbols = true;
			},
			when: () => !!activeEditor(),
		},
	]);
}

/** Tabs and panes of the selected worktree. */
function tabCommands(): Command[] {
	const split = (dir: "h" | "v") => () => {
		const g = ws.focusedGroup(ws.selected!.id);
		if (g.activeTab) ws.split(g.id, dir, g.activeTab);
	};
	return named([
		{
			id: "workspace::NewTerminal",
			label: () => m.shell_commands_new_terminal(),
			run: () => void ws.newTerminal(ws.selected!.id),
			when: hasWt,
		},
		{
			id: "atlas::NewAgentSession",
			label: () => m.shell_commands_new_agent_session(),
			run: () => void agents.launch(ws.selected!.id),
			when: hasWt,
		},
		{
			id: "atlas::NewOmpChat",
			label: () => m.shell_commands_new_omp_chat(),
			run: () => void agents.launch(ws.selected!.id, "omp", { chat: true }),
			when: hasWt,
		},
		{
			id: "atlas::NewBrowserTab",
			label: () => m.shell_commands_new_browser_tab(),
			// The name a browser tab is stored with: an identifier that the tab label maps to the app's language.
			run: () => ws.openTab(ws.selected!.id, { kind: "browser", label: "Browser" }),
			when: hasWt,
		},
		{
			id: "atlas::PreviewHtml",
			label: () => m.shell_commands_open_html_preview(),
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
		{
			id: "terminal::Reload",
			label: () => m.shell_commands_reload_terminal(),
			run: () => {
				const tab = ws.focusedGroup(ws.selected!.id).activeTab;
				void reloadTermView(tab).catch((e) => toast.error(errText(e)));
			},
			when: () => !!ws.selected && hasTermView(ws.focusedGroup(ws.selected.id).activeTab),
		},
		{
			id: "pane::CloseActiveItem",
			label: () => m.shell_commands_close_tab(),
			run: () => void closeActive(),
			when: hasWt,
		},
		{
			id: "pane::CloseOtherItems",
			label: () => m.shell_commands_close_other_tabs(),
			run: () => {
				const g = ws.focusedGroup(ws.selected!.id);
				ws.closeMany(g.id, g.activeTab ?? undefined);
			},
			when: hasWt,
		},
		{
			id: "pane::CloseAllItems",
			label: () => m.shell_commands_close_all_tabs(),
			run: () => ws.closeMany(ws.focusedGroup(ws.selected!.id).id),
			when: hasWt,
		},
		{
			id: "pane::ReopenClosedItem",
			label: () => m.shell_commands_reopen_closed_tab(),
			run: () => void ws.reopenClosed(ws.selected!.id),
			when: () => !!ws.selected && !!ws.doc.closed[ws.selected.id]?.length,
		},
		{
			id: "pane::ActivateRecentItem",
			label: () => m.shell_commands_switch_to_recent_tab(),
			run: () => switchTab(1),
			when: hasWt,
		},
		{
			id: "pane::ActivateOlderRecentItem",
			label: () => m.shell_commands_switch_to_older_tab(),
			run: () => switchTab(-1),
			when: hasWt,
		},
		{
			id: "pane::ActivateLastItem",
			label: () => m.shell_commands_go_to_last_tab(),
			// In the Crew workspace the same chord picks the last crewmate of the sidebar.
			run: () => (ws.appMode === "crew" ? crew.selectAt("last") : ws.activateNth(ws.selected!.id, 9)),
			when: () => (ws.appMode === "crew" ? crew.sidebarOrder.length > 0 : hasWt()),
		},
		{
			id: "atlas::CloseEditors",
			label: () => m.shell_commands_close_all_editors(),
			run: () =>
				ws
					.tabsOf(ws.selected!.id)
					.filter((t) => t.kind === "editor")
					.forEach((t) => ws.closeTab(t.id)),
			when: hasWt,
		},
		{ id: "pane::SplitRight", label: () => m.shell_commands_split_right(), run: split("h"), when: hasWt },
		{ id: "pane::SplitDown", label: () => m.shell_commands_split_down(), run: split("v"), when: hasWt },
		{
			id: "pane::ActivateNextItem",
			label: () => m.shell_commands_next_tab(),
			run: () => ws.cycle(ws.selected!.id, 1),
			when: hasWt,
		},
		{
			id: "pane::ActivatePreviousItem",
			label: () => m.shell_commands_previous_tab(),
			run: () => ws.cycle(ws.selected!.id, -1),
			when: hasWt,
		},
		{
			id: "git::Diff",
			label: () => m.shell_commands_changes(),
			run: () => openDiff(ws.selected!.id),
			when: hasWt,
		},
		{
			id: "git::GenerateCommitMessage",
			label: () => m.shell_commands_generate_commit_message(),
			run: generateForSelected,
			when: () => hasWt() && commitMessages.provider.state !== "off",
		},
		{
			id: "git::Branch",
			label: () => m.shell_commands_switch_branch(),
			run: () => void openBranchPicker(),
			when: hasWt,
		},
	]);
}

/** ⌘1…⌘9-style chords: spaces, tab positions and recent worktrees. */
function numberedCommands(): Command[] {
	const list: Parameters<typeof named>[0] = [];
	for (let n = 1; n <= 9; n++)
		list.push({
			id: `shell::Space${n}`,
			label: () => m.shell_commands_go_to_space({ n }),
			run: () => shell.spaceAt(n),
			when: () => !!shell.spaces[n - 1],
		});
	for (let n = 1; n <= 8; n++)
		list.push({
			id: `pane::ActivateItem${n}`,
			label: () => m.shell_commands_go_to_tab({ n }),
			// ⌘n: the n-th tab in the IDE, the n-th crewmate of the sidebar in the Crew workspace (where there are no tabs).
			run: () => (ws.appMode === "crew" ? crew.selectAt(n) : ws.activateNth(ws.selected!.id, n)),
			when: () => (ws.appMode === "crew" ? !!crew.sidebarOrder[n - 1] : hasWt()),
		});
	for (let n = 1; n <= 6; n++)
		list.push({
			id: `atlas::RecentWorktree${n}`,
			label: () => m.shell_commands_recent_worktree({ n }),
			run: () => {
				const r = recentWorktrees()[n - 1];
				if (r) goWorktree(r.id);
			},
		});
	return named(list);
}

export function shellCommands(): Command[] {
	return [
		...named([
			{
				id: "search::Toggle",
				label: () => m.shell_commands_search_in_project(),
				run: () => {
					if (!(docks.left && docks.leftPanel === "search")) toggleLeftPanel("search");
					focusSearch();
				},
				when: hasWt,
			},
			{ id: "shell::ToggleSidebar", label: () => m.shell_commands_toggle_sidebar(), run: toggleSidebar },
			// Off macOS only (their keys: lib/keys.ts); macOS has the menu's ⌘Q and the green button.
			{
				id: "shell::Quit",
				label: () => m.shell_commands_quit(),
				run: () => void invoke("app_quit").catch(() => {}),
				when: () => !IS_MAC,
			},
			{
				id: "shell::ToggleFullscreen",
				label: () => m.shell_commands_toggle_fullscreen(),
				run: toggleFullscreen,
				when: () => !IS_MAC,
			},
			{ id: "workspace::Save", label: () => m.common_action_save(), run: () => void saveActive() },
			{
				id: "workspace::SaveAll",
				label: () => m.shell_commands_save_all(),
				run: () =>
					void saveAllEditors().then((n) => {
						if (!n) toast.info(m.shell_commands_nothing_to_save());
					}),
			},
			{
				id: "shell::Back",
				label: () => m.shell_commands_go_back(),
				run: () => shell.go(-1),
				when: () => shell.canGo(-1),
			},
			{
				id: "shell::Forward",
				label: () => m.shell_commands_go_forward(),
				run: () => shell.go(1),
				when: () => shell.canGo(1),
			},
			{
				id: "shell::PreviousSpace",
				label: () => m.shell_commands_previous_space(),
				run: () => shell.cycleSpace(-1),
			},
			{ id: "shell::NextSpace", label: () => m.shell_commands_next_space(), run: () => shell.cycleSpace(1) },
			{ id: "shell::ZoomIn", label: () => m.shell_commands_zoom_in(), run: () => void stepZoom("in") },
			{ id: "shell::ZoomOut", label: () => m.shell_commands_zoom_out(), run: () => void stepZoom("out") },
			{
				id: "shell::ZoomReset",
				label: () => m.shell_commands_zoom_reset(),
				run: () => void stepZoom("reset"),
			},
		]),
		...dockCommands(),
		...paletteCommands(),
		...panelCommands(),
		...named([
			{
				id: "agent::Toggle",
				label: () => m.shell_commands_agents(),
				run: () => (ws.view = ws.view === "agents" ? "worktree" : "agents"),
			},
			{
				id: "atlas::Dashboard",
				label: () => m.shell_commands_dashboard(),
				run: () => (ws.view = "dashboard"),
			},
			{
				id: "atlas::ToggleCrewWorkspace",
				label: () => m.shell_commands_crew(),
				run: () => (ws.appMode = ws.appMode === "crew" ? "ide" : "crew"),
			},
			{
				id: "atlas::ReplayCrewOnboarding",
				label: () => m.shell_commands_crew_replay_onboarding(),
				run: replayCrewOnboarding,
			},
			{
				id: "atlas::ToggleCrewBoard",
				label: () => m.shell_commands_crew_toggle_board(),
				run: () => {
					crewView.value = crewView.value === "chat" ? "board" : "chat";
				},
				when: () => ws.appMode === "crew",
			},
			{
				id: "atlas::NewWorktree",
				label: () => m.shell_commands_new_worktree(),
				run: () => (worktrees.newDialog = { open: true, repoId: null }),
			},
		]),
		...tabCommands(),
		...projectCommands(),
		...lspCommands(),
		...editorShellCommands(),
		...numberedCommands(),
	];
}
