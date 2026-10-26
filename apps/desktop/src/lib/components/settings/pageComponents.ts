/**
 * The component that renders each page of the registry: `<section>` for a single-page section,
 * `<section>/<tab>` for the tabs of a tabbed one. Typed against the registry, so a page without a component (or
 * a component without a page) is a type error, and `registry.test.ts` pins it at runtime too.
 * Keep one `key: Component,` per line: the test reads this file.
 */
import type { Component } from "svelte";
import HelpSettings from "#lib/onboarding/HelpSettings.svelte";
import LanguagesTab from "#lib/settings/LanguagesTab.svelte";
import type { RegistryDefs } from "#lib/settings/index.ts";
import AgentSettings from "../AgentSettings.svelte";
import AppearanceSettings from "../AppearanceSettings.svelte";
import EditorSettings from "../EditorSettings.svelte";
import GitSettings from "../GitSettings.svelte";
import MobileSettings from "../MobileSettings.svelte";
import ShortcutSettings from "../ShortcutSettings.svelte";
import SshSettings from "../SshSettings.svelte";
import TerminalSettings from "../TerminalSettings.svelte";
import WorkspaceSettings from "../WorkspaceSettings.svelte";
import AgentHooks from "./AgentHooks.svelte";
import NotificationsPage from "./NotificationsPage.svelte";
import ShellLinks from "./ShellLinks.svelte";
import UpdatesSection from "./UpdatesSection.svelte";

/** The key of a page: a single-page section's id, or `section/tab` for each tab of a tabbed one. */
export type PageKey = RegistryDefs[number] extends infer C
	? C extends { id: infer S extends string; pages: readonly (infer P)[] }
		? P extends { id: infer T extends string }
			? `${S}/${T}`
			: S
		: never
	: never;

export const PAGE_COMPONENTS: Record<PageKey, Component> = {
	general: UpdatesSection,
	appearance: AppearanceSettings,
	notifications: NotificationsPage,
	keyboard: ShortcutSettings,
	help: HelpSettings,
	workspaces: WorkspaceSettings,
	"editor/editor": EditorSettings,
	"editor/languages": LanguagesTab,
	terminal: TerminalSettings,
	git: GitSettings,
	agents: AgentSettings,
	"integrations/shell": ShellLinks,
	"integrations/agents": AgentHooks,
	"integrations/ssh": SshSettings,
	"integrations/mobile": MobileSettings,
};
