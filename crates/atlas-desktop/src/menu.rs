//! Native menu bar. Standard App / Window items come from macOS (so ⌘Q ⌘H work everywhere, and
//! ⌘W never reaches the default "Close Window", which would hide the app). Edit keeps the
//! predefined Cut / Copy / Paste (they act on the focused field's selection; the GPU terminal
//! answers `copy` / `paste` events on its input sink). Undo, Redo and Select All are NOT
//! predefined: AppKit would send `undo:` / `redo:` / `selectAll:` to the WKWebView, which
//! knows nothing of CodeMirror's private history or the terminal's selection, and the menu
//! eats the key before the page. They are plain `edit::*` items; the frontend routes them
//! (`lib/edit.ts`). Atlas commands are plain items with id `cmd:<command id>`; the frontend
//! pushes the effective keymap through [`menu_sync`] and this module sets each item's
//! accelerator, so the menu owns those keys (they work with focus in the GPU terminal, an
//! embedded browser page or a native field) and the webview never sees them twice. A click
//! or key emits `menu://command`.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::Deserialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

const PREFIX: &str = "cmd:";

/// The command items by command id (nested items are not reachable through `Menu::get`).
#[derive(Default)]
pub struct Items(Mutex<HashMap<String, MenuItem<Wry>>>);

/// A disabled command item (enabled by the first [`menu_sync`]: a key must not fire early).
fn command(app: &AppHandle, id: &str, label: &str) -> tauri::Result<MenuItem<Wry>> {
    let item = MenuItem::with_id(app, format!("{PREFIX}{id}"), label, false, None::<&str>)?;
    app.state::<Items>()
        .0
        .lock()
        .insert(id.to_string(), item.clone());
    Ok(item)
}

/// `(command id, label)` per submenu; `None` is a separator.
type Entries = &'static [Option<(&'static str, &'static str)>];

const FILE: Entries = &[
    Some(("atlas::NewWorktree", "New Worktree…")),
    Some(("workspace::NewTerminal", "New Terminal")),
    Some(("atlas::NewAgentSession", "New Agent Session")),
    Some(("atlas::NewBrowserTab", "New Browser Tab")),
    None,
    Some(("workspace::Save", "Save")),
    Some(("workspace::SaveAll", "Save All")),
    None,
    Some(("pane::CloseActiveItem", "Close Tab")),
    Some(("pane::CloseOtherItems", "Close Other Tabs")),
    Some(("pane::CloseAllItems", "Close All Tabs")),
    Some(("pane::ReopenClosedItem", "Reopen Closed Tab")),
];

const GO: Entries = &[
    Some(("atlas::Jump", "Jump to Worktree or Terminal…")),
    Some(("file_finder::Toggle", "Go to File…")),
    Some(("search::Toggle", "Search in Project")),
    Some(("command_palette::Toggle", "Command Palette…")),
    None,
    Some(("pane::ActivateNextItem", "Next Tab")),
    Some(("pane::ActivatePreviousItem", "Previous Tab")),
    None,
    Some(("atlas::Logs", "Logs")),
    Some(("atlas::Config", "Config")),
    Some(("git::Diff", "Changes")),
    Some(("atlas::RestartService", "Restart Focused Service")),
];

const VIEW: Entries = &[
    Some(("shell::ToggleSidebar", "Toggle Sidebar")),
    Some(("workspace::ToggleLeftDock", "Toggle Left Dock")),
    Some(("workspace::ToggleRightDock", "Toggle Right Sidebar")),
    Some(("workspace::ToggleBottomDock", "Toggle Bottom Dock")),
    Some(("workspace::ToggleZoom", "Zoom Pane")),
    None,
    Some(("shell::ZoomIn", "Zoom In")),
    Some(("shell::ZoomOut", "Zoom Out")),
    Some(("shell::ZoomReset", "Actual Size")),
    None,
    Some(("pane::SplitRight", "Split Right")),
    Some(("pane::SplitDown", "Split Down")),
];

fn submenu(
    app: &AppHandle,
    title: &str,
    entries: Entries,
    tail: &[&dyn IsMenuItem<Wry>],
) -> tauri::Result<Submenu<Wry>> {
    let menu = Submenu::new(app, title, true)?;
    for e in entries {
        match e {
            Some((id, label)) => menu.append(&command(app, id, label)?)?,
            None => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    for t in tail {
        menu.append(*t)?;
    }
    Ok(menu)
}

/// The app menu bar. Close Window is deliberately absent: ⌘W is "Close Tab".
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let settings = command(app, "zed::OpenSettings", "Settings…")?;
    let help = command(app, "atlas::Help", "Keyboard Shortcuts")?;
    let sep = || PredefinedMenuItem::separator(app);
    let app_menu = Submenu::with_items(
        app,
        "Atlas",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &sep()?,
            &settings,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &sep()?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &command(app, "edit::Undo", "Undo")?,
            &command(app, "edit::Redo", "Redo")?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &command(app, "edit::SelectAll", "Select All")?,
        ],
    )?;
    let view = submenu(
        app,
        "View",
        VIEW,
        &[&sep()?, &PredefinedMenuItem::fullscreen(app, None)?],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    let help_menu = Submenu::with_items(app, "Help", true, &[&help])?;
    Menu::with_items(
        app,
        &[
            &app_menu,
            &submenu(app, "File", FILE, &[])?,
            &edit,
            &view,
            &submenu(app, "Go", GO, &[])?,
            &window,
            &help_menu,
        ],
    )
}

/// Menu click or key equivalent: hand the command id to the frontend.
pub fn on_event(app: &AppHandle, id: &str) {
    if let Some(cmd) = id.strip_prefix(PREFIX) {
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
