//! The text and shape of every native menu, as plain data built from a language: the macOS menu bar, the tray menu
//! and its capture submenu. Nothing here touches Tauri, so a language is compared with another in a unit test, the
//! live switch re-labels the real items from the same model (`menu::relabel`), and `menu_dump` prints it for the
//! debug bridge.
//!
//! The catalog areas are `native_menu` (menu bar, capture entries) and `native_tray` (tray, dock). A message's `os`
//! select (Quit / Exit, Settings / Preferences) is added by atlas-i18n for the OS the process runs on.

use atlas_i18n::t_in;
use serde::Serialize;

use crate::lifecycle::TrayItem;
use crate::shots::prefs::{Keys, menu_entries};

/// Prefix of the menu ids that carry a frontend command (`cmd:atlas::NewWorktree`); the frontend pushes their
/// shortcut and availability through `menu_sync`.
pub const PREFIX: &str = "cmd:";

pub const ID_OPEN: &str = "open";
pub const ID_TOGGLE: &str = "toggle";
pub const ID_DOCK: &str = "dock";
pub const ID_QUIT: &str = "quit";

/// A system-provided menu item: AppKit acts on it, Atlas only gives it its text (AppKit does not translate the
/// text of a menu item it did not name itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Predefined {
    About,
    Services,
    Hide,
    HideOthers,
    Quit,
    Cut,
    Copy,
    Paste,
    Minimize,
    Maximize,
    Fullscreen,
}

/// One entry of a native menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entry {
    /// A plain item with an id of its own.
    Command {
        id: String,
        label: String,
    },
    Predefined {
        item: Predefined,
        label: String,
    },
    Separator,
    Submenu {
        title: String,
        items: Vec<Entry>,
    },
}

impl Entry {
    fn command(id: &str, label: String) -> Self {
        Entry::Command {
            id: id.to_owned(),
            label,
        }
    }

    fn predefined(item: Predefined, label: String) -> Self {
        Entry::Predefined { item, label }
    }

    fn submenu(title: String, items: Vec<Entry>) -> Self {
        Entry::Submenu { title, items }
    }
}

/// `(frontend command id, label)` of a command item; the id becomes `cmd:<id>`.
fn cmd(id: &str, label: String) -> Entry {
    Entry::command(&format!("{PREFIX}{id}"), label)
}

/// A command item per row; `None` is a separator.
fn rows(rows: Vec<Option<(&str, String)>>) -> Vec<Entry> {
    rows.into_iter()
        .map(|r| match r {
            Some((id, label)) => cmd(id, label),
            None => Entry::Separator,
        })
        .collect()
}

fn file(tag: &str) -> Entry {
    Entry::submenu(
        t_in!(tag, "native_menu_title_file"),
        rows(vec![
            Some((
                "atlas::NewWorktree",
                t_in!(tag, "native_menu_file_new_worktree"),
            )),
            Some((
                "workspace::NewTerminal",
                t_in!(tag, "native_menu_file_new_terminal"),
            )),
            Some((
                "atlas::NewAgentSession",
                t_in!(tag, "native_menu_file_new_agent_session"),
            )),
            Some((
                "atlas::NewBrowserTab",
                t_in!(tag, "native_menu_file_new_browser_tab"),
            )),
            None,
            Some(("workspace::Save", t_in!(tag, "common_action_save"))),
            Some((
                "workspace::SaveAll",
                t_in!(tag, "native_menu_file_save_all"),
            )),
            None,
            Some((
                "pane::CloseActiveItem",
                t_in!(tag, "native_menu_file_close_tab"),
            )),
            Some((
                "pane::CloseOtherItems",
                t_in!(tag, "native_menu_file_close_other_tabs"),
            )),
            Some((
                "pane::CloseAllItems",
                t_in!(tag, "native_menu_file_close_all_tabs"),
            )),
            Some((
                "pane::ReopenClosedItem",
                t_in!(tag, "native_menu_file_reopen_tab"),
            )),
        ]),
    )
}

fn go(tag: &str) -> Entry {
    Entry::submenu(
        t_in!(tag, "native_menu_title_go"),
        rows(vec![
            Some(("atlas::Jump", t_in!(tag, "native_menu_go_jump"))),
            Some(("file_finder::Toggle", t_in!(tag, "native_menu_go_file"))),
            Some(("search::Toggle", t_in!(tag, "native_menu_go_search"))),
            Some((
                "command_palette::Toggle",
                t_in!(tag, "native_menu_go_palette"),
            )),
            None,
            Some((
                "pane::ActivateNextItem",
                t_in!(tag, "native_menu_go_next_tab"),
            )),
            Some((
                "pane::ActivatePreviousItem",
                t_in!(tag, "native_menu_go_previous_tab"),
            )),
            None,
            Some(("atlas::Logs", t_in!(tag, "native_menu_go_logs"))),
            Some(("atlas::Config", t_in!(tag, "native_menu_go_config"))),
            Some(("git::Diff", t_in!(tag, "native_menu_go_changes"))),
            Some(("git::Branch", t_in!(tag, "native_menu_go_switch_branch"))),
            Some((
                "atlas::RestartService",
                t_in!(tag, "native_menu_go_restart_service"),
            )),
        ]),
    )
}

fn view(tag: &str) -> Entry {
    let mut items = rows(vec![
        Some((
            "shell::ToggleSidebar",
            t_in!(tag, "native_menu_view_toggle_sidebar"),
        )),
        Some((
            "workspace::ToggleLeftDock",
            t_in!(tag, "native_menu_view_toggle_left_dock"),
        )),
        Some((
            "workspace::ToggleRightDock",
            t_in!(tag, "native_menu_view_toggle_right_dock"),
        )),
        Some((
            "workspace::ToggleBottomDock",
            t_in!(tag, "native_menu_view_toggle_bottom_dock"),
        )),
        Some((
            "workspace::ToggleZoom",
            t_in!(tag, "native_menu_view_zoom_pane"),
        )),
        None,
        Some(("shell::ZoomIn", t_in!(tag, "native_menu_view_zoom_in"))),
        Some(("shell::ZoomOut", t_in!(tag, "native_menu_view_zoom_out"))),
        Some((
            "shell::ZoomReset",
            t_in!(tag, "native_menu_view_actual_size"),
        )),
        None,
        Some((
            "pane::SplitRight",
            t_in!(tag, "native_menu_view_split_right"),
        )),
        Some(("pane::SplitDown", t_in!(tag, "native_menu_view_split_down"))),
    ]);
    items.push(Entry::Separator);
    items.push(Entry::predefined(
        Predefined::Fullscreen,
        t_in!(tag, "native_menu_view_enter_full_screen"),
    ));
    Entry::submenu(t_in!(tag, "native_menu_title_view"), items)
}

fn app(tag: &str) -> Entry {
    Entry::submenu(
        "Atlas".to_owned(),
        vec![
            Entry::predefined(Predefined::About, t_in!(tag, "native_menu_app_about")),
            Entry::Separator,
            cmd("zed::OpenSettings", t_in!(tag, "native_menu_app_settings")),
            Entry::Separator,
            Entry::predefined(Predefined::Services, t_in!(tag, "native_menu_app_services")),
            Entry::Separator,
            Entry::predefined(Predefined::Hide, t_in!(tag, "native_menu_app_hide")),
            Entry::predefined(
                Predefined::HideOthers,
                t_in!(tag, "native_menu_app_hide_others"),
            ),
            Entry::Separator,
            Entry::predefined(Predefined::Quit, t_in!(tag, "native_menu_app_quit")),
        ],
    )
}

fn edit(tag: &str) -> Entry {
    Entry::submenu(
        t_in!(tag, "common_action_edit"),
        vec![
            cmd("edit::Undo", t_in!(tag, "common_action_undo")),
            cmd("edit::Redo", t_in!(tag, "native_menu_edit_redo")),
            Entry::Separator,
            Entry::predefined(Predefined::Cut, t_in!(tag, "native_menu_edit_cut")),
            Entry::predefined(Predefined::Copy, t_in!(tag, "common_action_copy")),
            Entry::predefined(Predefined::Paste, t_in!(tag, "native_menu_edit_paste")),
            cmd("edit::SelectAll", t_in!(tag, "native_menu_edit_select_all")),
        ],
    )
}

fn window(tag: &str) -> Entry {
    Entry::submenu(
        t_in!(tag, "native_menu_title_window"),
        vec![
            Entry::predefined(
                Predefined::Minimize,
                t_in!(tag, "native_menu_window_minimize"),
            ),
            Entry::predefined(Predefined::Maximize, t_in!(tag, "native_menu_window_zoom")),
        ],
    )
}

fn help(tag: &str) -> Entry {
    Entry::submenu(
        t_in!(tag, "native_menu_title_help"),
        vec![cmd("atlas::Help", t_in!(tag, "native_menu_help_shortcuts"))],
    )
}

/// The text of a capture action (`capture:<name>`), without its key.
fn capture_label(tag: &str, h: crate::shots::prefs::Hotkey) -> String {
    use crate::shots::prefs::Hotkey;
    match h {
        Hotkey::AllInOne => t_in!(tag, "native_menu_capture_all_in_one"),
        Hotkey::Region => t_in!(tag, "native_menu_capture_region"),
        Hotkey::Previous => t_in!(tag, "native_menu_capture_previous"),
        Hotkey::Window => t_in!(tag, "native_menu_capture_window"),
        Hotkey::Fullscreen => t_in!(tag, "native_menu_capture_fullscreen"),
        Hotkey::Ocr => t_in!(tag, "native_menu_capture_ocr"),
        Hotkey::Record => t_in!(tag, "native_menu_capture_record"),
        Hotkey::Gif => t_in!(tag, "native_menu_capture_gif"),
    }
}

/// The text of a capture menu item: the action and, after a tab-like gap, the key it has now. The key is text, not a
/// menu accelerator: the real hotkey is the global one, and a second registration would fire twice in Atlas.
pub fn capture_text(label: &str, stroke: &str) -> String {
    if stroke.is_empty() {
        label.to_owned()
    } else {
        format!("{label}    {stroke}")
    }
}

/// The "Capture" submenu with the keys in force: the menu bar's and the tray's.
pub fn capture(tag: &str, keys: &Keys) -> Entry {
    let items = menu_entries(keys)
        .into_iter()
        .map(|e| {
            let label = capture_label(tag, e.hotkey);
            Entry::command(&e.id, capture_text(&label, &e.stroke))
        })
        .collect();
    Entry::submenu(t_in!(tag, "native_menu_title_capture"), items)
}

/// The macOS menu bar, left to right. Close Window is deliberately absent: ⌘W is "Close Tab".
pub fn menu_bar(tag: &str, keys: &Keys) -> Vec<Entry> {
    vec![
        app(tag),
        file(tag),
        edit(tag),
        view(tag),
        go(tag),
        capture(tag, keys),
        window(tag),
        help(tag),
    ]
}

/// The tray item's text for a dock that is `shown` now: it names what the click does.
pub fn dock_label(tag: &str, shown: bool) -> String {
    if shown {
        t_in!(tag, "native_tray_dock_hide")
    } else {
        t_in!(tag, "native_tray_dock_show")
    }
}

/// The tray's native menu (structure: [`crate::lifecycle::tray_menu`]).
pub fn tray(tag: &str, linux: bool, dock_shown: bool, keys: &Keys) -> Vec<Entry> {
    crate::lifecycle::tray_menu(linux)
        .iter()
        .map(|item| match item {
            TrayItem::Open => Entry::command(ID_OPEN, t_in!(tag, "native_tray_open")),
            TrayItem::ToggleWindow => Entry::command(ID_TOGGLE, t_in!(tag, "native_tray_toggle")),
            TrayItem::ToggleDock => Entry::command(ID_DOCK, dock_label(tag, dock_shown)),
            TrayItem::Separator => Entry::Separator,
            TrayItem::Capture => capture(tag, keys),
            TrayItem::Quit => Entry::command(ID_QUIT, t_in!(tag, "native_tray_quit")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_i18n::lookup_on;

    /// The text of an entry as a person reads it; empty for a separator.
    fn text(e: &Entry) -> &str {
        match e {
            Entry::Command { label, .. } | Entry::Predefined { label, .. } => label,
            Entry::Submenu { title, .. } => title,
            Entry::Separator => "",
        }
    }

    fn texts(entries: &[Entry]) -> Vec<String> {
        let mut out = Vec::new();
        for e in entries {
            match e {
                Entry::Separator => {}
                Entry::Submenu { title, items } => {
                    out.push(title.clone());
                    out.extend(texts(items));
                }
                other => out.push(text(other).to_owned()),
            }
        }
        out
    }

    /// The shape without the text: what a rebuild for another language must keep (ids, order, separators).
    fn shape(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Command { id, .. } => format!("cmd {id}"),
                Entry::Predefined { item, .. } => format!("predefined {item:?}"),
                Entry::Separator => "-".to_owned(),
                Entry::Submenu { items, .. } => format!("sub [{}]", shape(items).join(", ")),
            })
            .collect()
    }

    fn bar(tag: &str) -> Vec<Entry> {
        menu_bar(tag, &Keys::default())
    }

    #[test]
    fn the_menu_bar_has_the_titles_of_the_language_and_the_same_shape_in_every_language() {
        let titles = |tag| -> Vec<String> { bar(tag).iter().map(|e| text(e).to_owned()).collect() };
        assert_eq!(
            titles("en"),
            [
                "Atlas", "File", "Edit", "View", "Go", "Capture", "Window", "Help"
            ]
        );
        assert_eq!(
            titles("pt-BR"),
            [
                "Atlas",
                "Arquivo",
                "Editar",
                "Visualizar",
                "Ir",
                "Capturar",
                "Janela",
                "Ajuda"
            ]
        );
        assert_eq!(shape(&bar("en")), shape(&bar("pt-BR")));
    }

    #[test]
    fn every_item_of_the_menu_bar_is_translated_and_none_is_left_in_english() {
        let (en, pt) = (texts(&bar("en")), texts(&bar("pt-BR")));
        assert_eq!(en.len(), pt.len());
        // Words that are the same in both languages on purpose: the product, a glossary term, an OS word.
        let same = ["Atlas", "Logs", "Config", "Zoom"];
        for (e, p) in en.iter().zip(&pt) {
            assert!(
                e != p || same.contains(&e.as_str()),
                "{e:?} is not translated"
            );
        }
        assert!(pt.contains(&"Ocultar Atlas".to_owned()));
        assert!(pt.contains(&"Sair do Atlas".to_owned()));
        assert!(pt.contains(&"Ocultar outros".to_owned()));
        assert!(pt.contains(&"Sobre o Atlas".to_owned()));
    }

    #[test]
    fn shared_words_come_from_common_and_never_from_the_menu_area() {
        let edit = |tag| match &bar(tag)[2] {
            Entry::Submenu { items, .. } => texts(items),
            other => panic!("{other:?}"),
        };
        assert_eq!(&edit("en")[0..1], ["Undo"]);
        assert_eq!(&edit("pt-BR")[0..1], ["Desfazer"]);
        assert!(texts(&bar("pt-BR")).contains(&"Copiar".to_owned()));
        assert!(texts(&bar("pt-BR")).contains(&"Salvar".to_owned()));
    }

    #[test]
    fn frontend_commands_keep_their_ids_in_every_language() {
        // `shape` holds every id: equal shapes are equal ids. The ids the frontend and the click handler rely on:
        let shape = shape(&bar("pt-BR")).join("\n");
        for id in [
            "cmd:atlas::NewWorktree",
            "cmd:zed::OpenSettings",
            "cmd:edit::Undo",
            "capture:allInOne",
        ] {
            assert!(shape.contains(id), "{id}");
        }
    }

    // The native menu says each OS's own words: Settings / Preferences, Quit / Exit; pt-BR follows the system's own
    // menus ("Ajustes" on macOS). `os` is a select of the message, so the three platforms are checked from here
    // whatever the machine running the test.
    #[test]
    fn each_os_gets_its_own_word_in_both_languages() {
        let on = |tag, os, key| lookup_on(tag, key, None, os).unwrap();
        for (os, en, pt) in [
            ("mac", "Quit Atlas", "Sair do Atlas"),
            ("windows", "Exit", "Sair"),
            ("linux", "Quit", "Sair"),
        ] {
            assert_eq!(on("en", os, "native_tray_quit"), en, "{os}");
            assert_eq!(on("pt-BR", os, "native_tray_quit"), pt, "{os}");
        }
        for (os, en, pt) in [
            ("mac", "Settings…", "Ajustes…"),
            ("windows", "Settings…", "Configurações…"),
            ("linux", "Preferences…", "Preferências…"),
        ] {
            assert_eq!(on("en", os, "native_menu_app_settings"), en, "{os}");
            assert_eq!(on("pt-BR", os, "native_menu_app_settings"), pt, "{os}");
        }
    }

    #[test]
    fn the_permission_text_names_the_macos_panes_in_the_language() {
        let on = |tag| lookup_on(tag, "native_notify_no_permission", None, "mac").unwrap();
        assert_eq!(
            on("en"),
            "Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas."
        );
        assert_eq!(
            on("pt-BR"),
            "O Atlas precisa da permissão de Gravação de Tela para capturar a tela. Conceda em Ajustes do Sistema > Privacidade e Segurança > Gravação de Tela e reinicie o Atlas."
        );
    }

    #[test]
    fn the_tray_menu_is_rebuilt_for_a_language_and_for_the_dock_state() {
        let keys = Keys::default();
        let en = tray("en", false, true, &keys);
        let pt = tray("pt-BR", false, true, &keys);
        assert_eq!(shape(&en), shape(&pt));
        assert_eq!(
            texts(&pt)[..2],
            [
                "Abrir o Atlas".to_owned(),
                "Ocultar dock de agentes".to_owned()
            ]
        );
        assert_eq!(texts(&en)[..2], ["Open Atlas", "Hide Agent Dock"]);
        // The item names what a click does, in the language.
        assert_eq!(dock_label("en", false), "Show Agent Dock");
        assert_eq!(dock_label("pt-BR", false), "Mostrar dock de agentes");
        assert_eq!(dock_label("pt-BR", true), "Ocultar dock de agentes");
    }

    #[test]
    fn the_linux_tray_toggles_the_window_in_the_language() {
        let keys = Keys::default();
        let pt = tray("pt-BR", true, false, &keys);
        assert!(texts(&pt).contains(&"Mostrar / ocultar o Atlas".to_owned()));
        assert!(!texts(&pt).iter().any(|t| t.starts_with("Capturar")));
    }

    #[test]
    fn capture_entries_carry_the_key_in_force_after_the_translated_action() {
        let keys = Keys {
            region: "ctrl-alt-9".into(),
            window: String::new(),
            ..Keys::default()
        };
        let Entry::Submenu { title, items } = capture("pt-BR", &keys) else {
            panic!("a submenu")
        };
        assert_eq!(title, "Capturar");
        let label = |id: &str| {
            items
                .iter()
                .find_map(|e| match e {
                    Entry::Command { id: i, label } if i == id => Some(label.clone()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(label("capture:region"), "Capturar região    ⌃⌥9");
        assert_eq!(label("capture:window"), "Capturar janela");
        assert_eq!(label("capture:record"), "Gravar tela    ⌃⌥6");
    }

    // The model is what `menu_dump` prints for the debug bridge: its JSON has the texts, not a debug string.
    #[test]
    fn the_model_serializes_for_the_debug_dump() {
        let json = serde_json::to_value(tray("pt-BR", true, false, &Keys::default())).unwrap();
        assert_eq!(json[0]["kind"], "command");
        assert_eq!(json[0]["label"], "Mostrar / ocultar o Atlas");
        assert_eq!(json[2]["kind"], "separator");
    }
}
