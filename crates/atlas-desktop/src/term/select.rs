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
    let (col, row) = (col as usize, row as i32);
    match (clicks, block) {
        (_, true) => sh
            .vt
            .start_selection(col, row, SelectionType::Block, side(frac)),
        // A URL or a `path:line:col` is one word; anything else keeps the semantic boundaries.
        (2, _) => sh.vt.select_word(col, row, side(frac)),
        (3.., _) => sh
            .vt
            .start_selection(col, row, SelectionType::Lines, side(frac)),
        _ => sh
            .vt
            .start_selection(col, row, SelectionType::Simple, side(frac)),
    }
    sh.redraw();
    // The view is click-through, but a drag can blur the input sink: hand focus back.
    if let Some(app) = APP.get() {
        let _ = app.emit("term://focus", &tab);
    }
    Ok(())
}

/// Shift-click: move the end of the selection to the cell, keeping its anchor (a new selection
/// starts at the cursor when there is none). A drag that follows goes through `term_select_update`.
#[tauri::command]
pub fn term_select_extend(tab: String, col: u16, row: u16, frac: f32) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.vt.extend_selection(col as usize, row as i32, side(frac));
    sh.redraw();
    Ok(())
}

/// A drag past the top (`lines` > 0, into history) or bottom (`lines` < 0) edge: scroll the
/// viewport and keep the selection's end on the edge row, at `col`.
#[tauri::command]
pub fn term_select_scroll(tab: String, lines: i32, col: u16, frac: f32) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.vt.select_scroll(lines, col as usize, side(frac));
    sh.redraw();
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

/// What ⌘-click would open under viewport cell (`col`, `row`): an OSC 8 link or a plain URL
/// (`kind: "url"`), or a path-shaped token (`kind: "path"`) the app resolves against the worktree
/// (`termPath.ts`). `None` when the cell is no link.
#[tauri::command]
pub fn term_link_at(tab: String, col: u16, row: u16) -> Result<Option<LinkAt>, String> {
    Ok(with_term(&tab)?
        .vt
        .link_at(col as usize, row as i32)
        .map(LinkAt::from))
}

/// `term_link_at`'s answer, as the webview reads it (`pointer.ts`).
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct LinkAt {
    pub kind: &'static str,
    pub target: String,
}

impl From<atlas_term::Link> for LinkAt {
    fn from(l: atlas_term::Link) -> Self {
        LinkAt {
            kind: match l.kind {
                atlas_term::LinkKind::Url => "url",
                atlas_term::LinkKind::Path => "path",
            },
            target: l.target,
        }
    }
}

/// Underline the link under the pointer (⌘ held); `col`/`row` omitted clears it. A path-shaped
/// token is underlined only with `paths` (the app checked it names a file of the worktree).
#[tauri::command]
pub fn term_hover(
    tab: String,
    col: Option<u16>,
    row: Option<u16>,
    paths: Option<bool>,
) -> Result<(), String> {
    let sh = with_term(&tab)?;
    let at = col.zip(row).map(|(c, r)| (c as usize, r as i32));
    if sh.vt.hover(at, paths.unwrap_or(false)) {
        sh.redraw();
    }
    Ok(())
}

/// The selection, scroll position and hover as the VT holds them: what the debug bridge and the
/// pointer tests read instead of guessing from pixels.
#[tauri::command]
pub fn term_pointer_state(tab: String) -> Result<serde_json::Value, String> {
    let vt = &with_term(&tab)?.vt;
    let (offset, max) = vt.scrollbar();
    Ok(serde_json::json!({
        "selection": vt.selection_state().map(|s| serde_json::json!({
            "kind": s.kind,
            "start": { "line": s.start_line, "col": s.start_col },
            "end": { "line": s.end_line, "col": s.end_col },
        })),
        "hover": vt.hover_target().map(|h| serde_json::json!({
            "target": h.target,
            "start": { "line": h.start.0, "col": h.start.1 },
            "end": { "line": h.end.0, "col": h.end.1 },
        })),
        "scroll": { "offset": offset, "max": max },
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    // `pointer.ts` reads `kind` and `target` of this exact JSON: a renamed field would make every
    // ⌘-click silently do nothing.
    #[test]
    fn link_at_serializes_what_the_webview_reads() {
        let url = LinkAt::from(atlas_term::Link {
            kind: atlas_term::LinkKind::Url,
            target: "https://a.dev/".into(),
        });
        assert_eq!(
            serde_json::to_value(&url).unwrap(),
            serde_json::json!({ "kind": "url", "target": "https://a.dev/" })
        );
        let path = LinkAt::from(atlas_term::Link {
            kind: atlas_term::LinkKind::Path,
            target: "src/a.ts:2:5".into(),
        });
        assert_eq!(
            serde_json::to_value(&path).unwrap(),
            serde_json::json!({ "kind": "path", "target": "src/a.ts:2:5" })
        );
    }
}
