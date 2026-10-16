/**
 * Settings navigation model: which sections exist, the tabs inside the ones that have them and how
 * legacy ids resolve to them. Pure: no Svelte, no IPC.
 */

export type SettingsSection =
	| "general"
	| "appearance"
	| "editor"
	| "terminal"
	| "workspaces"
	| "agents"
	| "git"
	| "keyboard"
	| "ssh"
	| "help";

/** Tabs of the Editor section: everyday editor prefs, and the language-server manager. */
export type SettingsTab = "editor" | "languages";

/** Where Settings opens: a section, plus a tab when the section has tabs. */
export interface SettingsTarget {
	section: SettingsSection;
	tab?: SettingsTab;
}

export const SECTIONS: { id: SettingsSection; label: string }[] = [
	{ id: "general", label: "General" },
	{ id: "appearance", label: "Appearance" },
	{ id: "editor", label: "Editor" },
	{ id: "terminal", label: "Terminal" },
	{ id: "workspaces", label: "Workspaces" },
	{ id: "agents", label: "Agents" },
	{ id: "git", label: "Git" },
	{ id: "keyboard", label: "Keyboard shortcuts" },
	{ id: "ssh", label: "SSH hosts" },
	{ id: "help", label: "Help & onboarding" },
];

/** The sections that are split in tabs; every other section is one page. */
export const SECTION_TABS: Partial<Record<SettingsSection, { id: SettingsTab; label: string }[]>> = {
	editor: [
		{ id: "editor", label: "Editor" },
		{ id: "languages", label: "Languages" },
	],
};

/**
 * Ids used before the reorganisation (and by `updates.show(...)` call sites) mapped to where that content
 * lives now. `appearance/<tab>` pairs are handled in `resolveSection`: Editor, Languages and Terminal were
 * tabs of Appearance and are sections of their own now.
 */
const LEGACY: Record<string, SettingsTarget> = {
	updates: { section: "general" },
	shell: { section: "general" },
};

/** Tabs that used to live under Appearance, and where they are now. */
const MOVED_FROM_APPEARANCE: Record<string, SettingsTarget> = {
	editor: { section: "editor", tab: "editor" },
	languages: { section: "editor", tab: "languages" },
	terminal: { section: "terminal" },
};

const SECTION_IDS = new Set<string>(SECTIONS.map((s) => s.id));

/** A section's target with its first tab, or the tab asked for when the section has it. */
function open(section: SettingsSection, tab?: string): SettingsTarget {
	const tabs = SECTION_TABS[section];
	if (!tabs) return { section };
	return { section, tab: (tabs.find((t) => t.id === tab) ?? tabs[0]!).id };
}

/**
 * Accepts any current or legacy section id, or a `section/tab` pair (`editor/languages`, and the old
 * `appearance/terminal`), and returns the target. Unknown ids fall back to General.
 */
export function resolveSection(id: string): SettingsTarget {
	const [head = "", sub] = id.split("/");
	if (head === "appearance" && sub && Object.hasOwn(MOVED_FROM_APPEARANCE, sub))
		return MOVED_FROM_APPEARANCE[sub]!;
	if (Object.hasOwn(LEGACY, head)) return LEGACY[head]!;
	if (SECTION_IDS.has(head)) return open(head as SettingsSection, sub);
	return { section: "general" };
}
