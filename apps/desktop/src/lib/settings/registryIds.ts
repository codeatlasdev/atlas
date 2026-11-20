/**
 * The structure of the Settings registry: which sections, tabs and rows exist, how they nest, which ids older versions
 * used (`aliases`) and what only exists on macOS. No text and no message import: this is all `updates.show("<id>")`
 * and the deep links need, and it loads at startup. Every label, description and group title is in
 * `registryText.ts`, which loads with the Settings view; `registry.ts` joins the two.
 *
 * Rules the tests pin:
 *  - ids never change when a setting moves (search, deep links and the flash on arrival key on them);
 *  - an `aliases` entry keeps an id that shipped before working after the thing it named moved or was renamed;
 *  - `registryText.ts` has text for exactly the ids here (`registry.test.ts`).
 * Pure: no Svelte, no IPC.
 */
import { CATEGORIES as SOUND_CATEGORIES } from "#lib/sound/categories.ts";

/** A row: only what navigation needs. */
export interface ItemIds {
	readonly id: string;
	/** Older deep-link ids for this row. */
	readonly aliases?: readonly string[];
	/** Only exists on macOS (the native GPU terminal, capture): hidden from nav, search and deep links elsewhere. */
	readonly macOnly?: true;
}

/** A card of rows. */
export interface GroupIds {
	readonly items: readonly ItemIds[];
}

/** A tab of a category, or (without `id`) the whole page of a single-page category. */
export interface PageIds {
	readonly id?: string;
	readonly aliases?: readonly string[];
	/** Sources that render this page, relative to `src/lib`; `placement.test.ts` reads them. */
	readonly files: readonly string[];
	readonly groups: readonly GroupIds[];
}

export interface CategoryIds {
	readonly id: string;
	/** Sidebar heading the entry sits under. */
	readonly cluster: "app" | "work" | "connect";
	/** Older deep-link ids for the whole category (`appearance/terminal`). */
	readonly aliases?: readonly string[];
	/** The whole category is macOS only (capture): no sidebar entry, no search hits, deep links fall back. */
	readonly macOnly?: true;
	readonly pages: readonly PageIds[];
}

/** One row per sound category; the ids come from the sound policy's list so the two stay one. */
const SOUND_ITEMS: readonly ItemIds[] = SOUND_CATEGORIES.map((c) => ({ id: `sound-cat-${c}` }));

export const IDS = [
	{
		id: "general",
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
						items: [{ id: "updates" }, { id: "update-channel" }],
					},
					{
						items: [{ id: "display-language" }],
					},
				],
			},
		],
	},
	{
		id: "appearance",
		cluster: "app",
		pages: [
			{
				files: ["settings/ThemeTab.svelte", "settings/InterfaceTab.svelte"],
				groups: [
					{
						items: [
							{ id: "theme-mode" },
							{ id: "theme-light" },
							{ id: "theme-dark" },
							{ id: "theme-terminal" },
							{ id: "theme-import" },
							{ id: "theme-export" },
							{ id: "accent" },
						],
					},
					{
						items: [{ id: "ui-zoom" }],
					},
					{
						items: [{ id: "ui-font" }, { id: "ui-size" }],
					},
					{
						items: [
							{ id: "panel-dock" },
							{ id: "panel-indent" },
							{ id: "panel-file-icons" },
							{ id: "panel-folder-icons" },
						],
					},
					{
						items: [{ id: "pane-border" }, { id: "pane-opacity" }],
					},
				],
			},
		],
	},
	{
		id: "notifications",
		cluster: "app",
		pages: [
			{
				files: [
					"components/settings/NotificationSettings.svelte",
					"components/settings/SoundSettings.svelte",
				],
				groups: [
					{
						items: [{ id: "notify" }],
					},
					{
						items: [{ id: "sounds" }, { id: "sound-volume" }, { id: "sound-reduce" }, ...SOUND_ITEMS],
					},
				],
			},
		],
	},
	{
		id: "capture",
		cluster: "app",
		macOnly: true,
		pages: [
			{
				files: ["components/settings/CaptureSettings.svelte"],
				groups: [
					{
						items: [{ id: "capture-permission" }, { id: "capture-enabled" }, { id: "capture-folder" }],
					},
					{
						items: [
							{ id: "capture-preset" },
							{ id: "capture-system-keys" },
							{ id: "capture-key-all-in-one" },
							{ id: "capture-key-region" },
							{ id: "capture-key-previous" },
							{ id: "capture-key-window" },
							{ id: "capture-key-fullscreen" },
							{ id: "capture-key-ocr" },
							{ id: "capture-key-record" },
							{ id: "capture-key-gif" },
						],
					},
					{
						items: [
							{ id: "capture-on-release" },
							{ id: "capture-timer" },
							{ id: "capture-scale" },
							{ id: "capture-hide-icons" },
							{ id: "capture-window-shadow" },
							{ id: "capture-window-padding" },
						],
					},
					{
						items: [
							{ id: "capture-quality" },
							{ id: "capture-clicks" },
							{ id: "capture-keystrokes" },
							{ id: "capture-system-audio" },
							{ id: "capture-microphone" },
						],
					},
					{
						items: [
							{ id: "capture-copy" },
							{ id: "capture-quick-access" },
							{ id: "capture-dismiss" },
							{ id: "capture-corner" },
							{ id: "capture-stack" },
							{ id: "capture-sound" },
							{ id: "capture-highlight" },
						],
					},
					{
						items: [{ id: "capture-history-size" }, { id: "capture-history" }],
					},
				],
			},
		],
	},
	{
		id: "keyboard",
		cluster: "app",
		pages: [
			{
				files: ["components/ShortcutSettings.svelte"],
				groups: [
					{
						items: [{ id: "keymap" }],
					},
				],
			},
		],
	},
	{
		id: "help",
		cluster: "app",
		pages: [
			{
				files: ["onboarding/HelpSettings.svelte"],
				groups: [
					{
						items: [{ id: "tour" }, { id: "help-keys" }, { id: "checklist" }],
					},
				],
			},
		],
	},
	{
		id: "workspaces",
		cluster: "work",
		pages: [
			{
				files: ["components/WorkspaceSettings.svelte", "components/settings/HygieneGroup.svelte"],
				groups: [
					{
						items: [{ id: "ws-dir" }, { id: "ws-nest" }, { id: "ws-prefix" }],
					},
					{
						items: [{ id: "ws-autoclean" }],
					},
					{
						items: [{ id: "ws-cleanup" }],
					},
					{
						items: [{ id: "ws-hygiene" }],
					},
				],
			},
		],
	},
	{
		id: "editor",
		cluster: "work",
		aliases: ["appearance/editor"],
		pages: [
			{
				id: "editor",
				files: ["settings/CodeTab.svelte", "settings/EditorRows.svelte", "settings/FontRows.svelte"],
				groups: [
					{
						items: [{ id: "buffer-font" }, { id: "buffer-size" }, { id: "buffer-line-height" }],
					},
					{
						items: [
							{ id: "lsp-enable" },
							{ id: "lsp-format-on-save" },
							{ id: "lsp-inlay-hints" },
							{ id: "lsp-manage" },
						],
					},
					{
						items: [
							{ id: "editor-tab-size" },
							{ id: "editor-insert-spaces" },
							{ id: "editor-trim-whitespace" },
							{ id: "editor-final-newline" },
							{ id: "editor-auto-reload" },
							{ id: "editor-large-file" },
						],
					},
					{
						items: [
							{ id: "editor-word-wrap" },
							{ id: "editor-line-numbers" },
							{ id: "editor-highlight-line" },
							{ id: "editor-whitespace" },
							{ id: "editor-indent-guides" },
							{ id: "editor-bracket-colors" },
							{ id: "editor-sticky-scroll" },
							{ id: "editor-minimap" },
							{ id: "editor-rulers" },
							{ id: "editor-scroll-past-end" },
							{ id: "editor-smooth-scroll" },
						],
					},
				],
			},
			{
				id: "languages",
				aliases: ["appearance/languages", "languages"],
				files: ["settings/LanguagesTab.svelte", "settings/ServerRow.svelte", "settings/CustomServer.svelte"],
				groups: [
					{
						items: [{ id: "lsp-servers" }, { id: "lsp-custom" }, { id: "lsp-project" }],
					},
				],
			},
		],
	},
	{
		id: "terminal",
		cluster: "work",
		aliases: ["appearance/terminal"],
		pages: [
			{
				files: ["settings/TerminalTab.svelte"],
				groups: [
					{
						items: [{ id: "gpu", macOnly: true }, { id: "host-restart" }],
					},
					{
						items: [
							{ id: "term-font" },
							{ id: "term-size" },
							{ id: "font-thicken" },
							{ id: "ligatures", macOnly: true },
							{ id: "contrast", macOnly: true },
						],
					},
					{
						items: [{ id: "cursor-shape" }, { id: "cursor-blink" }],
					},
					{
						items: [{ id: "term-padding-x" }, { id: "term-padding-y" }],
					},
					{
						items: [
							{ id: "scrollback" },
							{ id: "scrollback-memory", macOnly: true },
							{ id: "clipboard" },
							{ id: "copy-on-select" },
							{ id: "click-to-move", macOnly: true },
							{ id: "option-as-alt", macOnly: true },
						],
					},
				],
			},
		],
	},
	{
		id: "git",
		cluster: "work",
		pages: [
			{
				files: ["components/GitSettings.svelte"],
				groups: [
					{
						items: [{ id: "git-commit-agent" }],
					},
				],
			},
		],
	},
	{
		id: "agents",
		cluster: "work",
		pages: [
			{
				files: ["components/AgentSettings.svelte", "components/settings/DockSettings.svelte"],
				groups: [
					{
						items: [{ id: "agent-default" }, { id: "agent-permissions" }, { id: "agent-args" }],
					},
					{
						items: [{ id: "dock-enabled" }, { id: "dock-hotkey" }, { id: "dock-toggle-hotkey" }],
					},
					{
						items: [{ id: "agent-custom" }],
					},
					{
						items: [{ id: "quick" }],
					},
				],
			},
		],
	},
	{
		id: "integrations",
		cluster: "connect",
		pages: [
			{
				id: "shell",
				files: ["components/settings/ShellLinks.svelte", "components/settings/LinksSettings.svelte"],
				groups: [
					{
						items: [{ id: "shell" }],
					},
					{
						items: [{ id: "browser" }],
					},
				],
			},
			{
				id: "agents",
				files: ["components/settings/AgentHooks.svelte"],
				groups: [
					{
						items: [{ id: "agent-integration", aliases: ["agent-hooks"] }],
					},
				],
			},
			{
				id: "ssh",
				aliases: ["ssh"],
				files: ["components/SshSettings.svelte"],
				groups: [
					{
						items: [{ id: "ssh-hosts" }],
					},
					{
						items: [{ id: "ssh-project" }],
					},
				],
			},
			{
				id: "mobile",
				aliases: ["mobile"],
				files: [
					"components/MobileSettings.svelte",
					"components/mobile/PairCard.svelte",
					"components/mobile/PhoneRow.svelte",
				],
				groups: [
					{
						items: [{ id: "mobile-pair" }],
					},
					{
						items: [{ id: "mobile-devices" }],
					},
				],
			},
		],
	},
	{
		id: "crew",
		cluster: "connect",
		pages: [
			{
				files: ["components/CrewSettings.svelte"],
				groups: [
					{
						items: [{ id: "crew-add" }],
					},
					{
						items: [{ id: "crew-github" }],
					},
					{
						items: [{ id: "short-replies" }],
					},
					{
						items: [{ id: "crew-dictation-key" }],
					},
					{
						items: [{ id: "crew-onboarding" }],
					},
				],
			},
		],
	},
] as const satisfies readonly CategoryIds[];

/** The registry's structure with its literal ids, from which the section and tab id types are derived. */
export type RegistryDefs = typeof IDS;
