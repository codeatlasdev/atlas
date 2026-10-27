/**
 * The Settings registry: the ONE declarative description of what Settings contains.
 *
 *   category (sidebar entry, page title) -> page (a tab, or the whole page) -> group (a card) -> item (a row)
 *
 * The sidebar, the page header, the tab strip, the search index, the breadcrumb of a result and every
 * `updates.show("<id>")` deep link are all derived from this file, so they cannot disagree. Adding a setting is
 * one `item(...)` here plus its row in the page component named in `files`; `registry.test.ts` and
 * `placement.test.ts` go red when either half is missing.
 *
 * Rules the tests pin:
 *  - ids never change when a setting moves (search, deep links and the flash on arrival key on them);
 *  - an `aliases` entry keeps an id that shipped before working after the thing it named moved or was renamed;
 *  - labels and descriptions are identical to the screens.
 * Pure: no Svelte, no IPC. Icons and components live next to the UI (`SettingsNav`, `pageComponents.ts`).
 */
import { CATEGORIES as SOUND_CATEGORIES, SPECS } from "#lib/sound/policy.ts";

/** One row. `keywords` are words people may type that the label does not contain. */
export interface ItemDef {
	readonly id: string;
	readonly label: string;
	readonly description: string;
	readonly keywords?: string;
	/** Older deep-link ids for this row. */
	readonly aliases?: readonly string[];
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
	readonly pages: readonly PageDef[];
}

/** Sidebar headings, in order. */
export const CLUSTERS = [
	{ id: "app", label: "Atlas" },
	{ id: "work", label: "Work" },
	{ id: "connect", label: "Connect" },
] as const;

const item = (
	id: string,
	label: string,
	description: string,
	keywords = "",
	...aliases: string[]
): ItemDef => ({ id, label, description, ...(keywords && { keywords }), ...(aliases.length && { aliases }) });

/** One row per sound category; the ids and texts come from the sound policy so the two lists stay one. */
const SOUND_ITEMS = SOUND_CATEGORIES.map((c) =>
	item(`sound-cat-${c}`, SPECS[c].label, SPECS[c].description, "sound earcon chime"),
);

const DEFS = [
	{
		id: "general",
		label: "General",
		description:
			"Keep Atlas up to date: the app, the atlas CLI and the terminal host, and the release channel they follow.",
		cluster: "app",
		pages: [
			{
				files: [
					"components/settings/UpdatesSection.svelte",
					"components/settings/UpdateChannel.svelte",
					"components/settings/HostStatusRow.svelte",
				],
				groups: [
					{
						title: "Updates",
						items: [
							item(
								"updates",
								"Updates",
								"Check for a new Atlas release and install it.",
								"version upgrade release cli app",
							),
							item(
								"update-channel",
								"Update channel",
								"Follow Stable, Beta or Alpha releases. Shared with the atlas CLI.",
								"beta alpha stable prerelease pre-release rc nightly insiders downgrade release train",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "appearance",
		label: "Appearance",
		description: "Theme, accent color, interface font, zoom and the layout of the file panel and panes.",
		cluster: "app",
		pages: [
			{
				files: ["settings/ThemeTab.svelte", "settings/InterfaceTab.svelte"],
				groups: [
					{
						title: "Color theme",
						items: [
							item(
								"theme-mode",
								"Theme",
								"Follow the system, or always use light or dark.",
								"dark light mode system",
							),
							item(
								"theme-light",
								"Light theme",
								"Colors used when Atlas is in light mode.",
								"alucard zed json",
							),
							item("theme-dark", "Dark theme", "Colors used when Atlas is in dark mode.", "dracula zed json"),
							item(
								"theme-terminal",
								"Terminal theme",
								"Use the app theme in terminals, or pick another one.",
								"dracula alucard colors ansi palette",
							),
							item(
								"theme-import",
								"Import theme",
								"Bring a theme from Ghostty, iTerm, VS Code, Alacritty or Base16, or a zip or folder of themes.",
								"ghostty iterm itermcolors vscode vsix base16 alacritty yaml toml zip package folder dracula pro zeno alucard import zed",
							),
							item(
								"theme-export",
								"Export theme",
								"Copy a theme as JSON or a Ghostty config, or save it to a file.",
								"ghostty iterm vscode base16 dracula alucard export share copy save reveal folder",
							),
							item(
								"accent",
								"Accent color",
								"Highlights buttons, selections and focus.",
								"brand color swatch",
							),
						],
					},
					{
						title: "Zoom",
						items: [
							item(
								"ui-zoom",
								"Zoom",
								"Scale the whole window. ⌘+ and ⌘− step it, ⌘0 resets it (Ctrl instead of ⌘ outside macOS).",
								"scale magnify bigger smaller zoom in out reset layout size",
							),
						],
					},
					{
						title: "Interface font",
						items: [
							item(
								"ui-font",
								"Interface font",
								"Font for menus, sidebars and buttons.",
								"ui_font_family typeface",
							),
							item("ui-size", "Interface font size", "Text size across the app, in pixels.", "ui_font_size"),
						],
					},
					{
						title: "File panel",
						items: [
							item(
								"panel-dock",
								"File panel side",
								"Show the file tree on the left or the right.",
								"project_panel dock explorer",
							),
							item(
								"panel-indent",
								"Folder indent",
								"How far each folder level is pushed in, in pixels.",
								"indent_size tree",
							),
							item("panel-file-icons", "File icons", "Show an icon next to each file.", "file_icons"),
							item("panel-folder-icons", "Folder icons", "Show an icon next to each folder.", "folder_icons"),
						],
					},
					{
						title: "Split panes",
						items: [
							item(
								"pane-border",
								"Active pane border",
								"Outline the pane you are working in, in pixels. 0 hides it.",
								"active_pane_modifiers border_size focus",
							),
							item(
								"pane-opacity",
								"Dim inactive panes",
								"How visible panes you are not using stay, from 0.2 (faint) to 1 (not dimmed).",
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
		label: "Notifications & sounds",
		description:
			"When Atlas tells you something: a system notification when an agent needs you, and short sounds.",
		cluster: "app",
		pages: [
			{
				files: [
					"components/settings/NotificationSettings.svelte",
					"components/settings/SoundSettings.svelte",
				],
				groups: [
					{
						title: "Notifications",
						items: [
							item(
								"notify",
								"Notifications",
								"Get told when an agent finishes or needs you.",
								"sound alert bell focus",
							),
						],
					},
					{
						title: "Sounds",
						items: [
							item(
								"sounds",
								"Sounds",
								"Short, quiet cues for the moments that matter. Off with one switch.",
								"audio sound effects earcon chime notification mute silent",
							),
							item(
								"sound-volume",
								"Sound volume",
								"How loud Atlas sounds are, on top of your system's volume.",
								"loudness quiet audio level",
							),
							item(
								"sound-reduce",
								"Reduce sounds",
								"Only play the ones that need your attention, and quieter.",
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
		label: "Capture",
		description:
			"Screenshots and screen recordings from a hotkey: where they go, the keys, and your history.",
		cluster: "app",
		pages: [
			{
				files: ["components/settings/CaptureSettings.svelte"],
				groups: [
					{
						title: "Capture",
						items: [
							item(
								"capture-permission",
								"Screen Recording access",
								"Screen Recording lets Atlas see the pixels of other apps. It is only used when you press a capture hotkey; nothing is recorded in the background. After granting it, macOS asks you to restart Atlas.",
								"capture screenshot screen recording permission privacy grant system settings",
							),
							item(
								"capture-enabled",
								"Capture hotkeys",
								"Turn the capture hotkeys on or off. Off gives the keys back to other apps.",
								"capture screenshot screen recording gif hotkey enable disable",
							),
							item(
								"capture-folder",
								"Save to",
								"Where captures are saved.",
								"capture screenshot screen recording gif folder directory pictures path",
							),
						],
					},
					{
						title: "Hotkeys",
						items: [
							item(
								"capture-preset",
								"Key set",
								"Start from a ready-made set of keys. Picking one shows what it changes; nothing is saved until you apply it.",
								"capture screenshot hotkey preset macos cleanshot shortcuts keys",
							),
							item(
								"capture-system-keys",
								"macOS screenshot shortcuts",
								"macOS answers ⇧⌘3, ⇧⌘4, ⇧⌘5 and ⇧⌘6 before any app, so Atlas can only use them once you turn them off yourself in System Settings ▸ Keyboard ▸ Keyboard Shortcuts ▸ Screenshots. Atlas never changes that setting for you.",
								"capture screenshot macos system shortcuts keyboard shift command 3 4 5 6 disable free",
							),
							item(
								"capture-key-all-in-one",
								"All-in-One",
								"One toolbar over the screen: pick region, window, screen, record, GIF or text.",
								"capture screenshot all in one toolbar menu hotkey shortcut",
							),
							item(
								"capture-key-region",
								"Region",
								"Drag to pick an area, or press Space to switch to picking a window.",
								"capture screenshot screen region area hotkey shortcut",
							),
							item(
								"capture-key-previous",
								"Previous area",
								"Capture the same area as your last region capture, right now.",
								"capture screenshot previous last area repeat region hotkey shortcut",
							),
							item(
								"capture-key-window",
								"Window",
								"Click a window to capture it.",
								"capture screenshot window hotkey shortcut",
							),
							item(
								"capture-key-fullscreen",
								"Screen",
								"Capture the whole screen under the pointer.",
								"capture screenshot fullscreen full screen display hotkey shortcut",
							),
							item(
								"capture-key-ocr",
								"Text",
								"Drag over text on screen and copy it, read on this Mac.",
								"capture screenshot text ocr recognize copy hotkey shortcut",
							),
							item(
								"capture-key-record",
								"Record",
								"Pick an area and record a video. Press the key again to stop.",
								"capture screen recording video record hotkey shortcut",
							),
							item(
								"capture-key-gif",
								"GIF",
								"Pick an area and record an animated GIF.",
								"capture screen recording gif animated hotkey shortcut",
							),
						],
					},
					{
						title: "Screenshots",
						items: [
							item(
								"capture-on-release",
								"Capture when the mouse is released",
								"Take the shot as soon as you let go of the selection; turn off to adjust it first and confirm with Return.",
								"capture screenshot region release mouse instant enter return confirm adjust",
							),
							item(
								"capture-timer",
								"Self-timer",
								"Wait this long after you pick before a screenshot is taken. Recordings are not delayed.",
								"capture screenshot timer delay countdown seconds",
							),
							item(
								"capture-scale",
								"Resolution",
								"Retina saves the display's own pixels; Standard saves one pixel per point, a smaller file.",
								"capture screenshot resolution retina scale 1x 2x pixels size",
							),
							item(
								"capture-hide-icons",
								"Hide desktop icons",
								"Hide the icons on the desktop while a screenshot or recording is taken.",
								"capture screenshot screen recording desktop icons hide clean",
							),
							item(
								"capture-window-shadow",
								"Window shadow",
								"A window capture keeps its drop shadow, on a transparent background.",
								"capture screenshot window shadow transparent",
							),
							item(
								"capture-window-padding",
								"Window background",
								"A window capture sits on a padded background instead of a shadow. It wins over the shadow when both are on.",
								"capture screenshot window background padding backdrop",
							),
						],
					},
					{
						title: "Recording",
						items: [
							item(
								"capture-quality",
								"Video quality",
								"Lower qualities record fewer pixels, so the file is smaller. GIFs are not affected.",
								"capture screen recording video quality high medium low size bitrate",
							),
							item(
								"capture-clicks",
								"Show clicks",
								"A ring on each click in screen recordings and GIFs.",
								"capture screen recording gif click ring mouse highlight",
							),
							item(
								"capture-keystrokes",
								"Show keystrokes",
								"A pill with the keys you press at the bottom of the recorded area. macOS asks for Input Monitoring the first time.",
								"capture screen recording keystrokes keys keyboard pressed input monitoring",
							),
							item(
								"capture-system-audio",
								"Record system sound",
								"The sound your Mac plays goes into the video. Not in GIFs.",
								"capture screen recording system sound audio computer",
							),
							item(
								"capture-microphone",
								"Record the microphone",
								"Your voice goes into the video. macOS asks for microphone access the first time. Not in GIFs.",
								"capture screen recording microphone mic voice audio narration",
							),
						],
					},
					{
						title: "After a capture",
						items: [
							item(
								"capture-copy",
								"Copy to clipboard",
								"Put every capture on the clipboard as soon as it is taken: screenshots as an image, recordings and GIFs as the file.",
								"capture screenshot clipboard copy",
							),
							item(
								"capture-quick-access",
								"Show the quick access overlay",
								"A small card with the capture: copy, save as, annotate, send to an agent, or drag it into another app.",
								"capture screenshot quick access overlay thumbnail card annotate",
							),
							item(
								"capture-dismiss",
								"Auto-dismiss the overlay",
								"Seconds before the card goes away by itself, 0 to 60. 0 keeps it until you close it.",
								"capture screenshot quick access overlay seconds timer never",
							),
							item(
								"capture-corner",
								"Overlay corner",
								"The corner of the display under the pointer where the quick access cards stack.",
								"capture screenshot quick access overlay corner position bottom top left right",
							),
							item(
								"capture-stack",
								"Cards in the stack",
								"How many quick access cards stack in the corner before the oldest goes, 1 to 8.",
								"capture screenshot quick access overlay stack cards number limit",
							),
							item(
								"capture-sound",
								"Play sound",
								"A shutter sound when a screenshot is taken.",
								"capture screenshot sound shutter audio",
							),
							item(
								"capture-highlight",
								"Highlight the cursor and clicks",
								"Show the pointer and a ring on each click in screen recordings and GIFs.",
								"capture screen recording gif cursor pointer click highlight",
							),
						],
					},
					{
						title: "History",
						items: [
							item(
								"capture-history-size",
								"History size",
								"How many captures to remember, 1 to 500. Older ones drop off the list; their files stay.",
								"capture screenshot screen recording gif history size limit keep",
							),
							item(
								"capture-history",
								"Recent captures",
								"Open, reveal or remove from the list your recent captures. Removing never deletes the file.",
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
		label: "Keyboard shortcuts",
		description: "Change the key of any command, or remove it.",
		cluster: "app",
		pages: [
			{
				files: ["components/ShortcutSettings.svelte"],
				groups: [
					{
						title: "Shortcuts",
						items: [
							item(
								"keymap",
								"Keyboard shortcuts",
								"Change the key for any command, or remove it.",
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
		label: "Help & onboarding",
		description: "Take the tour again, look up a shortcut, or bring the checklist back.",
		cluster: "app",
		pages: [
			{
				files: ["onboarding/HelpSettings.svelte"],
				groups: [
					{
						title: "Help",
						items: [
							item(
								"tour",
								"Guided tour",
								"A short walk through spaces, the header, the panel and search.",
								"onboarding intro",
							),
							item("help-keys", "Shortcut cheat sheet", "Every key, searchable.", "keyboard help"),
							item(
								"checklist",
								"Getting started checklist",
								"Bring back the first steps card, or start over.",
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
		label: "Workspaces",
		description:
			"Where Atlas puts worktrees, how their branches are named, and cleaning up the ones you no longer use.",
		cluster: "work",
		pages: [
			{
				files: ["components/WorkspaceSettings.svelte"],
				groups: [
					{
						title: "New worktrees",
						items: [
							item(
								"ws-dir",
								"Worktree folder",
								"Where Atlas creates checkouts for new worktrees.",
								"workspace directory path location",
							),
							item(
								"ws-nest",
								"Group by repository",
								"Put each worktree in a folder named after its repository.",
								"nest",
							),
							item(
								"ws-prefix",
								"Branch name prefix",
								"Added in front of the branch Atlas creates, so branches are easy to tell apart.",
								"git user.name custom branch",
							),
						],
					},
					{
						title: "Auto clean up",
						items: [
							item(
								"ws-autoclean",
								"Remove worktrees automatically",
								"Remove linked worktrees whose PR was merged or that sat idle, never with unsaved or unpushed work.",
								"auto delete merged idle stale prune restore",
							),
						],
					},
					{
						title: "Clean up",
						items: [
							item(
								"ws-cleanup",
								"Clean up worktrees",
								"See size, last commit and git state of linked worktrees, and delete the ones you no longer need.",
								"delete remove disk scan",
							),
						],
					},
				],
			},
		],
	},
	{
		id: "editor",
		label: "Editor",
		description:
			"The code editor and diffs: font, indentation and display, and the language servers behind completion.",
		cluster: "work",
		aliases: ["appearance/editor"],
		pages: [
			{
				id: "editor",
				label: "Editor",
				description: "Font, indentation and display of the code editor and diffs.",
				files: ["settings/CodeTab.svelte", "settings/EditorRows.svelte", "settings/FontRows.svelte"],
				groups: [
					{
						title: "Code font",
						items: [
							item(
								"buffer-font",
								"Code font",
								"Font for the editor and diffs.",
								"buffer_font_family monospace typeface",
							),
							item(
								"buffer-size",
								"Code font size",
								"Text size in the editor and diffs, in pixels.",
								"buffer_font_size",
							),
							item(
								"buffer-line-height",
								"Line spacing",
								"Space between lines of code, as a multiple of the font size.",
								"buffer_line_height leading",
							),
						],
					},
					{
						title: "Language servers",
						items: [
							item(
								"lsp-enable",
								"Language servers",
								"Completion, diagnostics, hover and go to definition from the language's server (downloaded on first use).",
								"lsp enable_language_servers rust-analyzer gopls typescript intellisense autocomplete",
							),
							item(
								"lsp-format-on-save",
								"Format on save",
								"Format the file with its language server every time it is saved.",
								"format_on_save formatter prettier rustfmt",
							),
							item(
								"lsp-inlay-hints",
								"Inlay hints",
								"Show inferred types and parameter names inline, dimmed.",
								"inlay_hints types parameters",
							),
							item(
								"lsp-manage",
								"Manage language servers",
								"Install, update or replace the server of each language.",
								"languages custom command lsp gopls rust-analyzer typescript server install uninstall",
							),
						],
					},
					{
						title: "Editing",
						items: [
							item(
								"editor-tab-size",
								"Tab size",
								"Columns a tab or one level of indentation spans.",
								"tab_size indent indentation width",
							),
							item(
								"editor-insert-spaces",
								"Indent with spaces",
								"Pressing Tab inserts spaces. Turn off to insert tab characters.",
								"insert_spaces tabs spaces indentation",
							),
							item(
								"editor-trim-whitespace",
								"Trim trailing whitespace on save",
								"Remove spaces and tabs at the end of lines when you save.",
								"trim_trailing_whitespace_on_save strip",
							),
							item(
								"editor-final-newline",
								"Add a final newline on save",
								"End every file with exactly one newline when you save.",
								"insert_final_newline_on_save eof end of file",
							),
							item(
								"editor-auto-reload",
								"Reload files changed on disk",
								"Update an open file when another program changes it, as long as you have no unsaved edits in it.",
								"auto_reload external changes refresh watch",
							),
							item(
								"editor-large-file",
								"Large file limit",
								"Files bigger than this open in a light mode without language features, so the editor stays fast. 0.1 to 100 MB.",
								"large_file_threshold_bytes performance slow big huge",
							),
						],
					},
					{
						title: "Display",
						items: [
							item(
								"editor-word-wrap",
								"Word wrap",
								"Wrap long lines at the edge of the editor.",
								"word_wrap soft wrap long lines",
							),
							item(
								"editor-line-numbers",
								"Line numbers",
								"Show line numbers in the gutter.",
								"line_numbers gutter",
							),
							item(
								"editor-highlight-line",
								"Highlight the current line",
								"Tint the line the cursor is on.",
								"highlight_active_line cursor line",
							),
							item(
								"editor-whitespace",
								"Show whitespace",
								"Draw dots for spaces and arrows for tabs.",
								"render_whitespace invisible characters",
							),
							item(
								"editor-indent-guides",
								"Indent guides",
								"Draw a faint vertical line at each level of indentation.",
								"indent_guides",
							),
							item(
								"editor-bracket-colors",
								"Color matching brackets",
								"Give each level of nested brackets its own color.",
								"bracket_pair_colorization rainbow brackets",
							),
							item(
								"editor-sticky-scroll",
								"Sticky scroll",
								"Keep the enclosing function or class pinned at the top while you scroll its body.",
								"sticky_scroll scope header",
							),
							item(
								"editor-minimap",
								"Minimap",
								"Show a small overview of the file on the right.",
								"overview",
							),
							item(
								"editor-rulers",
								"Rulers",
								"Columns where a vertical line is drawn, such as 80 or 100. Separate several with commas.",
								"column guide line length 80 100 120",
							),
							item(
								"editor-scroll-past-end",
								"Scroll past the last line",
								"Let the last line scroll up to the top of the editor.",
								"scroll_past_end",
							),
							item(
								"editor-smooth-scroll",
								"Smooth scrolling",
								"Animate scrolling when you jump to a line or use the keyboard.",
								"smooth_scrolling animation",
							),
						],
					},
				],
			},
			{
				id: "languages",
				label: "Languages",
				description: "Install, update and replace the language server of each language.",
				aliases: ["appearance/languages", "languages"],
				files: ["settings/LanguagesTab.svelte", "settings/ServerRow.svelte", "settings/CustomServer.svelte"],
				groups: [
					{
						title: "Language servers",
						items: [
							item(
								"lsp-servers",
								"Language servers",
								"See which language servers are installed, install or update them, and open their logs.",
								"language server typescript gopls rust-analyzer pyright clangd install uninstall update restart log path managed",
							),
							item(
								"lsp-custom",
								"Custom language server",
								"Run your own server for a language: its command, arguments, initialization options and settings.",
								"custom command args arguments initialization options json override replace binary lsp typescript gopls rust-analyzer",
							),
							item(
								"lsp-project",
								"Per-project language servers",
								"Override servers for one project with a file inside it; it wins over your own settings.",
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
		label: "Terminal",
		description:
			"How terminals and coding agents draw and behave: renderer, font, cursor, padding and clipboard.",
		cluster: "work",
		aliases: ["appearance/terminal"],
		pages: [
			{
				files: ["settings/TerminalTab.svelte"],
				groups: [
					{
						title: "Renderer",
						items: [
							item(
								"gpu",
								"Fast terminal (GPU)",
								"Draws local terminals on the graphics chip: smoother scrolling and faster output.",
								"gpu renderer native xterm metal",
							),
							item(
								"host-restart",
								"Restart terminal host",
								"Restart the background process that keeps terminals alive, to pick up a new Atlas version.",
								"hostd old upgrade",
							),
						],
					},
					{
						title: "Text",
						items: [
							item(
								"term-font",
								"Terminal font",
								"Font families in order; the first one installed wins, else JetBrains Mono. Variable fonts work. Both terminals use it.",
								"font_family typeface font-family xterm ghostty nerd variable",
							),
							item("term-size", "Terminal font size", "Text size in terminals, in pixels.", "font_size"),
							item(
								"font-thicken",
								"Thicken glyphs",
								"Draw terminal text slightly heavier, like Ghostty's font-thicken. Helps thin fonts on dark backgrounds.",
								"font_thicken bold weight ghostty",
							),
							item(
								"ligatures",
								"Ligatures",
								"Join characters like -> and != into single symbols (fast terminal only).",
								"ligatures fira",
							),
							item(
								"contrast",
								"Minimum contrast",
								"Brighten or darken text that is hard to read against its background (fast terminal only). 1 turns it off, 4.5 meets accessibility guidance.",
								"wcag accessibility minimum_contrast readable",
							),
						],
					},
					{
						title: "Cursor",
						items: [
							item(
								"cursor-shape",
								"Cursor shape",
								"How the cursor looks in terminals.",
								"block bar underline hollow caret",
							),
							item("cursor-blink", "Blinking cursor", "Make the terminal cursor blink.", "caret"),
						],
					},
					{
						title: "Padding",
						items: [
							item(
								"term-padding-x",
								"Horizontal padding",
								"Space left and right of the terminal text, in points. The margin uses the terminal background.",
								"padding_x margin inset window-padding-x ghostty",
							),
							item(
								"term-padding-y",
								"Vertical padding",
								"Space above and below the terminal text, in points. The margin uses the terminal background.",
								"padding_y margin inset window-padding-y ghostty",
							),
						],
					},
					{
						title: "Behavior",
						items: [
							item(
								"scrollback",
								"Scrollback",
								"Lines of output each terminal remembers. More lines use more memory.",
								"max_scroll_history_lines history buffer",
							),
							item(
								"scrollback-memory",
								"Scrollback memory",
								"Most memory one terminal spends on its scrollback, in MB. A wide terminal keeps fewer lines than the setting above to stay inside it (fast terminal only).",
								"scrollback_bytes memory limit megabytes mb ghostty scrollback-limit",
							),
							item(
								"clipboard",
								"Programs can copy to your clipboard",
								"Lets commands such as tmux or ssh sessions put text on your clipboard.",
								"osc52 osc 52 copy paste ask allow deny",
							),
							item(
								"copy-on-select",
								"Copy on select",
								"Copy selected text to the clipboard when you release the mouse (fast terminal only).",
								"copy_on_select clipboard selection ghostty",
							),
							item(
								"click-to-move",
								"Click to move the cursor",
								"Click the prompt line to move the cursor there. Needs shell integration (fast terminal only).",
								"click_to_move cursor prompt osc 133 ghostty",
							),
							item(
								"option-as-alt",
								"Option key as Alt",
								"Make ⌥ send Alt instead of typing macOS characters (fast terminal only).",
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
		label: "Git",
		description: "How Atlas helps with commits. Nothing is committed for you.",
		cluster: "work",
		pages: [
			{
				files: ["components/GitSettings.svelte"],
				groups: [
					{
						title: "Commit messages",
						items: [
							item(
								"git-commit-agent",
								"Generate commit messages with",
								"Which agent writes the commit message from your staged diff, or Off to hide the button. Secrets are never sent.",
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
		label: "Agents",
		description:
			"Which coding agent starts by default, how much it may do on its own, custom agents and quick commands.",
		cluster: "work",
		pages: [
			{
				files: ["components/AgentSettings.svelte", "components/settings/DockSettings.svelte"],
				groups: [
					{
						title: "Coding agents",
						items: [
							item(
								"agent-default",
								"Default agent",
								"What a new agent tab starts.",
								"omp claude codex blank",
							),
							item(
								"agent-permissions",
								"Tool approval",
								"Choose whether agents ask before running tools or run everything on their own.",
								"permissions yolo manual auto approve",
							),
							item(
								"agent-args",
								"Extra arguments",
								"Command-line flags added every time that agent starts.",
								"model flags args",
							),
						],
					},
					{
						title: "Agent dock",
						items: [
							item(
								"dock-enabled",
								"Show the dock",
								"Keep the pill on screen while Atlas runs. Hide it any time with the hide key, the pill's hide button or right-click menu, or the tray menu; the choice is remembered.",
								"floating pill overlay widget menu bar always on top hide show",
							),
							item(
								"dock-hotkey",
								"Message an agent",
								"Opens the dock's composer from any app. The text goes to the agent you last used, or the one you @mention.",
								"hotkey shortcut summon global keybinding send prompt",
							),
							item(
								"dock-toggle-hotkey",
								"Hide or show the dock",
								"Works even while the dock is hidden, so it is always the way back. The tray menu has Show Agent Dock too.",
								"hotkey shortcut hide show toggle global keybinding tray",
							),
						],
					},
					{
						title: "Custom agents",
						items: [
							item(
								"agent-custom",
								"Add agent",
								"Run any agent CLI (OpenCode, Gemini CLI, Amp…) from the + menu and ⌘⇧A.",
								"custom opencode gemini amp aider cli preset add",
							),
						],
					},
					{
						title: "Quick commands",
						items: [
							item(
								"quick",
								"Quick commands",
								"One-click buttons in the tab strip that run a command or send a prompt to the agent.",
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
		label: "Integrations",
		description: "What Atlas connects to: your shell, the browser, agent hooks, SSH machines and your phone.",
		cluster: "connect",
		pages: [
			{
				id: "shell",
				label: "Shell & links",
				description: "The shell your projects start with, and where links open.",
				files: ["components/settings/ShellLinks.svelte", "components/settings/LinksSettings.svelte"],
				groups: [
					{
						title: "Shell environment",
						items: [
							item(
								"shell",
								"Shell for your projects",
								"Services start with the PATH and variables of this shell, the same as in your terminal.",
								"zsh bash fish path environment variables env login",
							),
						],
					},
					{
						title: "Links",
						items: [
							item(
								"browser",
								"Open links in",
								"Where links from services, ports and terminals open.",
								"browser system atlas",
							),
						],
					},
				],
			},
			{
				id: "agents",
				label: "Agent hooks",
				description:
					"Let omp, Claude Code and Codex tell Atlas and Atlas Mobile when they work, wait or finish.",
				files: ["components/settings/AgentHooks.svelte"],
				groups: [
					{
						title: "Agent hooks",
						items: [
							item(
								"agent-integration",
								"Agent hooks",
								"Lets omp, Claude Code and Codex report their status and tools to Atlas and Atlas Mobile.",
								"hooks plugin enable omp claude codex integration extension",
								"agent-hooks",
							),
						],
					},
				],
			},
			{
				id: "ssh",
				label: "SSH hosts",
				description: "Work on a worktree that lives on another machine over SSH.",
				aliases: ["ssh"],
				files: ["components/SshSettings.svelte"],
				groups: [
					{
						title: "Hosts",
						items: [
							item(
								"ssh-hosts",
								"SSH hosts",
								"Work on another machine: Atlas starts its helper there and connects over SSH.",
								"remote server hostd tunnel",
							),
						],
					},
					{
						title: "Remote projects",
						items: [
							item(
								"ssh-project",
								"Remote project",
								"Add a project that lives on one of your SSH hosts.",
								"atlas.yaml path remote",
							),
						],
					},
				],
			},
			{
				id: "mobile",
				label: "Mobile",
				description: "Pair a phone to see and control this computer from Atlas Mobile.",
				aliases: ["mobile"],
				files: ["components/MobileSettings.svelte"],
				groups: [
					{
						title: "Pairing",
						items: [
							item(
								"mobile-pair",
								"Pair a phone",
								"Scan a code with Atlas Mobile to control this Mac from your phone.",
								"iphone android qr phone remote p2p",
							),
						],
					},
					{
						title: "Devices",
						items: [
							item(
								"mobile-devices",
								"Paired phones",
								"What each phone may do, and revoke it.",
								"revoke permissions scopes terminal approve",
							),
						],
					},
				],
			},
		],
	},
] as const satisfies readonly CategoryDef[];

/** The registry as plain definitions, for code that walks it without needing the literal ids. */
export const REGISTRY: readonly CategoryDef[] = DEFS;

/** The registry with its literal ids, from which the section and tab id types are derived. */
export type RegistryDefs = typeof DEFS;
