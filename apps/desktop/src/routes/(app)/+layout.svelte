<script lang="ts">
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import "@xterm/xterm/css/xterm.css";
	import { Toaster, colorScheme, toast } from "@codeatlasdev/ui";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { onMount, untrack, type Snippet } from "svelte";
	import Sidebar from "#lib/components/Sidebar.svelte";
	import Palette, { type PaletteItem } from "#lib/components/Palette.svelte";
	import { fileIcon } from "#lib/icons.ts";
	import NewWorktreeDialog from "#lib/components/NewWorktreeDialog.svelte";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { initHostStatus } from "#lib/stores/gpu.svelte.ts";
	import { api, errText } from "#lib/api.ts";
	import { host } from "#lib/host.ts";
	import { runAction, openService, configKey } from "#lib/actions.svelte.ts";
	import { activity } from "#lib/panel/activity.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import Settings from "#lib/components/Settings.svelte";
	import WhatsNew from "#lib/components/WhatsNew.svelte";
	import UpdatedCard from "#lib/components/UpdatedCard.svelte";
	import DevLine from "#lib/components/DevLine.svelte";
	import AskDialog from "#lib/components/AskDialog.svelte";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { nativeFeel } from "#lib/native.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { worktrees, hooks } from "#lib/stores/worktrees.svelte.ts";
	import { invoke } from "@tauri-apps/api/core";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { browserPrefs } from "#lib/stores/browser.svelte.ts";
	import { reconcile } from "#lib/browserSync.ts";
	import { startCtl } from "#lib/ctl.ts";
	import CreatePrDialog from "#lib/components/CreatePrDialog.svelte";
	import {
		docks,
		toggleAllDocks,
		toggleDock,
		toggleLeftPanel,
		toggleSidebar,
		peek,
	} from "#lib/stores/docks.svelte.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { startTrayFeed } from "#lib/stores/trayFeed.svelte.ts";
	import { metrics } from "#lib/panel/metrics.svelte.ts";
	import EdgePeek from "#lib/shell/EdgePeek.svelte";
	import { swipe } from "#lib/shell/swipe.ts";
	import { historySwipe } from "#lib/shell/gestures.ts";
	import { ChevronLeft, ChevronRight } from "@lucide/svelte";
	import Onboarding from "#lib/onboarding/Onboarding.svelte";
	import { focusSearch } from "#lib/search/search.svelte.ts";
	import { openFile, openDiff, worktreeFiles } from "#lib/stores/files.svelte.ts";
	import {
		register,
		allCommands,
		shortcutFor,
		dispatch,
		loadKeymap,
		noteRun,
		recent,
		type Command,
	} from "#lib/keymap.svelte.ts";
	import { editors, outline, saveAllEditors, saveEditor } from "#lib/symbols.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { loadPrefs } from "#lib/stores/prefs.svelte.ts";
	import { EditorSelection } from "@codemirror/state";
	import { EditorView } from "codemirror";
	import { pinned } from "#lib/stores/tabs.ts";
	import { startMenu } from "#lib/menu.svelte.ts";
	import { terminalKeeps } from "#lib/termKeys.ts";
	import TabSwitcher from "#lib/shell/TabSwitcher.svelte";
	import { cancelSwitcher, commitSwitcher, switchTab, switcher } from "#lib/shell/switcher.svelte.ts";

	let { children }: { children: Snippet } = $props();
	let paletteOpen = $state(false);
	let commandsOpen = $state(false);
	let quickOpen = $state(false);
	let quickFiles = $state<string[]>([]);
	let symbolsOpen = $state(false);

	/** Registry key of the focused pane's editor (file or atlas.yaml), if its active tab is one. */
	function activeEditorKey() {
		const w = ws.selected;
		const t = w && ws.tab(ws.focusedGroup(w.id).activeTab);
		if (t?.kind === "config") return `config:${ws.worktree(t.worktreeId)?.repoId}`;
		return t?.kind === "editor" && t.path ? `${t.worktreeId}:${t.path}` : undefined;
	}

	/** The focused pane's editor, if its active tab is one. */
	function activeEditor() {
		const key = activeEditorKey();
		return key ? editors.get(key) : undefined;
	}

	/** ⌘S: write the focused editor now (the autosave timer would do it in 0.5 s). */
	async function saveActive() {
		if (!(await saveEditor(activeEditorKey()))) toast.info("Nothing to save");
	}

	function symbolItems(): PaletteItem[] {
		const view = activeEditor();
		if (!view) return [];
		return outline(view.state).map((s, i) => ({
			id: `${i}`,
			group: "Symbols",
			label: s.name,
			description: `${s.kind} · line ${s.line}`,
			run: () => {
				view.dispatch({
					selection: EditorSelection.cursor(s.from),
					effects: EditorView.scrollIntoView(s.from, { y: "center" }),
				});
				view.focus();
			},
		}));
	}

	async function openQuick() {
		const w = ws.selected;
		if (!w) return;
		quickFiles = await worktreeFiles(w.id);
		quickOpen = true;
	}

	function fileItems(): PaletteItem[] {
		const w = ws.selected;
		if (!w) return [];
		return quickFiles.map((f) => {
			const i = f.lastIndexOf("/");
			return {
				id: f,
				group: "Files",
				label: f.slice(i + 1),
				icon: fileIcon(f.slice(i + 1)),
				description: i > 0 ? f.slice(0, i) : undefined,
				run: (split: boolean) => {
					if (split) {
						const g = ws.split(ws.focusedGroup(w.id).id, "h");
						if (g) ws.focusGroup(g.id);
					}
					openFile(w.id, f);
				},
			};
		});
	}

	onMount(() => {
		store.init().catch((e) => toast.error(errText(e)));
		updates.init().catch((e) => toast.error(errText(e)));
		ws.init().catch((e) => toast.error(errText(e)));
		agents.init().catch((e) => toast.error(errText(e)));
		quick.init().catch((e) => toast.error(errText(e)));
		browserPrefs.init().catch(() => {});
		const stopHostStatus = initHostStatus().catch(() => () => {});
		// A webview reload leaves native browser views from the last page behind, at the old
		// rects. Park them at once; the reconcile effect below closes the orphans once the
		// workspace has loaded, and each live tab's BrowserTab re-adopts and re-places its own.
		void invoke("browser_hide_all").catch(() => {});
		startCtl().catch(() => {});
		loadKeymap().catch(() => {});
		const stopMenu = startMenu();
		loadPrefs().catch(() => {});
		themes.init().catch((e) => toast.error(errText(e)));
		const unregister = register(commandList());
		hooks.launchAgent = (id, agent, prompt) => agents.launch(id, agent, { prompt });
		// Debug builds: the live stores, for `atlas desktop debug eval` (module
		// re-imports from eval get fresh instances).
		if ("__ATLAS_DEBUG__" in window)
			Object.assign(window, { __atlas: { ws, store, agents, worktrees, api, host } });
		Object.assign(window, { __atlasCtlStores: { ws, store, agents, worktrees, api, host } });
		const off = nativeFeel();
		const stopTray = startTrayFeed();
		// The popover's sparklines need samples even with no Panel open.
		const stopMetrics = metrics.start();
		return () => {
			unregister();
			void stopMenu.then((f) => f());
			void stopHostStatus.then((f) => f());
			store.dispose();
			updates.dispose();
			ws.dispose();
			agents.dispose();
			off();
			stopTray();
			stopMetrics();
		};
	});

	// The native sidebar material follows the window appearance, not CSS:
	// pin it to the chosen theme (null = follow macOS).
	$effect(() => {
		const p = colorScheme.preference;
		getCurrentWindow()
			.setTheme(p === "system" ? null : p)
			.catch(() => {});
	});

	// Once, right after the workspace loads: close native browser views whose tab is gone
	// (closed while the app was down, ⌘W racing a reload). Reconciling against an empty tab
	// list would close every live view, hence the wait; `untrack` keeps later tab changes
	// from re-running it (those close their own view through `closeTab`).
	$effect(() => {
		if (!ws.loaded) return;
		untrack(
			() =>
				void reconcile(
					invoke,
					ws.doc.tabs.filter((t) => t.kind === "browser").map((t) => t.id),
				).catch(() => {}),
		);
	});

	// Dock badge = unread worktrees.
	$effect(() => {
		const n = ws.doc.worktrees.filter((w) => w.unread).length;
		void invoke("set_badge", { count: n }).catch(() => {});
	});

	// Registry entries (main checkouts + linked worktrees) mirror into the workspace.
	$effect(() => {
		const projects = store.projects;
		if (ws.loaded && store.loaded) untrack(() => worktrees.sync(projects));
	});
	// Import worktrees created outside Atlas, once per repo.
	let imported: Record<string, true> = {};
	$effect(() => {
		if (!store.loaded) return;
		for (const p of store.projects)
			if (!p.parent && !imported[p.id]) {
				imported[p.id] = true;
				api.worktreesImport(p.id).then(
					(n) => {
						if (n) void store.refresh();
					},
					() => {},
				);
			}
	});
	// Selection follows the sidebar; a fresh worktree opens its default tab.
	$effect(() => {
		const id = store.selected;
		const ready = ws.loaded && store.loaded && store.projects.length;
		untrack(() => {
			if (!ready || !id || !ws.worktree(id)) return;
			if (ws.doc.selected !== id) ws.select(id);
			ws.ensureLayout(id);
			ws.ensurePanel(id);
		});
	});
	// Remember (worktree, tab) visits for ⌘[ / ⌘].
	$effect(() => {
		const w = ws.selected;
		if (!w || ws.view !== "worktree") return;
		const tabId = ws.focusedGroup(w.id).activeTab;
		untrack(() => shell.note({ worktreeId: w.id, tabId }));
	});

	const recentWorktrees = () => ws.doc.recentWorktrees.map((id) => ws.worktree(id)).filter((w) => !!w);
	const repoName = (repoId: string) => store.projects.find((p) => p.id === repoId)?.name ?? "";
	const wtLabel = (id: string) => {
		const w = ws.worktree(id);
		return w ? (w.isMain ? `${w.name} · main` : `${repoName(w.repoId)} / ${w.name}`) : id;
	};

	function goWorktree(id: string) {
		ws.view = "worktree";
		store.selected = id;
		ws.select(id);
	}

	function goTab(tabId: string, split: boolean) {
		const t = ws.tab(tabId);
		if (!t) return;
		goWorktree(t.worktreeId);
		if (split) ws.split(ws.focusedGroup(t.worktreeId).id, "h", t.id);
		else ws.activate(t.id);
	}

	function openKind(kind: "services" | "logs" | "config") {
		const w = ws.selected;
		if (!w) return;
		const labels = { services: "Panel", logs: "Logs", config: "Config" };
		ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
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

	function commandList(): Command[] {
		const hasWt = () => !!ws.selected;
		const running = () => !!store.current?.running;
		const list: Command[] = [
			{
				id: "search::Toggle",
				label: "Search in project",
				run: () => {
					if (!(docks.left && docks.leftPanel === "search")) toggleLeftPanel("search");
					focusSearch();
				},
				when: hasWt,
			},
			{
				id: "shell::ToggleSidebar",
				label: "Toggle sidebar",
				run: toggleSidebar,
			},
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
			{
				id: "workspace::ToggleLeftDock",
				label: "Toggle left dock",
				run: () => ((docks.left = !docks.left), (docks.zoomed = false)),
			},
			{
				id: "workspace::ToggleRightDock",
				label: "Toggle right sidebar",
				run: () => toggleDock("right"),
			},
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
			{ id: "command_palette::Toggle", label: "Command palette", run: () => (commandsOpen = true) },
			{ id: "atlas::Jump", label: "Jump to worktree or terminal", run: () => (paletteOpen = true) },
			{ id: "file_finder::Toggle", label: "Go to file…", run: () => void openQuick(), when: hasWt },
			{
				id: "outline::Toggle",
				label: "Go to symbol…",
				run: () => (symbolsOpen = true),
				when: () => !!activeEditor(),
			},
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
			{
				id: "pane::ActivateRecentItem",
				label: "Switch to recent tab",
				run: () => switchTab(1),
				when: hasWt,
			},
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
			{
				id: "pane::SplitRight",
				label: "Split right",
				run: () => {
					const g = ws.focusedGroup(ws.selected!.id);
					if (g.activeTab) ws.split(g.id, "h", g.activeTab);
				},
				when: hasWt,
			},
			{
				id: "pane::SplitDown",
				label: "Split down",
				run: () => {
					const g = ws.focusedGroup(ws.selected!.id);
					if (g.activeTab) ws.split(g.id, "v", g.activeTab);
				},
				when: hasWt,
			},
			{
				id: "pane::ActivateNextItem",
				label: "Next tab",
				run: () => ws.cycle(ws.selected!.id, 1),
				when: hasWt,
			},
			{
				id: "pane::ActivatePreviousItem",
				label: "Previous tab",
				run: () => ws.cycle(ws.selected!.id, -1),
				when: hasWt,
			},
			{ id: "git::Diff", label: "Changes", run: () => openDiff(ws.selected!.id), when: hasWt },
			{ id: "zed::OpenSettings", label: "Settings", run: () => updates.show() },
			{
				id: "atlas::CheckForUpdates",
				get label() {
					return updates.pill ? `Update to ${updates.release?.version}` : "Check for updates";
				},
				run: () => (updates.pill ? updates.showWhatsNew() : void updates.checkNow()),
			},
			{ id: "atlas::WhatsNew", label: "What's new", run: () => updates.showWhatsNew() },
			{ id: "atlas::ShellEnvironment", label: "Shell environment", run: () => updates.show("shell") },
			{
				id: "atlas::KeyboardShortcuts",
				label: "Keyboard shortcuts",
				run: () => updates.show("keyboard"),
			},
			{
				id: "atlas::Services",
				label: "Panel",
				run: () => openKind("services"),
				when: () => !!store.current,
			},
			{ id: "atlas::Logs", label: "Logs", run: () => openKind("logs"), when: () => !!store.current },
			{ id: "atlas::Config", label: "Config", run: () => openKind("config"), when: () => !!store.current },
			{
				id: "atlas::ShowActivity",
				label: "Show activity",
				run: () => activity.show(),
				when: () => !!store.current,
			},
			{
				id: "atlas::StartStack",
				label: "Start stack",
				run: () => store.run(`stack:${store.current!.id}`, () => api.start(store.current!.id)),
				when: () => !!store.current && !running(),
			},
			{
				id: "atlas::StopStack",
				label: "Stop stack",
				run: () => store.run(`stack:${store.current!.id}`, () => api.stop(store.current!.id)),
				when: running,
			},
			{
				id: "atlas::RestartService",
				label: "Restart focused service",
				run: () => {
					const id = store.current!.id;
					const svc = store.focusedService!;
					store.run(`svc:${id}:${svc}`, () => api.service(id, "restart_service", svc));
				},
				when: () => running() && !!store.focusedService,
			},
		];
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

	/** Registry commands plus the current project's services and actions. */
	function commandItems(): PaletteItem[] {
		const rank = (id: string) => {
			const i = recent.ids.indexOf(id);
			return i < 0 ? Infinity : i;
		};
		const items: PaletteItem[] = allCommands()
			.filter((c) => !/^(atlas::RecentWorktree|shell::Space\d|pane::ActivateItem\d)/.test(c.id))
			.map((c) => ({
				id: c.id,
				group: recent.ids.includes(c.id) ? "Recent commands" : "Commands",
				label: c.label,
				shortcut: shortcutFor(c.id),
				run: () => (noteRun(c.id), c.run()),
			}))
			.sort((a, b) => rank(a.id) - rank(b.id));
		const cur = store.current;
		if (!cur) return items;
		for (const s of store.configs[cur.id]?.services.filter((s) => s.target) ?? [])
			items.push({
				id: `svc:${s.name}`,
				group: "Project",
				label: s.target!.how === "browser" ? `Open ${s.name}` : `Copy ${s.name} URL`,
				run: () => void openService(s),
			});
		if (cur.running)
			for (const a of store.configs[cur.id]?.actions.filter((a) => !a.session) ?? [])
				items.push({
					id: `action:${a.key}`,
					group: "Project",
					label: a.label,
					run: () => runAction(cur.id, a.key),
				});
		return items;
	}

	function paletteItems(): PaletteItem[] {
		const items: PaletteItem[] = [];
		const recentTabs = recentWorktrees()
			.flatMap((w) => (ws.doc.recentTabs[w.id] ?? []).map((id) => ws.tab(id)))
			.filter((t) => t && t.kind === "terminal")
			.slice(0, 6);
		recentTabs.forEach((t) =>
			items.push({
				id: t!.id,
				group: "Recent chats & terminals",
				label: (t!.pty && ws.titles[t!.pty]) || t!.label,
				description: wtLabel(t!.worktreeId),
				run: (split) => goTab(t!.id, split),
			}),
		);
		for (const w of recentWorktrees().slice(0, 6))
			items.push({
				id: w.id,
				group: "Recent worktrees",
				project: w.repoId,
				label: wtLabel(w.id),
				description: w.path,
				run: () => goWorktree(w.id),
			});
		for (const r of store.sorted.filter((p) => !p.parent))
			for (const p of [r, ...store.projects.filter((x) => x.parent === r.id)])
				items.push({
					id: p.id,
					group: r.name,
					project: r.id,
					label: p.parent ? (ws.worktree(p.id)?.name ?? p.root) : "main",
					description: p.branch ?? p.root,
					run: () => goWorktree(p.id),
				});
		items.push(...commandItems());
		return items;
	}

	function createItems(q: string): PaletteItem[] {
		if (!q) return [];
		const repoId = store.current?.parent ?? store.current?.id;
		if (!repoId) return [];
		return [
			{
				id: "create",
				group: "Create",
				label: `Create worktree "${q}"`,
				description: repoName(repoId),
				run: () => void worktrees.create({ repoId, name: q, agent: agents.defaultAgent }),
			},
		];
	}

	async function closeActive() {
		const w = ws.selected;
		if (!w) return;
		const g = ws.focusedGroup(w.id);
		const t = ws.tab(g.activeTab);
		if (!t) return;
		if (pinned(t)) {
			// The Panel stays open: tell the user once, and move on to a neighbour.
			toast.info("The Panel stays open");
			const next = g.order.find((id) => !pinned(ws.tab(id)));
			if (next) ws.activate(next);
			return;
		}
		if (t.pty && !(t.pty in ws.exited)) {
			const info = await host.ptyInfo(t.pty).catch(() => null);
			const shell = (t.argv?.length ? t.argv[0] : info?.argv[0])?.split("/").pop();
			if (
				info?.fg &&
				info.fg.name.replace(/^-/, "") !== shell &&
				!(await confirmDialog(`${info.fg.name} is still running`, {
					description: "Closing the tab ends it.",
					confirmLabel: "Close tab",
					destructive: true,
				}))
			)
				return;
		}
		ws.closeTab(t.id);
	}

	function onkeydown(e: KeyboardEvent) {
		if (!e.metaKey && configKey(e)) {
			e.preventDefault();
			return;
		}
	}

	/** ⌃ released ends the ⌃Tab switcher: activate the highlighted tab. */
	function onkeyup(e: KeyboardEvent) {
		if (e.key === "Control" && switcher.s) commitSwitcher();
	}

	// Focus can leave the window while ⌃ is held (no keyup arrives): drop the overlay.
	$effect(() => {
		const off = () => cancelSwitcher();
		window.addEventListener("blur", off);
		return () => window.removeEventListener("blur", off);
	});

	/** Keymap commands win over focused terminals/editors, as in Zed. */
	function onkeydowncapture(e: KeyboardEvent) {
		if (e.key === "Escape" && switcher.s) {
			cancelSwitcher();
			e.preventDefault();
			e.stopPropagation();
			return;
		}
		const sink = document.activeElement instanceof HTMLElement && document.activeElement.dataset.termTab;
		if (e.isComposing || !dispatch(e, sink ? terminalKeeps : undefined)) return;
		e.preventDefault();
		e.stopPropagation();
	}
</script>

<svelte:window {onkeydown} {onkeydowncapture} {onkeyup} onfocus={() => store.refresh()} />

{#snippet sidebar()}
	<Sidebar
		onJump={() => (paletteOpen = true)}
		onAgent={(id) => {
			store.selected = id;
			ws.select(id);
			void agents.launch(id);
		}}
	/>
{/snippet}

<div
	class="app"
	style:--space-accent={shell.active ? shell.accent(shell.active.id) : undefined}
	style:--space-tint={shell.active ? shell.tint(shell.active.id) : undefined}
>
	<div class="sidebar-slot" class:hidden={!docks.sidebar} inert={!docks.sidebar}>
		<div class="sidebar-panel">{@render sidebar()}</div>
	</div>
	<EdgePeek edge="left" size={260} label="Sidebar" active={!docks.sidebar}>{@render sidebar()}</EdgePeek>
	<main class="content" class:full={!docks.sidebar} use:swipe={historySwipe()}>
		<div class="space-edge" aria-hidden="true"></div>
		<div class="nav-chip nav-chip-prev" aria-hidden="true"><ChevronLeft size={18} /></div>
		<div class="nav-chip nav-chip-next" aria-hidden="true"><ChevronRight size={18} /></div>
		{#key shell.epoch}
			<div class="space-view" data-dir={shell.direction}>
				{@render children()}
			</div>
		{/key}
	</main>
</div>

<Palette
	bind:open={paletteOpen}
	placeholder="Jump to worktree, terminal or command"
	items={paletteItems}
	dynamic={createItems}
/>
<Palette bind:open={commandsOpen} placeholder="Run a command" items={commandItems} />
<NewWorktreeDialog />
<CreatePrDialog />
<Palette bind:open={quickOpen} placeholder="Go to file" items={fileItems} />
<Palette bind:open={symbolsOpen} placeholder="Go to symbol in file" items={symbolItems} />
<Settings />
<WhatsNew />
<UpdatedCard />
<DevLine />
<Onboarding />
<AskDialog />
<TabSwitcher />
<Toaster />
