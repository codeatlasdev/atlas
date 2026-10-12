/**
 * Settings navigation model: which sections exist, the tabs inside Appearance,
 * where each setting lives, and the search over them. Pure: no Svelte, no IPC.
 */

export type SettingsSection =
	"general" | "appearance" | "workspaces" | "agents" | "keyboard" | "ssh" | "help";

export type AppearanceTab = "appearance" | "editor" | "interface" | "terminal";

/** Where Settings opens: a section, plus a tab when the section has tabs. */
export interface SettingsTarget {
	section: SettingsSection;
	tab?: AppearanceTab;
}

export const SECTIONS: { id: SettingsSection; label: string }[] = [
	{ id: "general", label: "General" },
	{ id: "appearance", label: "Appearance" },
	{ id: "workspaces", label: "Workspaces" },
	{ id: "agents", label: "Agents" },
	{ id: "keyboard", label: "Keyboard shortcuts" },
	{ id: "ssh", label: "SSH hosts" },
	{ id: "help", label: "Help & onboarding" },
];

export const APPEARANCE_TABS: { id: AppearanceTab; label: string }[] = [
	{ id: "appearance", label: "Appearance" },
	{ id: "editor", label: "Editor" },
	{ id: "interface", label: "Interface" },
	{ id: "terminal", label: "Terminal" },
];

/**
 * Section ids used before the reorganisation (and by `updates.show(...)` call sites)
 * mapped to where that content lives now.
 */
const LEGACY: Record<string, SettingsTarget> = {
	updates: { section: "general" },
	shell: { section: "general" },
	appearance: { section: "appearance", tab: "appearance" },
	editor: { section: "appearance", tab: "editor" },
};

const SECTION_IDS: Record<SettingsSection, true> = {
	general: true,
	appearance: true,
	workspaces: true,
	agents: true,
	keyboard: true,
	ssh: true,
	help: true,
};
const TAB_IDS: Record<AppearanceTab, true> = {
	appearance: true,
	editor: true,
	interface: true,
	terminal: true,
};
const has = <T extends string>(table: Record<T, true>, k: string): k is T => Object.hasOwn(table, k);

/**
 * Accepts any current or legacy section id, or a `section/tab` pair (`appearance/terminal`),
 * and returns the target. Unknown ids fall back to General.
 */
export function resolveSection(id: string): SettingsTarget {
	const [head, sub] = id.split("/");
	const tab = sub && has(TAB_IDS, sub) ? sub : undefined;
	if (head === "appearance") return { section: "appearance", tab: tab ?? "appearance" };
	if (Object.hasOwn(LEGACY, head)) return LEGACY[head]!;
	if (has(SECTION_IDS, head)) return { section: head };
	return { section: "general" };
}

export interface SettingEntry extends SettingsTarget {
	id: string;
	label: string;
	description: string;
	/** Extra words people may type that the label does not contain. */
	keywords?: string;
	/** Group title inside the section, shown with search results. */
	group: string;
}

const appearance = (
	tab: AppearanceTab,
	group: string,
	id: string,
	label: string,
	description: string,
	keywords = "",
): SettingEntry => ({ section: "appearance", tab, group, id, label, description, keywords });

const entry = (
	section: SettingsSection,
	group: string,
	id: string,
	label: string,
	description: string,
	keywords = "",
): SettingEntry => ({ section, group, id, label, description, keywords });

/** Every user-facing setting. Keep labels and descriptions identical to the screens. */
export const SETTINGS_INDEX: SettingEntry[] = [
	entry(
		"general",
		"Updates",
		"updates",
		"Updates",
		"Check for a new Atlas release and install it.",
		"version upgrade release cli app",
	),
	entry(
		"general",
		"Shell environment",
		"shell",
		"Shell for your projects",
		"Services start with the PATH and variables of this shell, the same as in your terminal.",
		"zsh bash fish path environment variables env login",
	),

	appearance(
		"appearance",
		"Color theme",
		"theme-mode",
		"Theme",
		"Follow the system, or always use light or dark.",
		"dark light mode system",
	),
	appearance(
		"appearance",
		"Color theme",
		"theme-light",
		"Light theme",
		"Colors used when Atlas is in light mode.",
		"zed json",
	),
	appearance(
		"appearance",
		"Color theme",
		"theme-dark",
		"Dark theme",
		"Colors used when Atlas is in dark mode.",
		"zed json",
	),
	appearance(
		"appearance",
		"Color theme",
		"accent",
		"Accent color",
		"Highlights buttons, selections and focus.",
		"brand color swatch",
	),

	appearance(
		"editor",
		"Code font",
		"buffer-font",
		"Code font",
		"Font for the editor and diffs.",
		"buffer_font_family monospace typeface",
	),
	appearance(
		"editor",
		"Code font",
		"buffer-size",
		"Code font size",
		"Text size in the editor and diffs, in pixels.",
		"buffer_font_size",
	),
	appearance(
		"editor",
		"Code font",
		"buffer-line-height",
		"Line spacing",
		"Space between lines of code, as a multiple of the font size.",
		"buffer_line_height leading",
	),

	appearance(
		"interface",
		"Interface font",
		"ui-font",
		"Interface font",
		"Font for menus, sidebars and buttons.",
		"ui_font_family typeface",
	),
	appearance(
		"interface",
		"Interface font",
		"ui-size",
		"Interface font size",
		"Text size across the app, in pixels.",
		"ui_font_size",
	),
	appearance(
		"interface",
		"File panel",
		"panel-dock",
		"File panel side",
		"Show the file tree on the left or the right.",
		"project_panel dock explorer",
	),
	appearance(
		"interface",
		"File panel",
		"panel-indent",
		"Folder indent",
		"How far each folder level is pushed in, in pixels.",
		"indent_size tree",
	),
	appearance(
		"interface",
		"File panel",
		"panel-file-icons",
		"File icons",
		"Show an icon next to each file.",
		"file_icons",
	),
	appearance(
		"interface",
		"File panel",
		"panel-folder-icons",
		"Folder icons",
		"Show an icon next to each folder.",
		"folder_icons",
	),
	appearance(
		"interface",
		"Split panes",
		"pane-border",
		"Active pane border",
		"Outline the pane you are working in, in pixels. 0 hides it.",
		"active_pane_modifiers border_size focus",
	),
	appearance(
		"interface",
		"Split panes",
		"pane-opacity",
		"Dim inactive panes",
		"How visible panes you are not using stay, from 0.2 (faint) to 1 (not dimmed).",
		"inactive_opacity fade",
	),

	appearance(
		"terminal",
		"Renderer",
		"gpu",
		"Fast terminal (GPU)",
		"Draws local terminals on the graphics chip: smoother scrolling and faster output.",
		"gpu renderer native xterm metal",
	),
	appearance(
		"terminal",
		"Renderer",
		"host-restart",
		"Restart terminal host",
		"Restart the background process that keeps terminals alive, to pick up a new Atlas version.",
		"hostd old upgrade",
	),
	appearance(
		"terminal",
		"Text",
		"term-font",
		"Terminal font",
		"Font used by the classic terminal. The fast terminal always uses JetBrains Mono.",
		"font_family xterm typeface",
	),
	appearance(
		"terminal",
		"Text",
		"term-size",
		"Terminal font size",
		"Text size in terminals, in pixels.",
		"font_size",
	),
	appearance(
		"terminal",
		"Text",
		"ligatures",
		"Ligatures",
		"Join characters like -> and != into single symbols (fast terminal only).",
		"ligatures fira",
	),
	appearance(
		"terminal",
		"Text",
		"contrast",
		"Minimum contrast",
		"Brighten or darken text that is hard to read against its background (fast terminal only). 1 turns it off, 4.5 meets accessibility guidance.",
		"wcag accessibility minimum_contrast readable",
	),
	appearance(
		"terminal",
		"Cursor",
		"cursor-shape",
		"Cursor shape",
		"How the cursor looks in terminals.",
		"block bar underline hollow caret",
	),
	appearance(
		"terminal",
		"Cursor",
		"cursor-blink",
		"Blinking cursor",
		"Make the terminal cursor blink.",
		"caret",
	),
	appearance(
		"terminal",
		"Behavior",
		"scrollback",
		"Scrollback",
		"Lines of output each terminal remembers. More lines use more memory.",
		"max_scroll_history_lines history buffer",
	),
	appearance(
		"terminal",
		"Behavior",
		"clipboard",
		"Programs can copy to your clipboard",
		"Lets commands such as tmux or ssh sessions put text on your clipboard.",
		"osc52 osc 52 copy paste ask allow deny",
	),
	appearance(
		"terminal",
		"Behavior",
		"copy-on-select",
		"Copy on select",
		"Copy selected text to the clipboard when you release the mouse (fast terminal only).",
		"copy_on_select clipboard selection ghostty",
	),
	appearance(
		"terminal",
		"Behavior",
		"click-to-move",
		"Click to move the cursor",
		"Click the prompt line to move the cursor there. Needs shell integration (fast terminal only).",
		"click_to_move cursor prompt osc 133 ghostty",
	),
	appearance(
		"terminal",
		"Behavior",
		"option-as-alt",
		"Option key as Alt",
		"Make ⌥ send Alt instead of typing macOS characters (fast terminal only).",
		"option_as_alt macos-option-as-alt meta alt esc ghostty",
	),

	entry(
		"workspaces",
		"New worktrees",
		"ws-dir",
		"Worktree folder",
		"Where Atlas creates checkouts for new worktrees.",
		"workspace directory path location",
	),
	entry(
		"workspaces",
		"New worktrees",
		"ws-nest",
		"Group by repository",
		"Put each worktree in a folder named after its repository.",
		"nest",
	),
	entry(
		"workspaces",
		"New worktrees",
		"ws-prefix",
		"Branch name prefix",
		"Added in front of the branch Atlas creates, so branches are easy to tell apart.",
		"git user.name custom branch",
	),
	entry(
		"workspaces",
		"Clean up",
		"ws-cleanup",
		"Clean up worktrees",
		"See size, last commit and git state of linked worktrees, and delete the ones you no longer need.",
		"delete remove disk scan",
	),

	entry(
		"agents",
		"Coding agents",
		"agent-default",
		"Default agent",
		"What a new agent tab starts.",
		"omp claude codex blank",
	),
	entry(
		"agents",
		"Coding agents",
		"agent-permissions",
		"Tool approval",
		"Choose whether agents ask before running tools or run everything on their own.",
		"permissions yolo manual auto approve",
	),
	entry(
		"agents",
		"Coding agents",
		"agent-args",
		"Extra arguments",
		"Command-line flags added every time that agent starts.",
		"model flags args",
	),
	entry(
		"agents",
		"Coding agents",
		"agent-omp",
		"omp integration",
		"Lets omp report its status and tools to Atlas.",
		"hooks plugin enable",
	),
	entry(
		"agents",
		"Links",
		"browser",
		"Open links in",
		"Where links from services, ports and terminals open.",
		"browser system atlas",
	),
	entry(
		"agents",
		"Notifications",
		"notify",
		"Notifications",
		"Get told when an agent finishes or needs you.",
		"sound alert bell focus",
	),
	entry(
		"agents",
		"Quick commands",
		"quick",
		"Quick commands",
		"One-click buttons in the tab strip that run a command or send a prompt to the agent.",
		"shortcuts prompt buttons",
	),

	entry(
		"keyboard",
		"Shortcuts",
		"keymap",
		"Keyboard shortcuts",
		"Change the key for any command, or remove it.",
		"keybindings keymap hotkeys bind",
	),

	entry(
		"ssh",
		"Hosts",
		"ssh-hosts",
		"SSH hosts",
		"Work on another machine: Atlas starts its helper there and connects over SSH.",
		"remote server hostd tunnel",
	),
	entry(
		"ssh",
		"Remote projects",
		"ssh-project",
		"Remote project",
		"Add a project that lives on one of your SSH hosts.",
		"atlas.yaml path remote",
	),

	entry(
		"help",
		"Help",
		"tour",
		"Guided tour",
		"A short walk through spaces, the header, the panel and search.",
		"onboarding intro",
	),
	entry("help", "Help", "help-keys", "Shortcut cheat sheet", "Every key, searchable.", "keyboard help"),
	entry(
		"help",
		"Help",
		"checklist",
		"Getting started checklist",
		"Bring back the first steps card, or start over.",
		"onboarding progress",
	),
];

const norm = (s: string) => s.toLowerCase().normalize("NFKD").replace(/\p{M}/gu, "");

/**
 * Settings matching `query`: every word must appear in the label, description,
 * group or keywords. Label hits rank above description-only hits.
 */
export function searchSettings(query: string, index: SettingEntry[] = SETTINGS_INDEX): SettingEntry[] {
	const words = norm(query).split(/\s+/).filter(Boolean);
	if (!words.length) return [];
	const scored: { e: SettingEntry; score: number; order: number }[] = [];
	index.forEach((e, order) => {
		const label = norm(e.label);
		const rest = norm(`${e.description} ${e.group} ${e.keywords ?? ""}`);
		let score = 0;
		for (const w of words) {
			if (label.startsWith(w) || label.includes(` ${w}`)) score += 3;
			else if (label.includes(w)) score += 2;
			else if (rest.includes(w)) score += 1;
			else return;
		}
		scored.push({ e, score, order });
	});
	return scored.sort((a, b) => b.score - a.score || a.order - b.order).map((x) => x.e);
}

/** Breadcrumb for a result: `Appearance › Terminal`. */
export function crumb(e: SettingEntry): string {
	const section = SECTIONS.find((s) => s.id === e.section)!.label;
	const tab = e.tab && APPEARANCE_TABS.find((t) => t.id === e.tab)!.label;
	return tab ? `${section} › ${tab}` : section;
}

/**
 * Path of `target` relative to `root` (both absolute, no trailing slash), using `..` to climb out.
 * Editor tabs take worktree-relative paths, so this lets one open a file outside the worktree.
 */
export function relativeFrom(root: string, target: string): string {
	const a = root.split("/").filter(Boolean);
	const b = target.split("/").filter(Boolean);
	let i = 0;
	while (i < a.length && i < b.length && a[i] === b[i]) i++;
	return [...Array<string>(a.length - i).fill(".."), ...b.slice(i)].join("/");
}
