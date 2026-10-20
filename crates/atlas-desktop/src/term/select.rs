//! Selection, copy, links (⌘-click, hover) and search.

use atlas_term::{SelectionType, Side};
use tauri::Emitter;

use super::{APP, with_term, write_clipboard};

/// Selected text → system clipboard (⌘C). Returns the text.
#[tauri::command]
pub fn term_copy(tab: String) -> Result<Option<String>, String> {
    let text = with_term(&tab)?.vt.end_selection();
    if let Some(t) = &text {
        write_clipboard(t);
    }
    Ok(text)
}

/// Which half of the cell a fractional column falls in (selection edge semantics).
fn side(frac: f32) -> Side {
    if frac < 0.5 { Side::Left } else { Side::Right }
}

/// Begin a selection at viewport cell (`col`, `row`); `frac` = x offset within the cell, 0..1.
/// `clicks` 1/2/3 = char/word/line, `block` = rectangular (⌥-drag).
#[tauri::command]
pub fn term_select_start(
    tab: String,
    col: u16,
    row: u16,
    frac: f32,
    clicks: u8,
    block: bool,
) -> Result<(), String> {
    let sh = with_term(&tab)?;
    let ty = match (clicks, block) {
        (_, true) => SelectionType::Block,
        (2, _) => SelectionType::Semantic,
        (3.., _) => SelectionType::Lines,
        _ => SelectionType::Simple,
    };
    sh.vt
        .start_selection(col as usize, row as i32, ty, side(frac));
    sh.redraw();
    // The view is click-through, but a drag can blur the input sink: hand focus back.
    if let Some(app) = APP.get() {
        let _ = app.emit("term://focus", &tab);
    }
    Ok(())
}

#[tauri::command]
pub fn term_select_update(tab: String, col: u16, row: u16, frac: f32) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.vt.update_selection(col as usize, row as i32, side(frac));
    sh.redraw();
    Ok(())
}

/// Finish a drag: the selection stays highlighted. With `copy` (Ghostty `copy-on-select`,
/// on by default) its text also goes to the clipboard.
#[tauri::command]
pub fn term_select_end(tab: String, copy: Option<bool>) -> Result<Option<String>, String> {
    let text = with_term(&tab)?.vt.end_selection();
    if let (Some(t), true) = (&text, copy.unwrap_or(true)) {
        write_clipboard(t);
    }
    Ok(text)
}

#[tauri::command]
pub fn term_clear_selection(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.vt.clear_selection();
    sh.redraw();
    Ok(())
}

/// URL under viewport cell (`col`, `row`) for ⌘-click: an OSC 8 link, else a plain
/// `http(s)://` / `file://` URL in the text.
#[tauri::command]
pub fn term_hyperlink_at(tab: String, col: u16, row: u16) -> Result<Option<String>, String> {
    Ok(with_term(&tab)?.vt.url_at(col as usize, row as i32))
}

/// The text token under viewport cell (`col`, `row`) for ⌘-click when no URL is there: the app
/// decides whether it is a file of the worktree (`termPath.ts`).
#[tauri::command]
pub fn term_path_at(tab: String, col: u16, row: u16) -> Result<Option<String>, String> {
    Ok(with_term(&tab)?.vt.path_at(col as usize, row as i32))
}

/// Underline the URL under the pointer (⌘ held); `col`/`row` omitted clears it.
#[tauri::command]
pub fn term_hover(tab: String, col: Option<u16>, row: Option<u16>) -> Result<(), String> {
    let sh = with_term(&tab)?;
    let at = col.zip(row).map(|(c, r)| (c as usize, r as i32));
    if sh.vt.set_hover(at) {
        sh.redraw();
    }
    Ok(())
}

/// Result of a search step: 1-based `index` of the current match (0 = none) of `total`.
#[derive(serde::Serialize)]
pub struct SearchResult {
    index: usize,
    total: usize,
}

/// Search scrollback + screen: highlights every match, scrolls to the next (`forward`) /
/// previous one (wrapping). A changed query/flags restarts at the top of the viewport.
#[tauri::command]
pub fn term_search(
    tab: String,
    query: String,
    regex: bool,
    case: bool,
    forward: bool,
) -> Result<SearchResult, String> {
    let sh = with_term(&tab)?;
    let r = sh.vt.search(&query, regex, case, forward);
    sh.redraw();
    r.map(|s| SearchResult {
        index: s.index,
        total: s.total,
    })
}

#[tauri::command]
pub fn term_search_clear(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.vt.clear_search();
    sh.redraw();
    Ok(())
}
