// Which area (and which surface) a source file's strings belong to. One table, read by `i18n:extract` (the default
// `--area`) and by the migration plan (`i18n:extract --plan`, i18n/MIGRATION.md), so the plan and the codemod cannot
// disagree. The longest matching prefix wins. Areas are topics of at most ~130 strings: the catalog budget is 150 keys
// and 24 KB per area file, and `i18n:check` fails past it.
//
// Rust areas are prefixed by crate (`cmd_` the CLI, `tui_`, `hostd_`, `native_` the desktop shell) and are not `cli_*`:
// those are the generated clap help areas, which the `unused` rule exempts.

const D = "apps/desktop/src/";
const C = `${D}lib/components/`;
const CLI = "crates/atlas-cli/src/";

/** Desktop and site, path prefix (a directory with a trailing slash, or one file) to area. */
const WEB: [string, string][] = [
	// settings
	[`${C}settings/`, "settings_general"],
	[`${C}settings/CaptureSettings.svelte`, "settings_capture"],
	[`${C}settings/UpdatesSection.svelte`, "settings_updates"],
	[`${C}settings/UpdateChannel.svelte`, "settings_updates"],
	[`${C}settings/ReleaseTrain.svelte`, "settings_updates"],
	[`${C}WhatsNew.svelte`, "settings_updates"],
	[`${C}UpdatedCard.svelte`, "settings_updates"],
	[`${C}Settings.svelte`, "settings_general"],
	[`${C}ShortcutSettings.svelte`, "settings_general"],
	[`${C}GitSettings.svelte`, "settings_general"],
	[`${C}Config.svelte`, "settings_general"],
	[`${C}WorkspaceSettings.svelte`, "settings_general"],
	[`${C}AgentSettings.svelte`, "settings_agents"],
	[`${C}CrewSettings.svelte`, "settings_agents"],
	[`${C}MobileSettings.svelte`, "settings_agents"],
	[`${C}SshSettings.svelte`, "settings_agents"],
	[`${C}ShellSettings.svelte`, "settings_terminal"],
	[`${C}EditorSettings.svelte`, "settings_editor"],
	[`${D}lib/settings/`, "settings_general"],
	[`${D}lib/settings/TerminalTab.svelte`, "settings_terminal"],
	[`${D}lib/settings/FontRows.svelte`, "settings_terminal"],
	[`${D}lib/settings/WorktreePreview.svelte`, "settings_terminal"],
	[`${D}lib/settings/EditorRows.svelte`, "settings_editor"],
	[`${D}lib/settings/CodeTab.svelte`, "settings_editor"],
	[`${D}lib/settings/LanguagesTab.svelte`, "settings_editor"],
	[`${D}lib/settings/ServerRow.svelte`, "settings_editor"],
	[`${D}lib/settings/CustomServer.svelte`, "settings_editor"],
	[`${D}lib/settings/theme/`, "settings_theme"],
	[`${D}lib/settings/ThemeTab.svelte`, "settings_theme"],
	[`${D}lib/settings/InterfaceTab.svelte`, "settings_theme"],
	// git and the sidebars
	[`${C}SourceControl.svelte`, "git"],
	[`${C}CreatePrDialog.svelte`, "git"],
	[`${C}DiffView.svelte`, "git"],
	[`${C}PrTab.svelte`, "git"],
	[`${C}IdentityDialog.svelte`, "git"],
	[`${C}NewWorktreeDialog.svelte`, "git"],
	[`${C}NotRepoState.svelte`, "git"],
	[`${C}diff/`, "git"],
	[`${C}Sidebar.svelte`, "sidebar"],
	[`${C}RightSidebar.svelte`, "sidebar"],
	[`${C}sidebar/`, "sidebar"],
	// workbench: tabs, panes, editor, search, logs, panel (also the fallback of a desktop file no other rule covers)
	[D, "workbench"],
	[`${D}routes/`, "shell"],
	[`${D}lib/editor/`, "workbench"],
	[`${D}lib/lsp/`, "workbench"],
	[`${D}lib/search/`, "search"],
	[`${C}logs/`, "logs"],
	[`${C}Logs.svelte`, "logs"],
	[`${D}lib/panel/`, "panel"],
	[`${C}mobile/`, "mobile"],
	[`${C}PairQr.svelte`, "mobile"],
	[`${D}lib/onboarding/`, "onboarding"],
	[`${C}Welcome.svelte`, "onboarding"],
	[`${C}SetupRow.svelte`, "onboarding"],
	[`${D}lib/viewers/`, "viewers_files"],
	...["Atlas", "Compose", "Dockerfile", "Env", "Make", "Workflow"].map((v): [string, string] => [`${D}lib/viewers/rich/${v}View.svelte`, "viewers_devops"]),
	[`${D}lib/shell/`, "shell"],
	[`${D}lib/errors/`, "shell"],
	[`${D}routes/(app)/`, "shell"],
	[`${D}routes/tray/`, "tray"],
	[`${D}routes/dock/`, "tray"],
	[`${D}routes/shots/`, "capture"],
	// crew
	[`${D}lib/crew/`, "crew_workspace"],
	[`${D}lib/crew/settings/`, "crew_settings"],
	[`${D}lib/crew/automation/`, "crew_automation"],
	[`${D}lib/crew/onboarding/`, "crew_onboarding"],
	[`${D}lib/crew/chat/`, "crew_chat"],
	[`${D}routes/crew-demo/`, "crew_workspace"],
	// site
	["site/src/", "islands"],
];

/** Rust, path prefix to area: one area per command family or crate. */
const RUST: [string, string][] = [
	[CLI, "cmd_misc"],
	[`${CLI}commands/self_update.rs`, "cmd_update"],
	[`${CLI}commands/update_`, "cmd_update"],
	[`${CLI}commands/install.rs`, "cmd_install"],
	[`${CLI}commands/uninstall.rs`, "cmd_install"],
	[`${CLI}commands/version.rs`, "cmd_install"],
	[`${CLI}commands/link.rs`, "cmd_link"],
	[`${CLI}commands/open.rs`, "cmd_link"],
	[`${CLI}commands/wt.rs`, "cmd_link"],
	[`${CLI}commands/crew`, "cmd_crew"],
	[`${CLI}commands/deploy/`, "cmd_deploy"],
	[`${CLI}commands/desktop`, "cmd_desktop"],
	[`${CLI}commands/hostd`, "cmd_hostd"],
	[`${CLI}commands/setup/`, "cmd_setup"],
	[`${CLI}commands/skills/`, "cmd_skills"],
	["crates/atlas-tui/src/", "tui_runtime"],
	["crates/atlas-host/src/", "hostd_daemon"],
	["crates/atlas-desktop/src/", "native_shell"],
];

/** The longest matching prefix of `file`, or undefined. */
function longest(table: [string, string][], file: string): string | undefined {
	let best: [string, string] | undefined;
	for (const row of table) if (file.startsWith(row[0]) && (!best || row[0].length > best[0].length)) best = row;
	return best?.[1];
}

/** The area `file`'s strings go to by default, or undefined for a file no rule covers. */
export const areaOf = (file: string): string | undefined => longest(file.endsWith(".rs") ? RUST : WEB, file);

export type Surface = "desktop" | "site" | "rust";

/** The surface that consumes the strings of `file` (the Paraglide project or the Rust crates). */
export function surfaceOf(file: string): Surface | undefined {
	if (file.startsWith("apps/desktop/src/")) return "desktop";
	if (file.startsWith("site/src/")) return "site";
	if (file.startsWith("crates/")) return "rust";
	return undefined;
}

/** `lib/components/settings/CaptureSettings.svelte` is `capture_settings`; `routes/(app)/+page.svelte` is `app_page`. */
export function stemOf(file: string): string {
	const parts = file.replace(/\.[a-z]+$/, "").split("/");
	let name = parts[parts.length - 1];
	if (name.startsWith("+")) name = `${parts[parts.length - 2] ?? ""}_${name.slice(1)}`;
	return name
		.replace(/[^A-Za-z0-9]+/g, "_")
		.replace(/([a-z0-9])([A-Z])/g, "$1_$2")
		.replace(/([A-Z])([A-Z][a-z])/g, "$1_$2")
		.toLowerCase()
		.replace(/^_+|_+$/g, "");
}
