/**
 * The source files that render each Settings page, relative to `src/lib`. `placement.test.ts` reads them to
 * check that the catalog files every setting under the page that really shows it. Keep in step with the page
 * components in `components/Settings.svelte`.
 */
import type { SettingsSection, SettingsTab } from "./sections.ts";

export const PAGES: { section: SettingsSection; tab?: SettingsTab; files: string[] }[] = [
	{
		section: "general",
		files: [
			"components/settings/UpdatesSection.svelte",
			"components/settings/UpdateChannel.svelte",
			"components/ShellSettings.svelte",
			"components/settings/LinksNotifications.svelte",
			"components/settings/SoundSettings.svelte",
			"components/Settings.svelte",
		],
	},
	{ section: "appearance", files: ["settings/ThemeTab.svelte", "settings/InterfaceTab.svelte"] },
	{
		section: "editor",
		tab: "editor",
		files: ["settings/CodeTab.svelte", "settings/EditorRows.svelte", "settings/FontRows.svelte"],
	},
	{
		section: "editor",
		tab: "languages",
		files: ["settings/LanguagesTab.svelte", "settings/ServerRow.svelte"],
	},
	{ section: "terminal", files: ["settings/TerminalTab.svelte"] },
	{ section: "workspaces", files: ["components/WorkspaceSettings.svelte"] },
	{ section: "agents", files: ["components/AgentSettings.svelte"] },
	{ section: "git", files: ["components/GitSettings.svelte"] },
	{ section: "keyboard", files: ["components/ShortcutSettings.svelte"] },
	{ section: "ssh", files: ["components/SshSettings.svelte"] },
	{ section: "help", files: ["onboarding/HelpSettings.svelte"] },
];
