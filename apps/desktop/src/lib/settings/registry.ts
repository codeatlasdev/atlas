/**
 * The Settings registry: the ONE declarative description of what Settings contains.
 *
 *   category (sidebar entry, page title) -> page (a tab, or the whole page) -> group (a card) -> item (a row)
 *
 * The sidebar, the page header, the tab strip, the search index, the breadcrumb of a result and every
 * `updates.show("<id>")` deep link are all derived from this file, so they cannot disagree. Adding a setting is
 * one `messageItem(...)` here plus its row in the page component named in `files`; `registry.test.ts` and
 * `placement.test.ts` go red when either half is missing.
 *
 * Rules the tests pin:
 *  - ids never change when a setting moves (search, deep links and the flash on arrival key on them);
 *  - an `aliases` entry keeps an id that shipped before working after the thing it named moved or was renamed;
 *  - labels and descriptions are identical to the screens;
 *  - every user-visible text is a message read through a getter, so the sidebar, the page, the tab strip and the
 *    search follow the display language live (nothing here may copy a label into a plain string at module load).
 * Pure: no Svelte, no IPC. Icons and components live next to the UI (`SettingsNav`, `pageComponents.ts`).
 */
import { kbd } from "#lib/keys.ts";
import { m } from "#lib/i18n/m.ts";
import { CATEGORIES as SOUND_CATEGORIES, SPECS } from "#lib/sound/policy.ts";

/** One row. `keywords` are words people may type that the label does not contain. */
export interface ItemDef {
	readonly id: string;
	readonly label: string;
	readonly description: string;
	readonly keywords?: string;
	/** Older deep-link ids for this row. */
	readonly aliases?: readonly string[];
	/** Only exists on macOS (the native GPU terminal, capture): hidden from nav, search and deep links elsewhere. */
	readonly macOnly?: true;
}

/** A card of rows under a title. */
export interface GroupDef {
	readonly title: string;
	readonly items: readonly ItemDef[];
}

/** A tab of a category, or (without `id`/`label`) the whole page of a single-page category. */
export interface PageDef {
	readonly id?: string;
	readonly label?: string;
	/** One line under the tab strip. Required for tabs. */
	readonly description?: string;
	readonly aliases?: readonly string[];
	/** Sources that render this page, relative to `src/lib`; `placement.test.ts` reads them. */
	readonly files: readonly string[];
	readonly groups: readonly GroupDef[];
}

export interface CategoryDef {
	readonly id: string;
	readonly label: string;
	/** One line: the page header, and the tooltip of the sidebar entry. */
	readonly description: string;
	/** Sidebar heading the entry sits under. */
	readonly cluster: "app" | "work" | "connect";
	/** Older deep-link ids for the whole category (`appearance/terminal`). */
	readonly aliases?: readonly string[];
	/** The whole category is macOS only (capture): no sidebar entry, no search hits, deep links fall back. */
	readonly macOnly?: true;
	readonly pages: readonly PageDef[];
}

/**
 * Sidebar headings, in order. The label is a getter: it is read when the sidebar draws, so it follows the display
 * language.
 */
export const CLUSTERS = [
	{
		id: "app",
		get label() {
			return m.settings_registry_clusters_app();
		},
	},
	{
		id: "work",
		get label() {
			return m.settings_registry_clusters_work();
		},
	},
	{
		id: "connect",
		get label() {
			return m.settings_registry_clusters_connect();
		},
	},
] as const;

/**
 * A row whose label and description come from the message catalog. They are getters, so they are read when something
 * shows them (the page, the search index) and follow the display language. `keywords` stay a plain string on
 * purpose: they hold words in every language people may type.
 */
const messageItem = (
	id: string,
	label: () => string,
	description: () => string,
	keywords = "",
	...aliases: string[]
): ItemDef => ({
	id,
	get label() {
		return label();
	},
	get description() {
		return description();
	},
	...(keywords && { keywords }),
	...(aliases.length && { aliases }),
});

/**
 * A row that only exists on macOS (the fast GPU terminal's own options): see [`forPlatform`]. Not a spread: a spread
 * would freeze the message-backed label and description at their current language.
 */
const macItem = (...a: Parameters<typeof messageItem>): ItemDef =>
	Object.defineProperty(messageItem(...a), "macOnly", { value: true, enumerable: true });

/** One row per sound category; the ids and texts come from the sound policy so the two lists stay one. */
const SOUND_ITEMS = SOUND_CATEGORIES.map((c) =>
	messageItem(
		`sound-cat-${c}`,
		() => SPECS[c].label,
		() => SPECS[c].description,
		"sound earcon chime",
	),
);

const DEFS = [
	{
		id: "general",
		get label() {
			return m.settings_registry_general_label();
		},
		get description() {
			return m.settings_registry_general_description();
		},
		cluster: "app",
		pages: [
			{
				files: [
					"components/settings/UpdatesSection.svelte",
					"components/settings/UpdateChannel.svelte",
					"components/settings/HostStatusRow.svelte",
					"components/settings/DisplayLanguage.svelte",
				],
				groups: [
					{
						get title() {
							return m.settings_registry_general_group_updates();
						},
						items: [
							messageItem(
								"updates",
								m.settings_registry_general_group_updates,
								m.settings_registry_general_updates_description,
								"version upgrade release cli app",
							),
							messageItem(
								"update-channel",
								m.settings_registry_general_update_channel_label,
								m.settings_registry_general_update_channel_description,
								"beta alpha stable prerelease pre-release rc nightly insiders downgrade release train",
							),
						],
					},
					{
						get title() {
							return m.settings_group_language();
						},
						items: [
							messageItem(
								"display-language",
								m.settings_display_language_label,
								m.settings_display_language_description,
								"language idioma portugues locale translation",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "appearance",
		get label() {
			return m.settings_registry_appearance_label();
		},
		get description() {
			return m.settings_registry_appearance_description();
		},
		cluster: "app",
		pages: [
			{
				files: ["settings/ThemeTab.svelte", "settings/InterfaceTab.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_appearance_group_color_theme();
						},
						items: [
							messageItem(
								"theme-mode",
								m.settings_registry_appearance_theme_mode_label,
								m.settings_registry_appearance_theme_mode_description,
								"dark light mode system",
							),
							messageItem(
								"theme-light",
								m.settings_registry_appearance_theme_light_label,
								m.settings_registry_appearance_theme_light_description,
								"alucard zed json",
							),
							messageItem(
								"theme-dark",
								m.settings_registry_appearance_theme_dark_label,
								m.settings_registry_appearance_theme_dark_description,
								"dracula zed json",
							),
							messageItem(
								"theme-terminal",
								m.settings_registry_appearance_theme_terminal_label,
								m.settings_registry_appearance_theme_terminal_description,
								"dracula alucard colors ansi palette",
							),
							messageItem(
								"theme-import",
								m.settings_registry_appearance_theme_import_label,
								m.settings_registry_appearance_theme_import_description,
								"ghostty iterm itermcolors vscode vsix base16 alacritty yaml toml zip package folder dracula pro zeno alucard import zed",
							),
							messageItem(
								"theme-export",
								m.settings_registry_appearance_theme_export_label,
								m.settings_registry_appearance_theme_export_description,
								"ghostty iterm vscode base16 dracula alucard export share copy save reveal folder",
							),
							messageItem(
								"accent",
								m.settings_registry_appearance_accent_label,
								m.settings_registry_appearance_accent_description,
								"brand color swatch",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_appearance_group_zoom();
						},
						items: [
							messageItem(
								"ui-zoom",
								m.settings_registry_appearance_group_zoom,
								() =>
									m.settings_registry_appearance_ui_zoom_description({
										zoomIn: kbd("cmd-="),
										zoomOut: kbd("cmd--"),
										reset: kbd("cmd-0"),
									}),
								"scale magnify bigger smaller zoom in out reset layout size",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_appearance_group_interface_font();
						},
						items: [
							messageItem(
								"ui-font",
								m.settings_registry_appearance_group_interface_font,
								m.settings_registry_appearance_ui_font_description,
								"ui_font_family typeface",
							),
							messageItem(
								"ui-size",
								m.settings_registry_appearance_ui_size_label,
								m.settings_registry_appearance_ui_size_description,
								"ui_font_size",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_appearance_group_file_panel();
						},
						items: [
							messageItem(
								"panel-dock",
								m.settings_registry_appearance_panel_dock_label,
								m.settings_registry_appearance_panel_dock_description,
								"project_panel dock explorer",
							),
							messageItem(
								"panel-indent",
								m.settings_registry_appearance_panel_indent_label,
								m.settings_registry_appearance_panel_indent_description,
								"indent_size tree",
							),
							messageItem(
								"panel-file-icons",
								m.settings_registry_appearance_panel_file_icons_label,
								m.settings_registry_appearance_panel_file_icons_description,
								"file_icons",
							),
							messageItem(
								"panel-folder-icons",
								m.settings_registry_appearance_panel_folder_icons_label,
								m.settings_registry_appearance_panel_folder_icons_description,
								"folder_icons",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_appearance_group_split_panes();
						},
						items: [
							messageItem(
								"pane-border",
								m.settings_registry_appearance_pane_border_label,
								m.settings_registry_appearance_pane_border_description,
								"active_pane_modifiers border_size focus",
							),
							messageItem(
								"pane-opacity",
								m.settings_registry_appearance_pane_opacity_label,
								m.settings_registry_appearance_pane_opacity_description,
								"inactive_opacity fade",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "notifications",
		get label() {
			return m.settings_registry_notifications_label();
		},
		get description() {
			return m.settings_registry_notifications_description();
		},
		cluster: "app",
		pages: [
			{
				files: [
					"components/settings/NotificationSettings.svelte",
					"components/settings/SoundSettings.svelte",
				],
				groups: [
					{
						get title() {
							return m.settings_general_notification_notifications();
						},
						items: [
							messageItem(
								"notify",
								m.settings_general_notification_notifications,
								m.settings_registry_notifications_notify_description,
								"sound alert bell focus",
							),
						],
					},
					{
						get title() {
							return m.settings_general_sound_sounds();
						},
						items: [
							messageItem(
								"sounds",
								m.settings_general_sound_sounds,
								m.settings_registry_notifications_sounds_description,
								"audio sound effects earcon chime notification mute silent",
							),
							messageItem(
								"sound-volume",
								m.settings_general_sound_sound_volume,
								m.settings_registry_notifications_sound_volume_description,
								"loudness quiet audio level",
							),
							messageItem(
								"sound-reduce",
								m.settings_general_sound_reduce_sounds,
								m.settings_registry_notifications_sound_reduce_description,
								"quiet minimal less fewer",
							),
							...SOUND_ITEMS,
						],
					},
				],
			},
		],
	},
	{
		id: "capture",
		get label() {
			return m.settings_registry_capture_label();
		},
		get description() {
			return m.settings_registry_capture_description();
		},
		cluster: "app",
		macOnly: true,
		pages: [
			{
				files: ["components/settings/CaptureSettings.svelte"],
				groups: [
					{
						get title() {
							return m.capture_quick_page_capture();
						},
						items: [
							messageItem(
								"capture-permission",
								m.settings_registry_capture_capture_permission_label,
								m.settings_registry_capture_capture_permission_description,
								"capture screenshot screen recording permission privacy grant system settings",
							),
							messageItem(
								"capture-enabled",
								m.settings_registry_capture_capture_enabled_label,
								m.settings_registry_capture_capture_enabled_description,
								"capture screenshot screen recording gif hotkey enable disable",
							),
							messageItem(
								"capture-folder",
								m.settings_registry_capture_capture_folder_label,
								m.settings_registry_capture_capture_folder_description,
								"capture screenshot screen recording gif folder directory pictures path",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_capture_group_hotkeys();
						},
						items: [
							messageItem(
								"capture-preset",
								m.settings_registry_capture_capture_preset_label,
								m.settings_registry_capture_capture_preset_description,
								"capture screenshot hotkey preset macos cleanshot shortcuts keys",
							),
							messageItem(
								"capture-system-keys",
								m.settings_registry_capture_capture_system_keys_label,
								() =>
									m.settings_registry_capture_capture_system_keys_description({
										screen: kbd("cmd-shift-3"),
										area: kbd("cmd-shift-4"),
										toolbar: kbd("cmd-shift-5"),
										touchBar: kbd("cmd-shift-6"),
									}),
								"capture screenshot macos system shortcuts keyboard shift command 3 4 5 6 disable free",
							),
							messageItem(
								"capture-key-all-in-one",
								m.settings_registry_capture_capture_key_all_in_one_label,
								m.settings_registry_capture_capture_key_all_in_one_description,
								"capture screenshot all in one toolbar menu hotkey shortcut",
							),
							messageItem(
								"capture-key-region",
								m.capture_overlay_page_mode_region,
								m.settings_registry_capture_capture_key_region_description,
								"capture screenshot screen region area hotkey shortcut",
							),
							messageItem(
								"capture-key-previous",
								m.capture_overlay_page_previous_area,
								m.settings_registry_capture_capture_key_previous_description,
								"capture screenshot previous last area repeat region hotkey shortcut",
							),
							messageItem(
								"capture-key-window",
								m.capture_overlay_page_mode_window,
								m.settings_registry_capture_capture_key_window_description,
								"capture screenshot window hotkey shortcut",
							),
							messageItem(
								"capture-key-fullscreen",
								m.capture_overlay_page_mode_screen,
								m.settings_registry_capture_capture_key_fullscreen_description,
								"capture screenshot fullscreen full screen display hotkey shortcut",
							),
							messageItem(
								"capture-key-ocr",
								m.capture_quick_page_ocr,
								m.settings_registry_capture_capture_key_ocr_description,
								"capture screenshot text ocr recognize copy hotkey shortcut",
							),
							messageItem(
								"capture-key-record",
								m.capture_overlay_page_mode_record,
								m.settings_registry_capture_capture_key_record_description,
								"capture screen recording video record hotkey shortcut",
							),
							messageItem(
								"capture-key-gif",
								m.settings_registry_capture_capture_key_gif_label,
								m.settings_registry_capture_capture_key_gif_description,
								"capture screen recording gif animated hotkey shortcut",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_capture_group_screenshots();
						},
						items: [
							messageItem(
								"capture-on-release",
								m.settings_registry_capture_capture_on_release_label,
								m.settings_registry_capture_capture_on_release_description,
								"capture screenshot region release mouse instant enter return confirm adjust",
							),
							messageItem(
								"capture-timer",
								m.settings_registry_capture_capture_timer_label,
								m.settings_registry_capture_capture_timer_description,
								"capture screenshot timer delay countdown seconds",
							),
							messageItem(
								"capture-scale",
								m.settings_registry_capture_capture_scale_label,
								m.settings_registry_capture_capture_scale_description,
								"capture screenshot resolution retina scale 1x 2x pixels size",
							),
							messageItem(
								"capture-hide-icons",
								m.settings_registry_capture_capture_hide_icons_label,
								m.settings_registry_capture_capture_hide_icons_description,
								"capture screenshot screen recording desktop icons hide clean",
							),
							messageItem(
								"capture-window-shadow",
								m.settings_registry_capture_capture_window_shadow_label,
								m.settings_registry_capture_capture_window_shadow_description,
								"capture screenshot window shadow transparent",
							),
							messageItem(
								"capture-window-padding",
								m.settings_registry_capture_capture_window_padding_label,
								m.settings_registry_capture_capture_window_padding_description,
								"capture screenshot window background padding backdrop",
							),
						],
					},
					{
						get title() {
							return m.capture_quick_title_recording();
						},
						items: [
							messageItem(
								"capture-quality",
								m.settings_registry_capture_capture_quality_label,
								m.settings_registry_capture_capture_quality_description,
								"capture screen recording video quality high medium low size bitrate",
							),
							messageItem(
								"capture-clicks",
								m.settings_registry_capture_capture_clicks_label,
								m.settings_registry_capture_capture_clicks_description,
								"capture screen recording gif click ring mouse highlight",
							),
							messageItem(
								"capture-keystrokes",
								m.settings_registry_capture_capture_keystrokes_label,
								m.settings_registry_capture_capture_keystrokes_description,
								"capture screen recording keystrokes keys keyboard pressed input monitoring",
							),
							messageItem(
								"capture-system-audio",
								m.settings_registry_capture_capture_system_audio_label,
								m.settings_registry_capture_capture_system_audio_description,
								"capture screen recording system sound audio computer",
							),
							messageItem(
								"capture-microphone",
								m.settings_registry_capture_capture_microphone_label,
								m.settings_registry_capture_capture_microphone_description,
								"capture screen recording microphone mic voice audio narration",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_capture_group_after_a_capture();
						},
						items: [
							messageItem(
								"capture-copy",
								m.capture_editor_page_copy_to_clipboard,
								m.settings_registry_capture_capture_copy_description,
								"capture screenshot clipboard copy",
							),
							messageItem(
								"capture-quick-access",
								m.settings_registry_capture_capture_quick_access_label,
								m.settings_registry_capture_capture_quick_access_description,
								"capture screenshot quick access overlay thumbnail card annotate",
							),
							messageItem(
								"capture-dismiss",
								m.settings_registry_capture_capture_dismiss_label,
								m.settings_registry_capture_capture_dismiss_description,
								"capture screenshot quick access overlay seconds timer never",
							),
							messageItem(
								"capture-corner",
								m.settings_registry_capture_capture_corner_label,
								m.settings_registry_capture_capture_corner_description,
								"capture screenshot quick access overlay corner position bottom top left right",
							),
							messageItem(
								"capture-stack",
								m.settings_registry_capture_capture_stack_label,
								m.settings_registry_capture_capture_stack_description,
								"capture screenshot quick access overlay stack cards number limit",
							),
							messageItem(
								"capture-sound",
								m.settings_registry_capture_capture_sound_label,
								m.settings_registry_capture_capture_sound_description,
								"capture screenshot sound shutter audio",
							),
							messageItem(
								"capture-highlight",
								m.settings_registry_capture_capture_highlight_label,
								m.settings_registry_capture_capture_highlight_description,
								"capture screen recording gif cursor pointer click highlight",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_capture_group_history();
						},
						items: [
							messageItem(
								"capture-history-size",
								m.settings_registry_capture_capture_history_size_label,
								m.settings_registry_capture_capture_history_size_description,
								"capture screenshot screen recording gif history size limit keep",
							),
							messageItem(
								"capture-history",
								m.settings_registry_capture_capture_history_label,
								m.settings_registry_capture_capture_history_description,
								"capture screenshot screen recording gif history list recent quick access folder",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "keyboard",
		get label() {
			return m.settings_registry_keyboard_label();
		},
		get description() {
			return m.settings_registry_keyboard_description();
		},
		cluster: "app",
		pages: [
			{
				files: ["components/ShortcutSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_keyboard_group_shortcuts();
						},
						items: [
							messageItem(
								"keymap",
								m.settings_registry_keyboard_label,
								m.settings_registry_keyboard_description,
								"keybindings keymap hotkeys bind",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "help",
		get label() {
			return m.settings_registry_help_label();
		},
		get description() {
			return m.settings_registry_help_description();
		},
		cluster: "app",
		pages: [
			{
				files: ["onboarding/HelpSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_help_group_help();
						},
						items: [
							messageItem(
								"tour",
								m.settings_registry_help_tour_label,
								m.settings_registry_help_tour_description,
								"onboarding intro",
							),
							messageItem(
								"help-keys",
								m.settings_registry_help_help_keys_label,
								m.settings_registry_help_help_keys_description,
								"keyboard help",
							),
							messageItem(
								"checklist",
								m.settings_registry_help_checklist_label,
								m.settings_registry_help_checklist_description,
								"onboarding progress",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "workspaces",
		get label() {
			return m.settings_registry_workspaces_label();
		},
		get description() {
			return m.settings_registry_workspaces_description();
		},
		cluster: "work",
		pages: [
			{
				files: ["components/WorkspaceSettings.svelte", "components/settings/HygieneGroup.svelte"],
				groups: [
					{
						get title() {
							return m.settings_general_workspace_new_worktrees();
						},
						items: [
							messageItem(
								"ws-dir",
								m.settings_general_workspace_worktree_folder,
								m.settings_registry_workspaces_ws_dir_description,
								"workspace directory path location",
							),
							messageItem(
								"ws-nest",
								m.settings_general_workspace_group_by_repository,
								m.settings_registry_workspaces_ws_nest_description,
								"nest",
							),
							messageItem(
								"ws-prefix",
								m.settings_general_workspace_branch_name_prefix,
								m.settings_registry_workspaces_ws_prefix_description,
								"git user.name custom branch",
							),
						],
					},
					{
						get title() {
							return m.settings_general_workspace_auto_clean_up();
						},
						items: [
							messageItem(
								"ws-autoclean",
								m.settings_general_workspace_remove_worktrees_automatically,
								m.settings_registry_workspaces_ws_autoclean_description,
								"auto delete merged idle stale prune restore",
							),
						],
					},
					{
						get title() {
							return m.settings_general_workspace_clean_up();
						},
						items: [
							messageItem(
								"ws-cleanup",
								m.settings_registry_workspaces_ws_cleanup_label,
								m.settings_registry_workspaces_ws_cleanup_description,
								"delete remove disk scan",
							),
						],
					},
					{
						get title() {
							return m.settings_general_hygiene_group_hygiene();
						},
						items: [
							messageItem(
								"ws-hygiene",
								m.settings_general_hygiene_group_orphans_and_disk,
								m.settings_registry_workspaces_ws_hygiene_description,
								"orphan zombie leftover browser chromium vite process kill disk space cache temp spotlight index mdworker cleanup",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "editor",
		get label() {
			return m.settings_registry_editor_label();
		},
		get description() {
			return m.settings_registry_editor_description();
		},
		cluster: "work",
		aliases: ["appearance/editor"],
		pages: [
			{
				id: "editor",
				get label() {
					return m.settings_registry_editor_label();
				},
				get description() {
					return m.settings_registry_editor_tab_editor_description();
				},
				files: ["settings/CodeTab.svelte", "settings/EditorRows.svelte", "settings/FontRows.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_editor_group_code_font();
						},
						items: [
							messageItem(
								"buffer-font",
								m.settings_registry_editor_group_code_font,
								m.settings_registry_editor_buffer_font_description,
								"buffer_font_family monospace typeface",
							),
							messageItem(
								"buffer-size",
								m.settings_registry_editor_buffer_size_label,
								m.settings_registry_editor_buffer_size_description,
								"buffer_font_size",
							),
							messageItem(
								"buffer-line-height",
								m.settings_registry_editor_buffer_line_height_label,
								m.settings_registry_editor_buffer_line_height_description,
								"buffer_line_height leading",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_editor_group_language_servers();
						},
						items: [
							messageItem(
								"lsp-enable",
								m.settings_registry_editor_group_language_servers,
								m.settings_registry_editor_lsp_enable_description,
								"lsp enable_language_servers rust-analyzer gopls typescript intellisense autocomplete",
							),
							messageItem(
								"lsp-format-on-save",
								m.settings_registry_editor_lsp_format_on_save_label,
								m.settings_registry_editor_lsp_format_on_save_description,
								"format_on_save formatter prettier rustfmt",
							),
							messageItem(
								"lsp-inlay-hints",
								m.settings_registry_editor_lsp_inlay_hints_label,
								m.settings_registry_editor_lsp_inlay_hints_description,
								"inlay_hints types parameters",
							),
							messageItem(
								"lsp-manage",
								m.settings_registry_editor_lsp_manage_label,
								m.settings_registry_editor_lsp_manage_description,
								"languages custom command lsp gopls rust-analyzer typescript server install uninstall",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_editor_group_editing();
						},
						items: [
							messageItem(
								"editor-tab-size",
								m.settings_registry_editor_editor_tab_size_label,
								m.settings_registry_editor_editor_tab_size_description,
								"tab_size indent indentation width",
							),
							messageItem(
								"editor-insert-spaces",
								m.settings_registry_editor_editor_insert_spaces_label,
								m.settings_registry_editor_editor_insert_spaces_description,
								"insert_spaces tabs spaces indentation",
							),
							messageItem(
								"editor-trim-whitespace",
								m.settings_registry_editor_editor_trim_whitespace_label,
								m.settings_registry_editor_editor_trim_whitespace_description,
								"trim_trailing_whitespace_on_save strip",
							),
							messageItem(
								"editor-final-newline",
								m.settings_registry_editor_editor_final_newline_label,
								m.settings_registry_editor_editor_final_newline_description,
								"insert_final_newline_on_save eof end of file",
							),
							messageItem(
								"editor-auto-reload",
								m.settings_registry_editor_editor_auto_reload_label,
								m.settings_registry_editor_editor_auto_reload_description,
								"auto_reload external changes refresh watch",
							),
							messageItem(
								"editor-large-file",
								m.settings_registry_editor_editor_large_file_label,
								m.settings_registry_editor_editor_large_file_description,
								"large_file_threshold_bytes performance slow big huge",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_editor_group_display();
						},
						items: [
							messageItem(
								"editor-word-wrap",
								m.settings_registry_editor_editor_word_wrap_label,
								m.settings_registry_editor_editor_word_wrap_description,
								"word_wrap soft wrap long lines",
							),
							messageItem(
								"editor-line-numbers",
								m.settings_registry_editor_editor_line_numbers_label,
								m.settings_registry_editor_editor_line_numbers_description,
								"line_numbers gutter",
							),
							messageItem(
								"editor-highlight-line",
								m.settings_registry_editor_editor_highlight_line_label,
								m.settings_registry_editor_editor_highlight_line_description,
								"highlight_active_line cursor line",
							),
							messageItem(
								"editor-whitespace",
								m.settings_registry_editor_editor_whitespace_label,
								m.settings_registry_editor_editor_whitespace_description,
								"render_whitespace invisible characters",
							),
							messageItem(
								"editor-indent-guides",
								m.settings_registry_editor_editor_indent_guides_label,
								m.settings_registry_editor_editor_indent_guides_description,
								"indent_guides",
							),
							messageItem(
								"editor-bracket-colors",
								m.settings_registry_editor_editor_bracket_colors_label,
								m.settings_registry_editor_editor_bracket_colors_description,
								"bracket_pair_colorization rainbow brackets",
							),
							messageItem(
								"editor-sticky-scroll",
								m.settings_registry_editor_editor_sticky_scroll_label,
								m.settings_registry_editor_editor_sticky_scroll_description,
								"sticky_scroll scope header",
							),
							messageItem(
								"editor-minimap",
								m.settings_registry_editor_editor_minimap_label,
								m.settings_registry_editor_editor_minimap_description,
								"overview",
							),
							messageItem(
								"editor-rulers",
								m.settings_registry_editor_editor_rulers_label,
								m.settings_registry_editor_editor_rulers_description,
								"column guide line length 80 100 120",
							),
							messageItem(
								"editor-scroll-past-end",
								m.settings_registry_editor_editor_scroll_past_end_label,
								m.settings_registry_editor_editor_scroll_past_end_description,
								"scroll_past_end",
							),
							messageItem(
								"editor-smooth-scroll",
								m.settings_registry_editor_editor_smooth_scroll_label,
								m.settings_registry_editor_editor_smooth_scroll_description,
								"smooth_scrolling animation",
							),
						],
					},
				],
			},
			{
				id: "languages",
				get label() {
					return m.settings_registry_editor_tab_languages_label();
				},
				get description() {
					return m.settings_registry_editor_tab_languages_description();
				},
				aliases: ["appearance/languages", "languages"],
				files: ["settings/LanguagesTab.svelte", "settings/ServerRow.svelte", "settings/CustomServer.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_editor_group_language_servers();
						},
						items: [
							messageItem(
								"lsp-servers",
								m.settings_registry_editor_group_language_servers,
								m.settings_registry_editor_lsp_servers_description,
								"language server typescript gopls rust-analyzer pyright clangd install uninstall update restart log path managed",
							),
							messageItem(
								"lsp-custom",
								m.settings_registry_editor_lsp_custom_label,
								m.settings_registry_editor_lsp_custom_description,
								"custom command args arguments initialization options json override replace binary lsp typescript gopls rust-analyzer",
							),
							messageItem(
								"lsp-project",
								m.settings_registry_editor_lsp_project_label,
								m.settings_registry_editor_lsp_project_description,
								"project override .atlas workspace lsp",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "terminal",
		get label() {
			return m.settings_registry_terminal_label();
		},
		get description() {
			return m.settings_registry_terminal_description();
		},
		cluster: "work",
		aliases: ["appearance/terminal"],
		pages: [
			{
				files: ["settings/TerminalTab.svelte"],
				groups: [
					{
						get title() {
							return m.settings_terminal_tab_renderer();
						},
						items: [
							macItem(
								"gpu",
								m.settings_terminal_tab_fast_terminal_gpu,
								m.settings_registry_terminal_gpu_description,
								"gpu renderer native xterm metal",
							),
							messageItem(
								"host-restart",
								m.settings_terminal_tab_restart_terminal_host,
								m.settings_registry_terminal_host_restart_description,
								"hostd old upgrade",
							),
						],
					},
					{
						get title() {
							return m.settings_terminal_tab_text();
						},
						items: [
							messageItem(
								"term-font",
								m.settings_terminal_tab_terminal_font,
								m.settings_registry_terminal_term_font_description,
								"font_family typeface font-family xterm ghostty nerd variable",
							),
							messageItem(
								"term-size",
								m.settings_terminal_tab_terminal_font_size,
								m.settings_registry_terminal_term_size_description,
								"font_size",
							),
							messageItem(
								"font-thicken",
								m.settings_terminal_tab_thicken_glyphs,
								m.settings_registry_terminal_font_thicken_description,
								"font_thicken bold weight ghostty",
							),
							macItem(
								"ligatures",
								m.settings_terminal_tab_ligatures,
								m.settings_registry_terminal_ligatures_description,
								"ligatures fira",
							),
							macItem(
								"contrast",
								m.settings_terminal_tab_minimum_contrast,
								m.settings_registry_terminal_contrast_description,
								"wcag accessibility minimum_contrast readable",
							),
						],
					},
					{
						get title() {
							return m.settings_terminal_tab_cursor();
						},
						items: [
							messageItem(
								"cursor-shape",
								m.settings_terminal_tab_cursor_shape,
								m.settings_terminal_tab_how_the_cursor_looks_in,
								"block bar underline hollow caret",
							),
							messageItem(
								"cursor-blink",
								m.settings_terminal_tab_blinking_cursor,
								m.settings_registry_terminal_cursor_blink_description,
								"caret",
							),
						],
					},
					{
						get title() {
							return m.settings_terminal_tab_padding();
						},
						items: [
							messageItem(
								"term-padding-x",
								m.settings_terminal_tab_horizontal_padding,
								m.settings_registry_terminal_term_padding_x_description,
								"padding_x margin inset window-padding-x ghostty",
							),
							messageItem(
								"term-padding-y",
								m.settings_terminal_tab_vertical_padding,
								m.settings_registry_terminal_term_padding_y_description,
								"padding_y margin inset window-padding-y ghostty",
							),
						],
					},
					{
						get title() {
							return m.settings_terminal_tab_behavior();
						},
						items: [
							messageItem(
								"scrollback",
								m.settings_terminal_tab_scrollback,
								m.settings_terminal_tab_lines_of_output_each_terminal,
								"max_scroll_history_lines history buffer",
							),
							macItem(
								"scrollback-memory",
								m.settings_terminal_tab_scrollback_memory,
								m.settings_registry_terminal_scrollback_memory_description,
								"scrollback_bytes memory limit megabytes mb ghostty scrollback-limit",
							),
							messageItem(
								"clipboard",
								m.settings_terminal_tab_programs_can_copy_to_your,
								m.settings_registry_terminal_clipboard_description,
								"osc52 osc 52 copy paste ask allow deny",
							),
							messageItem(
								"copy-on-select",
								m.settings_terminal_tab_copy_on_select,
								m.settings_registry_terminal_copy_on_select_description,
								"copy_on_select clipboard selection ghostty",
							),
							macItem(
								"click-to-move",
								m.settings_terminal_tab_click_to_move_the_cursor,
								m.settings_registry_terminal_click_to_move_description,
								"click_to_move cursor prompt osc 133 ghostty",
							),
							macItem(
								"option-as-alt",
								m.settings_terminal_tab_option_key_as_alt,
								() => m.settings_registry_terminal_option_as_alt_description({ key: kbd("alt") }),
								"option_as_alt macos-option-as-alt meta alt esc ghostty",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "git",
		get label() {
			return m.settings_registry_git_label();
		},
		get description() {
			return m.settings_registry_git_description();
		},
		cluster: "work",
		pages: [
			{
				files: ["components/GitSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_general_git_commit_messages();
						},
						items: [
							messageItem(
								"git-commit-agent",
								m.settings_general_git_generate_commit_messages_with,
								m.settings_registry_git_git_commit_agent_description,
								"ai commit message sparkle generate omp claude codex privacy diff staged",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "agents",
		get label() {
			return m.settings_registry_agents_label();
		},
		get description() {
			return m.settings_registry_agents_description();
		},
		cluster: "work",
		pages: [
			{
				files: ["components/AgentSettings.svelte", "components/settings/DockSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_agents_group_coding_agents();
						},
						items: [
							messageItem(
								"agent-default",
								m.settings_registry_agents_agent_default_label,
								m.settings_registry_agents_agent_default_description,
								"omp claude codex blank",
							),
							messageItem(
								"agent-permissions",
								m.settings_registry_agents_agent_permissions_label,
								m.settings_registry_agents_agent_permissions_description,
								"permissions yolo manual auto approve",
							),
							messageItem(
								"agent-args",
								m.settings_registry_agents_agent_args_label,
								m.settings_registry_agents_agent_args_description,
								"model flags args",
							),
						],
					},
					{
						get title() {
							return m.settings_general_dock_agent_dock();
						},
						items: [
							messageItem(
								"dock-enabled",
								m.settings_general_dock_show_the_dock,
								m.settings_registry_agents_dock_enabled_description,
								"floating pill overlay widget menu bar always on top hide show",
							),
							messageItem(
								"dock-hotkey",
								m.settings_general_dock_message_an_agent,
								m.settings_general_dock_opens_the_dock_s_composer,
								"hotkey shortcut summon global keybinding send prompt",
							),
							messageItem(
								"dock-toggle-hotkey",
								m.settings_general_dock_hide_or_show_the_dock,
								m.settings_general_dock_works_even_while_the_dock,
								"hotkey shortcut hide show toggle global keybinding tray",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_agents_group_custom_agents();
						},
						items: [
							messageItem(
								"agent-custom",
								m.settings_registry_agents_agent_custom_label,
								() => m.settings_registry_agents_agent_custom_description({ key: kbd("cmd-shift-a") }),
								"custom opencode gemini amp aider cli preset add",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_agents_group_quick_commands();
						},
						items: [
							messageItem(
								"quick",
								m.settings_registry_agents_group_quick_commands,
								m.settings_registry_agents_quick_description,
								"shortcuts prompt buttons",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "integrations",
		get label() {
			return m.settings_registry_integrations_label();
		},
		get description() {
			return m.settings_registry_integrations_description();
		},
		cluster: "connect",
		pages: [
			{
				id: "shell",
				get label() {
					return m.settings_registry_integrations_tab_shell_label();
				},
				get description() {
					return m.settings_registry_integrations_tab_shell_description();
				},
				files: ["components/settings/ShellLinks.svelte", "components/settings/LinksSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_integrations_group_shell_environment();
						},
						items: [
							messageItem(
								"shell",
								m.settings_general_shell_links_shell_for_your_projects,
								m.settings_registry_integrations_shell_description,
								"zsh bash fish path environment variables env login",
							),
						],
					},
					{
						get title() {
							return m.settings_general_links_links();
						},
						items: [
							messageItem(
								"browser",
								m.settings_general_links_open_links_in,
								m.settings_registry_integrations_browser_description,
								"browser system atlas",
							),
						],
					},
				],
			},
			{
				id: "agents",
				get label() {
					return m.settings_general_agent_hooks_agent_hooks();
				},
				get description() {
					return m.settings_registry_integrations_tab_agents_description();
				},
				files: ["components/settings/AgentHooks.svelte"],
				groups: [
					{
						get title() {
							return m.settings_general_agent_hooks_agent_hooks();
						},
						items: [
							messageItem(
								"agent-integration",
								m.settings_general_agent_hooks_agent_hooks,
								m.settings_registry_integrations_agent_integration_description,
								"hooks plugin enable omp claude codex integration extension",
								"agent-hooks",
							),
						],
					},
				],
			},
			{
				id: "ssh",
				get label() {
					return m.settings_registry_integrations_tab_ssh_label();
				},
				get description() {
					return m.settings_registry_integrations_tab_ssh_description();
				},
				aliases: ["ssh"],
				files: ["components/SshSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_integrations_group_hosts();
						},
						items: [
							messageItem(
								"ssh-hosts",
								m.settings_registry_integrations_tab_ssh_label,
								m.settings_registry_integrations_ssh_hosts_description,
								"remote server hostd tunnel",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_integrations_group_remote_projects();
						},
						items: [
							messageItem(
								"ssh-project",
								m.settings_registry_integrations_ssh_project_label,
								m.settings_registry_integrations_ssh_project_description,
								"atlas.yaml path remote",
							),
						],
					},
				],
			},
			{
				id: "mobile",
				get label() {
					return m.settings_registry_integrations_tab_mobile_label();
				},
				get description() {
					return m.settings_registry_integrations_tab_mobile_description();
				},
				aliases: ["mobile"],
				files: [
					"components/MobileSettings.svelte",
					"components/mobile/PairCard.svelte",
					"components/mobile/PhoneRow.svelte",
				],
				groups: [
					{
						get title() {
							return m.settings_registry_integrations_group_pairing();
						},
						items: [
							messageItem(
								"mobile-pair",
								m.settings_registry_integrations_mobile_pair_label,
								m.settings_registry_integrations_mobile_pair_description,
								"iphone android qr phone remote p2p app store testflight vpn warp wifi",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_integrations_group_devices();
						},
						items: [
							messageItem(
								"mobile-devices",
								m.settings_registry_integrations_mobile_devices_label,
								m.settings_registry_integrations_mobile_devices_description,
								"revoke unpair rename activity last seen permissions scopes terminal approve",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "crew",
		get label() {
			return m.settings_registry_crew_label();
		},
		get description() {
			return m.settings_registry_crew_description();
		},
		cluster: "connect",
		pages: [
			{
				files: ["components/CrewSettings.svelte"],
				groups: [
					{
						get title() {
							return m.settings_registry_crew_group_crew_servers();
						},
						items: [
							messageItem(
								"crew-add",
								m.crew_onboarding_primary_add_crew_server,
								m.settings_registry_crew_crew_add_description,
								"ssh server crew agent",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_crew_group_github();
						},
						items: [
							messageItem(
								"crew-github",
								m.settings_registry_crew_group_github,
								m.settings_registry_crew_crew_github_description,
								"git private repo token oauth pat",
							),
						],
					},
					{
						get title() {
							return m.crew_settings_behaviour_card_short_replies();
						},
						items: [
							messageItem(
								"short-replies",
								m.crew_settings_behaviour_card_short_replies,
								m.settings_registry_crew_short_replies_description,
								"summary compact ai model cheap",
							),
						],
					},
					{
						get title() {
							return m.settings_registry_crew_group_onboarding();
						},
						items: [
							messageItem(
								"crew-onboarding",
								m.settings_registry_crew_crew_onboarding_label,
								m.settings_registry_crew_crew_onboarding_description,
								"onboarding intro tour crew atlas crew",
							),
						],
					},
				],
			},
		],
	},
] as const satisfies readonly CategoryDef[];

/** The registry as plain definitions, for code that walks it without needing the literal ids. Every platform's rows. */
export const REGISTRY: readonly CategoryDef[] = DEFS;

/**
 * `base` with some properties replaced, keeping its getters live. Not `{ ...base, ...over }`: a spread would freeze a
 * message-backed label, description or title at its current language.
 */
const override = <T extends object>(base: T, over: object): T =>
	Object.defineProperties(
		{},
		{ ...Object.getOwnPropertyDescriptors(base), ...Object.getOwnPropertyDescriptors(over) },
	) as T;

/**
 * What this platform shows: the registry without the categories and rows that only exist on macOS (capture, the
 * native GPU terminal and its own options). Sidebar, search index and deep links all read this, so a hidden page can
 * neither be found nor opened. Empty groups and pages drop with their last row.
 */
export function forPlatform(defs: readonly CategoryDef[], mac: boolean): readonly CategoryDef[] {
	if (mac) return defs;
	return defs
		.filter((c) => !c.macOnly)
		.map((c) =>
			override(c, {
				pages: c.pages
					.map((p) =>
						override(p, {
							groups: p.groups
								.map((g) => override(g, { items: g.items.filter((i) => !i.macOnly) }))
								.filter((g) => g.items.length > 0),
						}),
					)
					.filter((p) => p.groups.length > 0),
			}),
		);
}

/** The registry with its literal ids, from which the section and tab id types are derived. */
export type RegistryDefs = typeof DEFS;
