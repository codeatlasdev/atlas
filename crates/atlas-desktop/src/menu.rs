//! Native menu bar. Standard App / Window items come from macOS (so ⌘Q ⌘H work everywhere, and
//! ⌘W never reaches the default "Close Window", which would hide the app). Edit keeps the
//! predefined Cut / Copy / Paste (they act on the focused field's selection; the GPU terminal
//! answers `copy` / `paste` events on its input sink). Undo, Redo and Select All are NOT
//! predefined: AppKit would send `undo:` / `redo:` / `selectAll:` to the WKWebView, which
//! knows nothing of CodeMirror's private history or the terminal's selection, and the menu
//! eats the key before the page. They are plain `edit::*` items; the frontend routes them
//! (`lib/edit.ts`). Atlas commands are plain items with id `cmd:<command id>`; the frontend
//! pushes the effective keymap through [`menu_sync`] and this module sets each item's
//! accelerator, so on macOS the menu owns those keys (they work with focus in the GPU terminal, an
//! embedded browser page or a native field) and the webview never sees them twice. A click
//! or key emits `menu://command`.
//!
//! The text and structure of every menu is data (`menu_model`, built from a language); this module turns it into
//! native items once ([`realize`]) and, when the language changes, puts the new text on the live items ([`relabel`]).

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::menu_model::{Entry, PREFIX, Predefined};

/// The command items by command id (nested items are not reachable through `Menu::get`).
#[derive(Default)]
pub struct Items(Mutex<HashMap<String, MenuItem<Wry>>>);

/// The item of `entry` as a native one. A frontend command (`cmd:<id>`) starts disabled and is registered in
/// [`Items`]: the first [`menu_sync`] enables it (a key must not fire early); every other item (tray, capture) is
/// live at once.
fn realize(app: &AppHandle, entry: &Entry) -> tauri::Result<MenuItemKind<Wry>> {
    Ok(match entry {
        Entry::Command { id, label } => {
            let command = id.strip_prefix(PREFIX);
            let item = MenuItem::with_id(app, id, label, command.is_none(), None::<&str>)?;
            if let Some(command) = command {
                app.state::<Items>()
                    .0
                    .lock()
                    .insert(command.to_string(), item.clone());
            }
            MenuItemKind::MenuItem(item)
        }
        Entry::Predefined { item, label } => {
            let label = Some(label.as_str());
            MenuItemKind::Predefined(match item {
                Predefined::About => PredefinedMenuItem::about(app, label, None)?,
                Predefined::Services => PredefinedMenuItem::services(app, label)?,
                Predefined::Hide => PredefinedMenuItem::hide(app, label)?,
                Predefined::HideOthers => PredefinedMenuItem::hide_others(app, label)?,
                Predefined::Quit => PredefinedMenuItem::quit(app, label)?,
                Predefined::Cut => PredefinedMenuItem::cut(app, label)?,
                Predefined::Copy => PredefinedMenuItem::copy(app, label)?,
                Predefined::Paste => PredefinedMenuItem::paste(app, label)?,
                Predefined::Minimize => PredefinedMenuItem::minimize(app, label)?,
                Predefined::Maximize => PredefinedMenuItem::maximize(app, label)?,
                Predefined::Fullscreen => PredefinedMenuItem::fullscreen(app, label)?,
            })
        }
        Entry::Separator => MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?),
        Entry::Submenu { title, items } => MenuItemKind::Submenu(submenu(app, title, items)?),
    })
}

/// A native submenu with `items`.
pub fn submenu(app: &AppHandle, title: &str, items: &[Entry]) -> tauri::Result<Submenu<Wry>> {
    let menu = Submenu::new(app, title, true)?;
    for e in items {
        menu.append(&realize(app, e)?)?;
    }
    Ok(menu)
}

/// A native menu with `entries` (the tray's).
pub fn realize_menu(app: &AppHandle, entries: &[Entry]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for e in entries {
        menu.append(&realize(app, e)?)?;
    }
    Ok(menu)
}

/// Give the live items of a menu the text of `model` (built from the same structure, so item `n` is entry `n`): the
/// language changed, nothing else did, so accelerators, enabled state and ids stay as the frontend set them.
pub fn relabel(live: &[MenuItemKind<Wry>], model: &[Entry]) {
    debug_assert_eq!(
        live.len(),
        model.len(),
        "a menu and its model differ in shape"
    );
    for (item, entry) in live.iter().zip(model) {
        match (item, entry) {
            (MenuItemKind::MenuItem(i), Entry::Command { label, .. }) => {
                let _ = i.set_text(label);
            }
            (MenuItemKind::Predefined(i), Entry::Predefined { label, .. }) => {
                let _ = i.set_text(label);
            }
            (MenuItemKind::Submenu(s), Entry::Submenu { title, items }) => {
                let _ = s.set_text(title);
                if let Ok(children) = s.items() {
                    relabel(&children, items);
                }
            }
            _ => {}
        }
    }
}

/// The app menu bar (macOS only: Windows and Linux have none, the palette lists every command). Standard App / Window
/// items come from macOS; the text of every item is [`crate::menu_model::menu_bar`].
#[cfg(target_os = "macos")]
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    realize_menu(
        app,
        &crate::menu_model::menu_bar(atlas_i18n::lang(), &crate::tray::capture_keys(app)),
    )
}

/// Put the text of the display language and of the capture keys `keys` on every native menu, in place: the menu bar
/// (macOS) and the tray menu keep their items, ids, accelerators and enabled state; only the text changes. Called when
/// the language changes ([`language_changed`]) and when the capture keys do (`shots`).
pub fn relabel_all(app: &AppHandle, keys: &crate::shots::prefs::Keys) {
    let tag = atlas_i18n::lang();
    #[cfg(target_os = "macos")]
    if let Some(bar) = app.menu()
        && let Ok(live) = bar.items()
    {
        relabel(&live, &crate::menu_model::menu_bar(tag, keys));
    }
    crate::tray::relabel(app, tag, keys);
}

/// The display language changed (Settings → Language): put its text on every native menu without a restart. The menu
/// bar (macOS), the tray menu, the agent dock's title and the titles of the capture windows follow it; the dock's right-click menu is built on demand,
/// so it is in the new language the next time it opens.
pub fn language_changed(app: &AppHandle, tag: &str) {
    atlas_i18n::set_lang(tag);
    relabel_all(app, &crate::tray::capture_keys(app));
    crate::dock::retitle(app, atlas_i18n::lang());
    crate::shots::surface::retitle(app, atlas_i18n::lang());
}

/// The native text of the app as it is now, for the debug bridge (`atlas desktop debug eval` ... `menu_dump`): the
/// language, the menu bar as the OS holds it right now (read back from the live items), the tray menu and the dock's
/// right-click menu from their models.
#[tauri::command]
pub fn menu_dump(app: AppHandle) -> Value {
    let tag = atlas_i18n::lang();
    let keys = crate::tray::capture_keys(&app);
    let bar = app.menu().and_then(|m| m.items().ok());
    json!({
        "locale": tag,
        "menuBar": bar.as_deref().map(live_json),
        "menuBarModel": crate::menu_model::menu_bar(tag, &keys),
        "tray": crate::tray::dump(&app, tag, &keys),
        "dockPill": crate::dock::pill_menu(&crate::dock::DockPrefs::load(), tag),
        "dockTitle": atlas_i18n::t_in!(tag, "native_tray_dock_window_title"),
    })
}

/// The text the live items of a menu hold right now, as JSON (read back from the OS menu, not from the model).
pub fn live_json(items: &[MenuItemKind<Wry>]) -> Vec<Value> {
    items
        .iter()
        .map(|i| match i {
            MenuItemKind::MenuItem(m) => json!({"kind": "item", "id": m.id().as_ref(), "text": m.text().unwrap_or_default(), "enabled": m.is_enabled().unwrap_or(false)}),
            MenuItemKind::Predefined(p) => json!({"kind": "predefined", "text": p.text().unwrap_or_default()}),
            MenuItemKind::Submenu(s) => json!({"kind": "submenu", "text": s.text().unwrap_or_default(), "items": live_json(&s.items().unwrap_or_default())}),
            MenuItemKind::Check(c) => json!({"kind": "check", "text": c.text().unwrap_or_default()}),
            MenuItemKind::Icon(c) => json!({"kind": "icon", "text": c.text().unwrap_or_default()}),
        })
        .collect()
}

/// Menu click or key equivalent: hand the command id to the frontend. `dock:*` ids belong to the agent dock's
/// native right-click menu (`dock::dock_menu`), `capture:*` ids to the capture submenu (same path as the hotkey).
pub fn on_event(app: &AppHandle, id: &str) {
    if id.starts_with("dock:") {
        crate::dock::on_menu(app, id);
    } else if let Some(h) = crate::shots::prefs::hotkey_of_menu_id(id) {
        crate::shots::feed(app, crate::shots::session::Event::Hotkey(h));
    } else if let Some(cmd) = id.strip_prefix(PREFIX) {
        let _ = app.emit("menu://command", cmd);
    }
}

#[derive(Deserialize)]
pub struct Item {
    id: String,
    accelerator: Option<String>,
    enabled: bool,
}

/// Apply the frontend's effective keys and availability to the menu items.
#[tauri::command]
pub fn menu_sync(items: tauri::State<'_, Items>, list: Vec<Item>) {
    let map = items.0.lock();
    for it in list {
        let Some(item) = map.get(&it.id) else {
            continue;
        };
        // A remap to a key muda cannot parse leaves the item without a shortcut.
        if item.set_accelerator(it.accelerator.as_deref()).is_err() {
            let _ = item.set_accelerator(None::<&str>);
        }
        let _ = item.set_enabled(it.enabled);
    }
}
