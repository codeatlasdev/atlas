/**
 * The text of the Settings registry: every label, description, group title and search keyword, one entry per id of
 * `registryIds.ts`. All of it is a function that reads the catalog when something shows it, so the sidebar, the page,
 * the tab strip and the search follow the display language live. This module (with its ~280 messages) loads with the
 * Settings view; startup code only needs `registryIds.ts`. `registry.ts` joins the two.
 * `keywords` stay plain strings on purpose: they hold words in every language people may type.
 */
import { kbd } from "#lib/keys.ts";
import { m } from "#lib/i18n/m.ts";
import { CATEGORIES as SOUND_CATEGORIES } from "#lib/sound/categories.ts";
import { SPECS } from "#lib/sound/policy.ts";

export interface Text {
	readonly label: () => string;
	readonly description: () => string;
}
export interface ItemText extends Text {
	readonly keywords?: string;
}

/** Category text by category id. */
export const CATEGORY_TEXT: Readonly<Record<string, Text>> = {
	general: {
		label: () => m.settings_registry_general_label(),
		description: () => m.settings_registry_general_description(),
	},
	appearance: {
		label: () => m.settings_registry_appearance_label(),
		description: () => m.settings_registry_appearance_description(),
	},
	notifications: {
		label: () => m.settings_registry_notifications_label(),
		description: () => m.settings_registry_notifications_description(),
	},
	capture: {
		label: () => m.settings_registry_capture_label(),
		description: () => m.settings_registry_capture_description(),
	},
	keyboard: {
		label: () => m.settings_registry_keyboard_label(),
		description: () => m.settings_registry_keyboard_description(),
	},
	help: {
		label: () => m.settings_registry_help_label(),
		description: () => m.settings_registry_help_description(),
	},
	workspaces: {
		label: () => m.settings_registry_workspaces_label(),
		description: () => m.settings_registry_workspaces_description(),
	},
	editor: {
		label: () => m.settings_registry_editor_label(),
		description: () => m.settings_registry_editor_description(),
	},
	terminal: {
		label: () => m.settings_registry_terminal_label(),
		description: () => m.settings_registry_terminal_description(),
	},
	git: {
		label: () => m.settings_registry_git_label(),
		description: () => m.settings_registry_git_description(),
	},
	agents: {
		label: () => m.settings_registry_agents_label(),
		description: () => m.settings_registry_agents_description(),
	},
	integrations: {
		label: () => m.settings_registry_integrations_label(),
		description: () => m.settings_registry_integrations_description(),
	},
	crew: {
		label: () => m.settings_registry_crew_label(),
		description: () => m.settings_registry_crew_description(),
	},
};

/** Tab text by `<category>/<tab>`. */
export const PAGE_TEXT: Readonly<Record<string, Text>> = {
	"editor/editor": {
		label: () => m.settings_registry_editor_label(),
		description: () => m.settings_registry_editor_tab_editor_description(),
	},
	"editor/languages": {
		label: () => m.settings_registry_editor_tab_languages_label(),
		description: () => m.settings_registry_editor_tab_languages_description(),
	},
	"integrations/shell": {
		label: () => m.settings_registry_integrations_tab_shell_label(),
		description: () => m.settings_registry_integrations_tab_shell_description(),
	},
	"integrations/agents": {
		label: () => m.settings_general_agent_hooks_agent_hooks(),
		description: () => m.settings_registry_integrations_tab_agents_description(),
	},
	"integrations/ssh": {
		label: () => m.settings_registry_integrations_tab_ssh_label(),
		description: () => m.settings_registry_integrations_tab_ssh_description(),
	},
	"integrations/mobile": {
		label: () => m.settings_registry_integrations_tab_mobile_label(),
		description: () => m.settings_registry_integrations_tab_mobile_description(),
	},
};

/** Group titles by `<category>#<page index>#<group index>`. */
export const GROUP_TITLE: Readonly<Record<string, () => string>> = {
	"general#0#0": () => m.settings_registry_general_group_updates(),
	"general#0#1": () => m.settings_group_language(),
	"appearance#0#0": () => m.settings_registry_appearance_group_color_theme(),
	"appearance#0#1": () => m.settings_registry_appearance_group_zoom(),
	"appearance#0#2": () => m.settings_registry_appearance_group_interface_font(),
	"appearance#0#3": () => m.settings_registry_appearance_group_file_panel(),
	"appearance#0#4": () => m.settings_registry_appearance_group_split_panes(),
	"notifications#0#0": () => m.settings_general_notification_notifications(),
	"notifications#0#1": () => m.settings_general_sound_sounds(),
	"capture#0#0": () => m.capture_quick_page_capture(),
	"capture#0#1": () => m.settings_capture_group_hotkeys(),
	"capture#0#2": () => m.settings_capture_group_screenshots(),
	"capture#0#3": () => m.capture_quick_title_recording(),
	"capture#0#4": () => m.settings_capture_group_after(),
	"capture#0#5": () => m.settings_registry_capture_group_history(),
	"keyboard#0#0": () => m.settings_registry_keyboard_group_shortcuts(),
	"help#0#0": () => m.settings_registry_help_group_help(),
	"workspaces#0#0": () => m.settings_general_workspace_new_worktrees(),
	"workspaces#0#1": () => m.settings_general_workspace_auto_clean_up(),
	"workspaces#0#2": () => m.settings_general_workspace_clean_up(),
	"workspaces#0#3": () => m.settings_general_hygiene_group_hygiene(),
	"editor#0#0": () => m.settings_editor_code_tab_code_font(),
	"editor#0#1": () => m.settings_editor_code_tab_language_servers(),
	"editor#0#2": () => m.settings_registry_editor_group_editing(),
	"editor#0#3": () => m.settings_editor_rows_display(),
	"editor#1#0": () => m.settings_editor_code_tab_language_servers(),
	"terminal#0#0": () => m.settings_terminal_tab_renderer(),
	"terminal#0#1": () => m.settings_terminal_tab_text(),
	"terminal#0#2": () => m.settings_terminal_tab_cursor(),
	"terminal#0#3": () => m.settings_terminal_tab_padding(),
	"terminal#0#4": () => m.settings_terminal_tab_behavior(),
	"git#0#0": () => m.settings_general_git_commit_messages(),
	"agents#0#0": () => m.settings_agents_agent_coding_agents(),
	"agents#0#1": () => m.settings_general_dock_agent_dock(),
	"agents#0#2": () => m.settings_agents_agent_custom_agents(),
	"agents#0#3": () => m.settings_agents_agent_quick_commands(),
	"integrations#0#0": () => m.settings_registry_integrations_group_shell_environment(),
	"integrations#0#1": () => m.settings_general_links_links(),
	"integrations#1#0": () => m.settings_general_agent_hooks_agent_hooks(),
	"integrations#2#0": () => m.settings_registry_integrations_group_hosts(),
	"integrations#2#1": () => m.settings_registry_integrations_group_remote_projects(),
	"integrations#3#0": () => m.settings_registry_integrations_group_pairing(),
	"integrations#3#1": () => m.settings_registry_integrations_group_devices(),
	"crew#0#0": () => m.settings_registry_crew_group_crew_servers(),
	"crew#0#1": () => m.settings_registry_crew_group_github(),
	"crew#0#2": () => m.crew_settings_behaviour_card_short_replies(),
	"crew#0#3": () => m.settings_agents_crew_onboarding(),
};

/** Row text by row id. */
export const ITEM_TEXT: Readonly<Record<string, ItemText>> = {
	updates: {
		label: m.settings_registry_general_group_updates,
		description: m.settings_registry_general_updates_description,
		keywords: "version upgrade release cli app atualizar atualização versão baixar instalar",
	},
	"update-channel": {
		label: m.settings_registry_general_update_channel_label,
		description: m.settings_registry_general_update_channel_description,
		keywords:
			"beta alpha stable prerelease pre-release rc nightly insiders downgrade release train canal estável prévia pré-lançamento versão",
	},
	"display-language": {
		label: m.settings_display_language_label,
		description: m.settings_display_language_description,
		keywords: "language idioma portugues locale translation língua português inglês tradução",
	},
	"theme-mode": {
		label: m.settings_registry_appearance_theme_mode_label,
		description: m.settings_registry_appearance_theme_mode_description,
		keywords: "dark light mode system aparência claro escuro automático sistema",
	},
	"theme-light": {
		label: m.settings_registry_appearance_theme_light_label,
		description: m.settings_registry_appearance_theme_light_description,
		keywords: "alucard zed json aparência claro",
	},
	"theme-dark": {
		label: m.settings_registry_appearance_theme_dark_label,
		description: m.settings_registry_appearance_theme_dark_description,
		keywords: "dracula zed json aparência escuro",
	},
	"theme-terminal": {
		label: m.settings_registry_appearance_theme_terminal_label,
		description: m.settings_registry_appearance_theme_terminal_description,
		keywords: "dracula alucard colors ansi palette aparência cores paleta terminal",
	},
	"theme-import": {
		label: m.settings_registry_appearance_theme_import_label,
		description: m.settings_registry_appearance_theme_import_description,
		keywords:
			"ghostty iterm itermcolors vscode vsix base16 alacritty yaml toml zip package folder dracula pro zeno alucard import zed aparência importar arquivo pasta",
	},
	"theme-export": {
		label: m.settings_registry_appearance_theme_export_label,
		description: m.settings_registry_appearance_theme_export_description,
		keywords:
			"ghostty iterm vscode base16 dracula alucard export share copy save reveal folder aparência exportar compartilhar copiar salvar",
	},
	accent: {
		label: m.settings_registry_appearance_accent_label,
		description: m.settings_registry_appearance_accent_description,
		keywords: "brand color swatch aparência cor destaque realce marca",
	},
	"ui-zoom": {
		label: m.settings_registry_appearance_group_zoom,
		description: () =>
			m.settings_registry_appearance_ui_zoom_description({
				zoomIn: kbd("cmd-="),
				zoomOut: kbd("cmd--"),
				reset: kbd("cmd-0"),
			}),
		keywords:
			"scale magnify bigger smaller zoom in out reset layout size aparência ampliar reduzir aumentar diminuir escala tamanho tela",
	},
	"ui-font": {
		label: m.settings_registry_appearance_group_interface_font,
		description: m.settings_registry_appearance_ui_font_description,
		keywords: "ui_font_family typeface aparência fonte letra tipografia",
	},
	"ui-size": {
		label: m.settings_registry_appearance_ui_size_label,
		description: m.settings_registry_appearance_ui_size_description,
		keywords: "ui_font_size aparência tamanho fonte letra texto",
	},
	"panel-dock": {
		label: m.settings_registry_appearance_panel_dock_label,
		description: m.settings_registry_appearance_panel_dock_description,
		keywords: "project_panel dock explorer aparência painel arquivos lado esquerda direita árvore explorador",
	},
	"panel-indent": {
		label: m.settings_registry_appearance_panel_indent_label,
		description: m.settings_registry_appearance_panel_indent_description,
		keywords: "indent_size tree aparência recuo indentação árvore pastas",
	},
	"panel-file-icons": {
		label: m.settings_registry_appearance_panel_file_icons_label,
		description: m.settings_registry_appearance_panel_file_icons_description,
		keywords: "file_icons aparência ícones arquivos",
	},
	"panel-folder-icons": {
		label: m.settings_registry_appearance_panel_folder_icons_label,
		description: m.settings_registry_appearance_panel_folder_icons_description,
		keywords: "folder_icons aparência ícones pastas",
	},
	"pane-border": {
		label: m.settings_registry_appearance_pane_border_label,
		description: m.settings_registry_appearance_pane_border_description,
		keywords: "active_pane_modifiers border_size focus aparência borda painel foco contorno",
	},
	"pane-opacity": {
		label: m.settings_registry_appearance_pane_opacity_label,
		description: m.settings_registry_appearance_pane_opacity_description,
		keywords: "inactive_opacity fade aparência opacidade transparência atenuar painéis inativos",
	},
	notify: {
		label: m.settings_general_notification_notifications,
		description: m.settings_registry_notifications_notify_description,
		keywords: "sound alert bell focus notificação notificações aviso alerta som",
	},
	sounds: {
		label: m.settings_general_sound_sounds,
		description: m.settings_registry_notifications_sounds_description,
		keywords:
			"audio sound effects earcon chime notification mute silent som sons áudio efeitos sonoros mudo silêncio",
	},
	"sound-volume": {
		label: m.settings_general_sound_sound_volume,
		description: m.settings_registry_notifications_sound_volume_description,
		keywords: "loudness quiet audio level volume som áudio alto baixo",
	},
	"sound-reduce": {
		label: m.settings_general_sound_reduce_sounds,
		description: m.settings_registry_notifications_sound_reduce_description,
		keywords: "quiet minimal less fewer silencioso discreto menos sons",
	},
	"capture-permission": {
		label: m.settings_capture_permission_label,
		description: m.settings_capture_permission_description,
		keywords:
			"capture screenshot screen recording permission privacy grant system settings captura print tela gravação permissão privacidade acesso",
	},
	"capture-enabled": {
		label: m.settings_capture_enabled_label,
		description: m.settings_capture_enabled_description,
		keywords:
			"capture screenshot screen recording gif hotkey enable disable captura print tela gravação atalho ativar desativar",
	},
	"capture-folder": {
		label: m.settings_capture_folder_label,
		description: m.settings_capture_folder_description,
		keywords:
			"capture screenshot screen recording gif folder directory pictures path captura print tela gravação pasta diretório imagens caminho salvar",
	},
	"capture-preset": {
		label: m.settings_capture_preset_label,
		description: m.settings_capture_preset_description,
		keywords:
			"capture screenshot hotkey preset macos cleanshot shortcuts keys captura print tela gravação atalhos teclas conjunto predefinição",
	},
	"capture-system-keys": {
		label: m.settings_capture_system_keys_label,
		description: () =>
			m.settings_capture_system_keys_description({
				screen: kbd("cmd-shift-3"),
				area: kbd("cmd-shift-4"),
				toolbar: kbd("cmd-shift-5"),
				touchBar: kbd("cmd-shift-6"),
			}),
		keywords:
			"capture screenshot macos system shortcuts keyboard shift command 3 4 5 6 disable free captura print tela gravação atalhos teclado sistema desativar liberar",
	},
	"capture-key-all-in-one": {
		label: m.settings_capture_key_all_in_one,
		description: m.settings_capture_key_all_in_one_description,
		keywords:
			"capture screenshot all in one toolbar menu hotkey shortcut captura print tela gravação tudo em um barra de ferramentas atalho",
	},
	"capture-key-region": {
		label: m.capture_overlay_page_mode_region,
		description: m.settings_registry_capture_capture_key_region_description,
		keywords:
			"capture screenshot screen region area hotkey shortcut captura print tela gravação região área atalho",
	},
	"capture-key-previous": {
		label: m.capture_overlay_page_previous_area,
		description: m.settings_capture_key_previous_description,
		keywords:
			"capture screenshot previous last area repeat region hotkey shortcut captura print tela gravação última anterior repetir região atalho",
	},
	"capture-key-window": {
		label: m.capture_overlay_page_mode_window,
		description: m.settings_capture_key_window_description,
		keywords: "capture screenshot window hotkey shortcut captura print tela gravação janela atalho",
	},
	"capture-key-fullscreen": {
		label: m.capture_overlay_page_mode_screen,
		description: m.settings_capture_key_fullscreen_description,
		keywords:
			"capture screenshot fullscreen full screen display hotkey shortcut captura print tela gravação inteira cheia atalho",
	},
	"capture-key-ocr": {
		label: m.capture_quick_page_ocr,
		description: m.settings_capture_key_ocr_description,
		keywords:
			"capture screenshot text ocr recognize copy hotkey shortcut captura print tela gravação texto reconhecer ler copiar atalho",
	},
	"capture-key-record": {
		label: m.capture_overlay_page_mode_record,
		description: m.settings_capture_key_record_description,
		keywords:
			"capture screen recording video record hotkey shortcut captura print tela gravação vídeo gravar atalho",
	},
	"capture-key-gif": {
		label: m.settings_capture_key_gif,
		description: m.settings_capture_key_gif_description,
		keywords:
			"capture screen recording gif animated hotkey shortcut captura print tela gravação animado atalho",
	},
	"capture-on-release": {
		label: m.settings_capture_on_release_label,
		description: m.settings_registry_capture_capture_on_release_description,
		keywords:
			"capture screenshot region release mouse instant enter return confirm adjust captura print tela gravação região soltar instantâneo confirmar ajustar",
	},
	"capture-timer": {
		label: m.settings_capture_timer_label,
		description: m.settings_capture_timer_description,
		keywords:
			"capture screenshot timer delay countdown seconds captura print tela gravação temporizador atraso contagem regressiva segundos",
	},
	"capture-scale": {
		label: m.settings_capture_scale_label,
		description: m.settings_capture_scale_description,
		keywords:
			"capture screenshot resolution retina scale 1x 2x pixels size captura print tela gravação resolução escala tamanho",
	},
	"capture-hide-icons": {
		label: m.settings_capture_hide_icons_label,
		description: m.settings_capture_hide_icons_description,
		keywords:
			"capture screenshot screen recording desktop icons hide clean captura print tela gravação ícones área de trabalho ocultar limpa",
	},
	"capture-window-shadow": {
		label: m.settings_capture_window_shadow_label,
		description: m.settings_capture_window_shadow_description,
		keywords:
			"capture screenshot window shadow transparent captura print tela gravação janela sombra transparente",
	},
	"capture-window-padding": {
		label: m.settings_capture_window_background_label,
		description: m.settings_capture_window_background_description,
		keywords:
			"capture screenshot window background padding backdrop captura print tela gravação janela fundo margem",
	},
	"capture-quality": {
		label: m.settings_capture_quality_label,
		description: m.settings_capture_quality_description,
		keywords:
			"capture screen recording video quality high medium low size bitrate captura print tela gravação vídeo qualidade alta média baixa tamanho",
	},
	"capture-clicks": {
		label: m.settings_capture_clicks_label,
		description: m.settings_capture_clicks_description,
		keywords:
			"capture screen recording gif click ring mouse highlight captura print tela gravação cliques anel destaque",
	},
	"capture-keystrokes": {
		label: m.settings_capture_keystrokes_label,
		description: m.settings_capture_keystrokes_description,
		keywords:
			"capture screen recording keystrokes keys keyboard pressed input monitoring captura print tela gravação teclas digitadas teclado pressionadas",
	},
	"capture-system-audio": {
		label: m.settings_capture_system_audio_label,
		description: m.settings_capture_system_audio_description,
		keywords:
			"capture screen recording system sound audio computer captura print tela gravação áudio som sistema computador",
	},
	"capture-microphone": {
		label: m.settings_capture_microphone_label,
		description: m.settings_capture_microphone_description,
		keywords:
			"capture screen recording microphone mic voice audio narration captura print tela gravação microfone voz áudio narração",
	},
	"capture-copy": {
		label: m.capture_editor_page_copy_to_clipboard,
		description: m.settings_capture_copy_description,
		keywords: "capture screenshot clipboard copy captura print tela gravação copiar área de transferência",
	},
	"capture-quick-access": {
		label: m.settings_capture_quick_access_label,
		description: m.settings_capture_quick_access_description,
		keywords:
			"capture screenshot quick access overlay thumbnail card annotate captura print tela gravação acesso rápido sobreposição miniatura cartão anotar",
	},
	"capture-dismiss": {
		label: m.settings_capture_dismiss_label,
		description: m.settings_capture_dismiss_description,
		keywords:
			"capture screenshot quick access overlay seconds timer never captura print tela gravação acesso rápido sobreposição segundos fechar nunca",
	},
	"capture-corner": {
		label: m.settings_capture_corner_label,
		description: m.settings_capture_corner_description,
		keywords:
			"capture screenshot quick access overlay corner position bottom top left right captura print tela gravação acesso rápido sobreposição canto posição",
	},
	"capture-stack": {
		label: m.settings_capture_stack_label,
		description: m.settings_capture_stack_description,
		keywords:
			"capture screenshot quick access overlay stack cards number limit captura print tela gravação acesso rápido sobreposição pilha cartões limite",
	},
	"capture-sound": {
		label: m.settings_capture_sound_label,
		description: m.settings_capture_sound_description,
		keywords: "capture screenshot sound shutter audio captura print tela gravação som obturador áudio",
	},
	"capture-highlight": {
		label: m.settings_capture_highlight_label,
		description: m.settings_capture_highlight_description,
		keywords:
			"capture screen recording gif cursor pointer click highlight captura print tela gravação ponteiro clique destaque",
	},
	"capture-history-size": {
		label: m.settings_capture_history_size_label,
		description: m.settings_capture_history_size_description,
		keywords:
			"capture screenshot screen recording gif history size limit keep captura print tela gravação histórico tamanho limite guardar",
	},
	"capture-history": {
		label: m.settings_capture_recent_label,
		description: m.settings_capture_recent_description,
		keywords:
			"capture screenshot screen recording gif history list recent quick access folder captura print tela gravação histórico lista recentes acesso rápido pasta",
	},
	keymap: {
		label: m.settings_registry_keyboard_label,
		description: m.settings_registry_keyboard_description,
		keywords: "keybindings keymap hotkeys bind atalhos teclas teclado comandos",
	},
	tour: {
		label: m.settings_registry_help_tour_label,
		description: m.settings_registry_help_tour_description,
		keywords: "onboarding intro tutorial introdução passeio",
	},
	"help-keys": {
		label: m.settings_registry_help_help_keys_label,
		description: m.settings_registry_help_help_keys_description,
		keywords: "keyboard help ajuda atalhos teclado",
	},
	checklist: {
		label: m.settings_registry_help_checklist_label,
		description: m.settings_registry_help_checklist_description,
		keywords: "onboarding progress primeiros passos progresso introdução",
	},
	"ws-dir": {
		label: m.settings_general_workspace_worktree_folder,
		description: m.settings_registry_workspaces_ws_dir_description,
		keywords: "workspace directory path location pasta diretório caminho local",
	},
	"ws-nest": {
		label: m.settings_general_workspace_group_by_repository,
		description: m.settings_registry_workspaces_ws_nest_description,
		keywords: "nest agrupar pasta repositório",
	},
	"ws-prefix": {
		label: m.settings_general_workspace_branch_name_prefix,
		description: m.settings_registry_workspaces_ws_prefix_description,
		keywords: "git user.name custom branch prefixo nome personalizado",
	},
	"ws-autoclean": {
		label: m.settings_general_workspace_remove_worktrees_automatically,
		description: m.settings_registry_workspaces_ws_autoclean_description,
		keywords:
			"auto delete merged idle stale prune restore limpeza automática excluir remover merge inativa parada restaurar",
	},
	"ws-cleanup": {
		label: m.settings_registry_workspaces_ws_cleanup_label,
		description: m.settings_registry_workspaces_ws_cleanup_description,
		keywords: "delete remove disk scan limpar excluir apagar disco analisar espaço",
	},
	"ws-hygiene": {
		label: m.settings_general_hygiene_group_orphans_and_disk,
		description: m.settings_registry_workspaces_ws_hygiene_description,
		keywords:
			"orphan zombie leftover browser chromium vite process kill disk space cache temp spotlight index mdworker cleanup órfão zumbi processos sobra navegador disco espaço temporário limpeza",
	},
	"buffer-font": {
		label: m.settings_editor_code_tab_code_font,
		description: m.settings_editor_code_tab_code_font_hint,
		keywords: "buffer_font_family monospace typeface fonte código letra monoespaçada tipografia",
	},
	"buffer-size": {
		label: m.settings_editor_code_tab_code_font_size,
		description: m.settings_registry_editor_buffer_size_description,
		keywords: "buffer_font_size tamanho fonte código letra",
	},
	"buffer-line-height": {
		label: m.settings_editor_code_tab_line_spacing,
		description: m.settings_registry_editor_buffer_line_height_description,
		keywords: "buffer_line_height leading espaçamento altura linha",
	},
	"lsp-enable": {
		label: m.settings_editor_code_tab_language_servers,
		description: m.settings_editor_code_tab_completion_diagnostics_hover_and_go,
		keywords:
			"lsp enable_language_servers rust-analyzer gopls typescript intellisense autocomplete linguagem servidor autocompletar sugestões",
	},
	"lsp-format-on-save": {
		label: m.settings_editor_code_tab_format_on_save,
		description: m.settings_editor_code_tab_format_the_file_with_its,
		keywords: "format_on_save formatter prettier rustfmt formatar salvar formatador",
	},
	"lsp-inlay-hints": {
		label: m.settings_editor_code_tab_inlay_hints,
		description: m.settings_editor_code_tab_show_inferred_types_and_parameter,
		keywords: "inlay_hints types parameters dicas tipos parâmetros",
	},
	"lsp-manage": {
		label: m.settings_editor_code_tab_manage_language_servers,
		description: m.settings_editor_code_tab_install_update_or_replace_the,
		keywords:
			"languages custom command lsp gopls rust-analyzer typescript server install uninstall linguagens servidor instalar desinstalar personalizado comando",
	},
	"editor-tab-size": {
		label: m.settings_editor_rows_tab_size,
		description: m.settings_editor_rows_columns_a_tab_or_one,
		keywords: "tab_size indent indentation width tabulação indentação largura espaços",
	},
	"editor-insert-spaces": {
		label: m.settings_editor_rows_indent_with_spaces,
		description: m.settings_editor_rows_pressing_tab_inserts_spaces_turn,
		keywords: "insert_spaces tabs spaces indentation tabulação espaços indentação",
	},
	"editor-trim-whitespace": {
		label: m.settings_editor_rows_trim_trailing_whitespace_on_save,
		description: m.settings_editor_rows_remove_spaces_and_tabs_at,
		keywords: "trim_trailing_whitespace_on_save strip espaços fim linha remover salvar",
	},
	"editor-final-newline": {
		label: m.settings_editor_rows_add_a_final_newline_on,
		description: m.settings_editor_rows_end_every_file_with_exactly,
		keywords: "insert_final_newline_on_save eof end of file quebra de linha final fim arquivo",
	},
	"editor-auto-reload": {
		label: m.settings_editor_rows_reload_files_changed_on_disk,
		description: m.settings_editor_rows_update_an_open_file_when,
		keywords: "auto_reload external changes refresh watch recarregar atualizar alterações externas disco",
	},
	"editor-large-file": {
		label: m.settings_editor_rows_large_file_limit,
		description: m.settings_editor_rows_files_bigger_than_this_open,
		keywords:
			"large_file_threshold_bytes performance slow big huge arquivo grande limite desempenho lento pesado",
	},
	"editor-word-wrap": {
		label: m.settings_editor_rows_word_wrap,
		description: m.settings_editor_rows_wrap_long_lines_at_the,
		keywords: "word_wrap soft wrap long lines quebra automática linha longas",
	},
	"editor-line-numbers": {
		label: m.settings_editor_rows_line_numbers,
		description: m.settings_editor_rows_show_line_numbers_in_the,
		keywords: "line_numbers gutter números linha margem",
	},
	"editor-highlight-line": {
		label: m.settings_editor_rows_highlight_the_current_line,
		description: m.settings_editor_rows_tint_the_line_the_cursor,
		keywords: "highlight_active_line cursor line destacar linha atual",
	},
	"editor-whitespace": {
		label: m.settings_editor_rows_show_whitespace,
		description: m.settings_editor_rows_draw_dots_for_spaces_and,
		keywords: "render_whitespace invisible characters espaços em branco invisíveis caracteres",
	},
	"editor-indent-guides": {
		label: m.settings_editor_rows_indent_guides,
		description: m.settings_editor_rows_draw_a_faint_vertical_line,
		keywords: "indent_guides guias indentação",
	},
	"editor-bracket-colors": {
		label: m.settings_editor_rows_color_matching_brackets,
		description: m.settings_editor_rows_give_each_level_of_nested,
		keywords: "bracket_pair_colorization rainbow brackets colchetes cores arco-íris parênteses",
	},
	"editor-sticky-scroll": {
		label: m.settings_editor_rows_sticky_scroll,
		description: m.settings_editor_rows_keep_the_enclosing_function_or,
		keywords: "sticky_scroll scope header rolagem fixa cabeçalho escopo",
	},
	"editor-minimap": {
		label: m.settings_editor_rows_minimap,
		description: m.settings_editor_rows_show_a_small_overview_of,
		keywords: "overview minimapa visão geral",
	},
	"editor-rulers": {
		label: m.settings_editor_rows_rulers,
		description: m.settings_editor_rows_columns_where_a_vertical_line,
		keywords: "column guide line length 80 100 120 réguas coluna guia tamanho linha",
	},
	"editor-scroll-past-end": {
		label: m.settings_editor_rows_scroll_past_the_last_line,
		description: m.settings_editor_rows_let_the_last_line_scroll,
		keywords: "scroll_past_end rolar além última linha fim",
	},
	"editor-smooth-scroll": {
		label: m.settings_editor_rows_smooth_scrolling,
		description: m.settings_editor_rows_animate_scrolling_when_you_jump,
		keywords: "smooth_scrolling animation rolagem suave animação",
	},
	"lsp-servers": {
		label: m.settings_editor_code_tab_language_servers,
		description: m.settings_registry_editor_lsp_servers_description,
		keywords:
			"language server typescript gopls rust-analyzer pyright clangd install uninstall update restart log path managed servidores linguagem instalar desinstalar atualizar reiniciar caminho",
	},
	"lsp-custom": {
		label: m.settings_registry_editor_lsp_custom_label,
		description: m.settings_registry_editor_lsp_custom_description,
		keywords:
			"custom command args arguments initialization options json override replace binary lsp typescript gopls rust-analyzer personalizado comando argumentos opções inicialização substituir binário servidor",
	},
	"lsp-project": {
		label: m.settings_registry_editor_lsp_project_label,
		description: m.settings_registry_editor_lsp_project_description,
		keywords: "project override .atlas workspace lsp projeto substituir servidor arquivo",
	},
	gpu: {
		label: m.settings_terminal_tab_fast_terminal_gpu,
		description: m.settings_registry_terminal_gpu_description,
		keywords: "gpu renderer native xterm metal placa de vídeo gráfico renderizador rápido nativo",
	},
	"host-restart": {
		label: m.settings_terminal_tab_restart_terminal_host,
		description: m.settings_registry_terminal_host_restart_description,
		keywords: "hostd old upgrade host reiniciar terminal antigo atualizar",
	},
	"term-font": {
		label: m.settings_terminal_tab_terminal_font,
		description: m.settings_registry_terminal_term_font_description,
		keywords: "font_family typeface font-family xterm ghostty nerd variable fonte terminal letra tipografia",
	},
	"term-size": {
		label: m.settings_terminal_tab_terminal_font_size,
		description: m.settings_registry_terminal_term_size_description,
		keywords: "font_size tamanho fonte letra terminal",
	},
	"font-thicken": {
		label: m.settings_terminal_tab_thicken_glyphs,
		description: m.settings_registry_terminal_font_thicken_description,
		keywords: "font_thicken bold weight ghostty engrossar negrito peso glifos",
	},
	ligatures: {
		label: m.settings_terminal_tab_ligatures,
		description: m.settings_registry_terminal_ligatures_description,
		keywords: "ligatures fira ligaduras símbolos",
	},
	contrast: {
		label: m.settings_terminal_tab_minimum_contrast,
		description: m.settings_registry_terminal_contrast_description,
		keywords: "wcag accessibility minimum_contrast readable contraste acessibilidade legível",
	},
	"cursor-shape": {
		label: m.settings_terminal_tab_cursor_shape,
		description: m.settings_terminal_tab_how_the_cursor_looks_in,
		keywords: "block bar underline hollow caret cursor forma bloco barra sublinhado vazado",
	},
	"cursor-blink": {
		label: m.settings_terminal_tab_blinking_cursor,
		description: m.settings_registry_terminal_cursor_blink_description,
		keywords: "caret cursor piscar piscando",
	},
	"term-padding-x": {
		label: m.settings_terminal_tab_horizontal_padding,
		description: m.settings_registry_terminal_term_padding_x_description,
		keywords: "padding_x margin inset window-padding-x ghostty margem interna horizontal espaço",
	},
	"term-padding-y": {
		label: m.settings_terminal_tab_vertical_padding,
		description: m.settings_registry_terminal_term_padding_y_description,
		keywords: "padding_y margin inset window-padding-y ghostty margem interna vertical espaço",
	},
	scrollback: {
		label: m.settings_terminal_tab_scrollback,
		description: m.settings_terminal_tab_lines_of_output_each_terminal,
		keywords: "max_scroll_history_lines history buffer histórico linhas rolagem",
	},
	"scrollback-memory": {
		label: m.settings_terminal_tab_scrollback_memory,
		description: m.settings_registry_terminal_scrollback_memory_description,
		keywords: "scrollback_bytes memory limit megabytes mb ghostty scrollback-limit memória limite histórico",
	},
	clipboard: {
		label: m.settings_terminal_tab_programs_can_copy_to_your,
		description: m.settings_registry_terminal_clipboard_description,
		keywords:
			"osc52 osc 52 copy paste ask allow deny área de transferência copiar colar perguntar permitir negar",
	},
	"copy-on-select": {
		label: m.settings_terminal_tab_copy_on_select,
		description: m.settings_registry_terminal_copy_on_select_description,
		keywords: "copy_on_select clipboard selection ghostty copiar ao selecionar área de transferência seleção",
	},
	"click-to-move": {
		label: m.settings_terminal_tab_click_to_move_the_cursor,
		description: m.settings_registry_terminal_click_to_move_description,
		keywords: "click_to_move cursor prompt osc 133 ghostty clicar mover",
	},
	"option-as-alt": {
		label: m.settings_terminal_tab_option_key_as_alt,
		description: () => m.settings_registry_terminal_option_as_alt_description({ key: kbd("alt") }),
		keywords: "option_as_alt macos-option-as-alt meta alt esc ghostty tecla option",
	},
	"git-commit-agent": {
		label: m.settings_general_git_generate_commit_messages_with,
		description: m.settings_registry_git_git_commit_agent_description,
		keywords:
			"ai commit message sparkle generate omp claude codex privacy diff staged mensagem ia agente gerar privacidade stage",
	},
	"agent-default": {
		label: m.settings_agents_agent_default_agent,
		description: m.settings_registry_agents_agent_default_description,
		keywords: "omp claude codex blank agente padrão em branco terminal",
	},
	"agent-permissions": {
		label: m.settings_agents_agent_tool_approval,
		description: m.settings_registry_agents_agent_permissions_description,
		keywords: "permissions yolo manual auto approve permissões aprovação automático aprovar ferramentas",
	},
	"agent-args": {
		label: m.settings_registry_agents_agent_args_label,
		description: m.settings_registry_agents_agent_args_description,
		keywords: "model flags args argumentos extras modelo",
	},
	"dock-enabled": {
		label: m.settings_general_dock_show_the_dock,
		description: m.settings_registry_agents_dock_enabled_description,
		keywords:
			"floating pill overlay widget menu bar always on top hide show dock flutuante pílula janela sempre no topo ocultar mostrar barra de menus",
	},
	"dock-hotkey": {
		label: m.settings_general_dock_message_an_agent,
		description: m.settings_general_dock_opens_the_dock_s_composer,
		keywords: "hotkey shortcut summon global keybinding send prompt atalho tecla enviar mensagem",
	},
	"dock-toggle-hotkey": {
		label: m.settings_general_dock_hide_or_show_the_dock,
		description: m.settings_general_dock_works_even_while_the_dock,
		keywords:
			"hotkey shortcut hide show toggle global keybinding tray atalho tecla ocultar mostrar alternar bandeja",
	},
	"agent-custom": {
		label: m.settings_agents_agent_add_agent,
		description: () => m.settings_registry_agents_agent_custom_description({ key: kbd("cmd-shift-a") }),
		keywords: "custom opencode gemini amp aider cli preset add agente personalizado adicionar executar",
	},
	quick: {
		label: m.settings_agents_agent_quick_commands,
		description: m.settings_registry_agents_quick_description,
		keywords: "shortcuts prompt buttons comandos rápidos botões atalhos",
	},
	shell: {
		label: m.settings_general_shell_links_shell_for_your_projects,
		description: m.settings_registry_integrations_shell_description,
		keywords: "zsh bash fish path environment variables env login shell terminal ambiente variáveis caminho",
	},
	browser: {
		label: m.settings_general_links_open_links_in,
		description: m.settings_registry_integrations_browser_description,
		keywords: "browser system atlas navegador links abrir sistema",
	},
	"agent-integration": {
		label: m.settings_general_agent_hooks_agent_hooks,
		description: m.settings_registry_integrations_agent_integration_description,
		keywords:
			"hooks plugin enable omp claude codex integration extension ganchos ativar integração extensão agente",
	},
	"ssh-hosts": {
		label: m.settings_registry_integrations_tab_ssh_label,
		description: m.settings_registry_integrations_ssh_hosts_description,
		keywords: "remote server hostd tunnel ssh remoto servidor túnel máquina",
	},
	"ssh-project": {
		label: m.settings_agents_ssh_remote_project,
		description: m.settings_registry_integrations_ssh_project_description,
		keywords: "atlas.yaml path remote projeto remoto ssh caminho",
	},
	"mobile-pair": {
		label: m.settings_agents_mobile_title,
		description: m.settings_registry_integrations_mobile_pair_description,
		keywords:
			"iphone android qr phone remote p2p app store testflight vpn warp wifi celular parear pareamento código",
	},
	"mobile-devices": {
		label: m.settings_agents_paired_phones,
		description: m.settings_registry_integrations_mobile_devices_description,
		keywords:
			"revoke unpair rename activity last seen permissions scopes terminal approve celulares pareados revogar desparear renomear atividade permissões",
	},
	"crew-add": {
		label: m.crew_onboarding_primary_add_crew_server,
		description: m.settings_registry_crew_crew_add_description,
		keywords: "ssh server crew agent servidor agentes adicionar conectar",
	},
	"crew-github": {
		label: m.settings_registry_crew_group_github,
		description: m.settings_registry_crew_crew_github_description,
		keywords: "git private repo token oauth pat github repositório privado",
	},
	"short-replies": {
		label: m.crew_settings_behaviour_card_short_replies,
		description: m.settings_registry_crew_short_replies_description,
		keywords: "summary compact ai model cheap resumo respostas curtas modelo barato",
	},
	"crew-onboarding": {
		label: m.settings_agents_crew_replay_onboarding,
		description: m.settings_agents_crew_replay_description,
		keywords: "onboarding intro tour crew atlas crew introdução rever repetir",
	},
	...Object.fromEntries(
		SOUND_CATEGORIES.map((c): [string, ItemText] => [
			`sound-cat-${c}`,
			{
				label: () => SPECS[c].label,
				description: () => SPECS[c].description,
				keywords: "sound earcon chime som áudio efeito sonoro",
			},
		]),
	),
};
