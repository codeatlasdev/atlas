//! The quick-access stack: every capture (and every text read from an image) becomes a card in the corner of the
//! display under the pointer, newest nearest the corner, up to `prefs.quick.stack` cards (the oldest goes first).
//! ONE window (`shots-quick`) holds the whole stack and is resized to the number of cards; the page
//! (`routes/shots/quick`) renders the list Rust emits (`shots://quick-stack`) and calls the `shots_quick_*`
//! commands below. Each card closes itself after `prefs.after.dismissSecs` (the page counts, hover pauses that card),
//! and the window never takes focus from the app being worked in.
//!
//! The stack lives here (a module static), not in `Shots`: it is UI state with no meaning outside this window.

use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::naming::Kind;
use super::prefs::Corner;
use super::surface::{self, Surface};
use super::{QuickState, Shots};

/// Margin between the stack and the display edge, logical points.
#[cfg(target_os = "macos")]
const MARGIN: f64 = 16.0;
/// The most cards the setting may ask for (the settings page offers 1..=8).
const MAX_STACK: usize = 8;

/// A capture's card. `id` is the file path, so one file is one card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCard {
    pub id: String,
    pub entry: super::history::Entry,
    /// Data URL.
    pub thumb: String,
    pub dismiss_secs: u32,
    pub copied: bool,
}

/// A text read from an image ("capture text", or OCR from a card). `text` stays in Rust: the page only reads the
/// preview, and asks for "copy again" / "send to agent" by `id`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextCard {
    pub id: String,
    pub preview: String,
    pub lines: usize,
    pub chars: usize,
    /// Nothing was recognized: the card says "No text found" and has no copy / send.
    pub empty: bool,
    pub dismiss_secs: u32,
    #[serde(skip)]
    pub text: String,
}

/// One card of the stack; the page tells them apart by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum Card {
    #[serde(rename = "capture")]
    Capture(CaptureCard),
    #[serde(rename = "text")]
    Text(TextCard),
}

impl Card {
    pub fn id(&self) -> &str {
        match self {
            Card::Capture(c) => &c.id,
            Card::Text(t) => &t.id,
        }
    }
}

impl From<&QuickState> for CaptureCard {
    fn from(q: &QuickState) -> Self {
        Self {
            id: q.entry.path.to_string_lossy().into_owned(),
            entry: q.entry.clone(),
            thumb: q.thumb.clone(),
            dismiss_secs: q.dismiss_secs,
            copied: q.copied,
        }
    }
}

/// The cards, oldest first.
#[derive(Debug, Default)]
pub struct Stack {
    cards: Vec<Card>,
}

impl Stack {
    pub const fn new() -> Self {
        Self { cards: Vec::new() }
    }

    /// Add `card` as the newest. A card with the same id is replaced (a file captured again is one card, moved to
    /// the corner). Past `max` (at least 1) the oldest cards are dropped; they are returned, oldest first.
    pub fn push(&mut self, card: Card, max: usize) -> Vec<Card> {
        self.cards.retain(|c| c.id() != card.id());
        self.cards.push(card);
        let over = self.cards.len().saturating_sub(max.max(1));
        self.cards.drain(..over).collect()
    }

    pub fn remove(&mut self, id: &str) -> Option<Card> {
        let at = self.cards.iter().position(|c| c.id() == id)?;
        Some(self.cards.remove(at))
    }

    pub fn get(&self, id: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.id() == id)
    }

    pub fn list(&self) -> Vec<Card> {
        self.cards.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

static STACK: Mutex<Stack> = Mutex::new(Stack::new());
#[cfg(target_os = "macos")]
static TEXT_SEQ: AtomicU32 = AtomicU32::new(1);

/// What `shots://quick-stack` and `shots_quick_stack` carry: the corner (the page orders the cards by it) and the
/// cards, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackView {
    pub position: Corner,
    pub cards: Vec<Card>,
}

/// The cards the setting allows: `prefs.quick.stack` clamped to `1..=MAX_STACK`.
pub fn stack_limit(setting: u32) -> usize {
    (setting as usize).clamp(1, MAX_STACK)
}

fn view(app: &AppHandle) -> StackView {
    StackView {
        position: app.state::<Shots>().prefs.lock().quick.position,
        cards: STACK.lock().list(),
    }
}

/// The stack now (`shots_quick_stack`): what the page asks for when it mounts, because the first card is pushed
/// before the page has loaded and an event to nobody is lost.
#[tauri::command]
pub fn shots_quick_stack(app: AppHandle) -> StackView {
    view(&app)
}

/// Show a capture: it becomes the newest card (`runner::finish` calls this).
pub fn show(app: &AppHandle, q: &QuickState) -> Result<(), String> {
    push(app, q)
}

/// Add `q` to the stack and show it.
pub fn push(app: &AppHandle, q: &QuickState) -> Result<(), String> {
    add(app, Card::Capture(q.into()))
}

/// Show a text read from an image as a card of the stack. An empty read shows "No text found".
#[cfg(target_os = "macos")]
pub fn show_text(app: &AppHandle, r: &super::ocr::TextResult) -> Result<(), String> {
    let dismiss = app.state::<Shots>().prefs.lock().after.dismiss_secs;
    let id = format!("text:{}", TEXT_SEQ.fetch_add(1, Ordering::Relaxed));
    let card = text_card(id, &r.text, &r.preview, r.lines, r.chars, r.empty, dismiss);
    add(app, Card::Text(card))
}

/// The card for a read: a non-empty read keeps its text for "copy again" / "send"; an empty one keeps none (and
/// has nothing to copy, so nothing can reach the clipboard from it).
pub fn text_card(
    id: String,
    text: &str,
    preview: &str,
    lines: usize,
    chars: usize,
    empty: bool,
    dismiss_secs: u32,
) -> TextCard {
    TextCard {
        id,
        preview: preview.to_string(),
        lines,
        chars,
        empty,
        dismiss_secs,
        text: if empty {
            String::new()
        } else {
            text.to_string()
        },
    }
}

fn add(app: &AppHandle, card: Card) -> Result<(), String> {
    // The window first: when it cannot be built nothing was added to a stack nobody can see.
    let win = surface::build(app, Surface::Quick, 0, "")?;
    let max = stack_limit(app.state::<Shots>().prefs.lock().quick.stack);
    STACK.lock().push(card, max);
    present(app, &win)
}

/// Remove the card `id` (a close, an action that opened something else, a discard). The last card hides the window.
pub fn remove(app: &AppHandle, id: &str) {
    let empty = {
        let mut s = STACK.lock();
        s.remove(id);
        s.is_empty()
    };
    let Some(win) = app.get_webview_window(&Surface::Quick.label(0)) else {
        return;
    };
    if empty {
        let _ = win.hide();
        let _ = win.emit("shots://quick-stack", view(app));
    } else {
        let _ = present(app, &win);
    }
}

/// Size and place the window for the cards now (corner of the display under the pointer), tell the page, and order
/// it in when it is hidden.
fn present(app: &AppHandle, win: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // The display under the pointer from CoreGraphics (microseconds), not a ScreenCaptureKit window listing.
        let tp = std::time::Instant::now();
        let cur = super::runner::cursor();
        let frames: Vec<super::geom::Rect> = crate::display_mac::active_displays()
            .iter()
            .map(|d| super::geom::Rect::new(d.bounds.x, d.bounds.y, d.bounds.w, d.bounds.h))
            .collect();
        if let Some(d) = super::geom::display_at(&frames, cur.x, cur.y) {
            let v = view(app);
            let frame = surface::quick_stack_frame(
                d,
                v.cards.len(),
                v.position,
                Surface::Quick.size(),
                MARGIN,
            );
            surface::place(app, win, frame);
        }
        super::perf::mark("card.place", tp);
    }
    let _ = win.emit("shots://quick-stack", view(app));
    if !win.is_visible().unwrap_or(false) {
        surface::show(app, win);
    }
    Ok(())
}

/// What a capture card's buttons can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Copy,
    Save,
    Annotate,
    Pin,
    Ocr,
    Send,
    Reveal,
    Close,
    Discard,
}

/// What a text card's buttons can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAction {
    Copy,
    Send,
    Close,
}

/// A capture file Atlas itself made: actions refuse any other path, because the page is a webview and its argument is
/// untrusted input. Only a path in the history is acted on (and `discard` deletes it, so this is not optional).
pub fn known(state: &Shots, path: &Path) -> Result<super::history::Entry, String> {
    state
        .history
        .lock()
        .entries
        .iter()
        .find(|e| e.path == path)
        .cloned()
        .ok_or_else(|| "That file is not one of Atlas's captures.".to_string())
}

#[tauri::command]
pub async fn shots_quick_action(
    app: AppHandle,
    state: State<'_, Shots>,
    action: Action,
    path: String,
) -> Result<(), String> {
    // Closing a card only removes it from the screen: it must work for a capture the history already dropped, or
    // that card could never leave.
    if action == Action::Close {
        remove(&app, &path);
        return Ok(());
    }
    let path = PathBuf::from(path);
    let id = path.to_string_lossy().into_owned();
    let entry = known(&state, &path)?;
    match action {
        Action::Close => {}
        Action::Copy => {
            #[cfg(target_os = "macos")]
            match entry.kind {
                Kind::Screenshot => super::mac::copy_image(&path)?,
                _ => super::mac::copy_file(&path)?,
            }
            #[cfg(not(target_os = "macos"))]
            return super::unavailable();
        }
        Action::Reveal => {
            #[cfg(target_os = "macos")]
            super::mac::reveal(&path)?;
            #[cfg(not(target_os = "macos"))]
            return super::unavailable();
        }
        Action::Save => {
            use tauri_plugin_dialog::DialogExt;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog().file().set_file_name(name).save_file(move |p| {
                let _ = tx.send(p.map(|p| p.to_string()));
            });
            if let Some(dest) = rx.await.map_err(|e| e.to_string())? {
                std::fs::copy(&path, &dest).map_err(|e| format!("{dest}: {e}"))?;
            }
        }
        Action::Annotate => {
            if entry.kind != Kind::Screenshot {
                return Err("Only screenshots can be annotated.".into());
            }
            super::editor::open(&app, &path)?;
            remove(&app, &id);
        }
        Action::Pin => super::pin::open(&app, &state, &path)?,
        Action::Ocr => {
            if entry.kind != Kind::Screenshot {
                return Err("Only screenshots can be read for text.".into());
            }
            #[cfg(target_os = "macos")]
            {
                // Vision blocks for a moment on a big image: off the async executor.
                let a = app.clone();
                tokio::task::spawn_blocking(move || super::ocr::from_file(&a, &path))
                    .await
                    .map_err(|e| e.to_string())??;
            }
            #[cfg(not(target_os = "macos"))]
            return super::unavailable();
        }
        Action::Send => {
            send_to_agent(&app, &entry)?;
            remove(&app, &id);
        }
        Action::Discard => {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            state.history.lock().remove(&path);
            state.save_history();
            remove(&app, &id);
        }
    }
    Ok(())
}

/// A text card's buttons. `id` names a card of the stack (the page's argument is untrusted: an unknown id is an
/// error, and nothing but that card's own text is ever copied or sent).
#[tauri::command]
pub fn shots_quick_text(app: AppHandle, action: TextAction, id: String) -> Result<(), String> {
    if action == TextAction::Close {
        remove(&app, &id);
        return Ok(());
    }
    let card = match STACK.lock().get(&id) {
        Some(Card::Text(t)) => t.clone(),
        _ => return Err("That text card is gone.".into()),
    };
    if card.empty {
        return Err("There is no text to use.".into());
    }
    match action {
        TextAction::Close => {}
        TextAction::Copy => {
            #[cfg(target_os = "macos")]
            super::mac::copy_text(&card.text)?;
            #[cfg(not(target_os = "macos"))]
            return super::unavailable();
        }
        TextAction::Send => {
            send_text_to_agent(&app, &card)?;
            remove(&app, &id);
        }
    }
    Ok(())
}

/// Attach the capture to the dock's composer: the dock page shows a chip and includes the path in what it sends.
/// The dock is summoned (shown if hidden): attaching to a pill nobody can see would be silent.
pub fn send_to_agent(app: &AppHandle, entry: &super::history::Entry) -> Result<(), String> {
    let kind = serde_json::to_value(entry.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    attach(app, entry.path.to_string_lossy().into_owned(), kind)
}

/// The file a text card is sent as: agents read an attachment by path, so the text goes to a `.txt` in the temp
/// folder (one per card; the process id keeps two Atlas instances apart).
pub fn text_file(dir: &Path, pid: u32, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    dir.join(format!("atlas-shots-text-{pid}-{safe}.txt"))
}

/// Attach the text of `card` through the same dock path as a capture: written to a file, attached with kind `text`.
fn send_text_to_agent(app: &AppHandle, card: &TextCard) -> Result<(), String> {
    let path = text_file(&std::env::temp_dir(), std::process::id(), &card.id);
    std::fs::write(&path, &card.text).map_err(|e| format!("{}: {e}", path.display()))?;
    attach(app, path.to_string_lossy().into_owned(), "text".into())
}

fn attach(app: &AppHandle, path: String, kind: String) -> Result<(), String> {
    if app.get_webview_window(crate::dock::LABEL).is_none() {
        return Err("The agent dock is not available.".into());
    }
    crate::dock::attach(app, crate::dock::Attachment { path, kind });
    Ok(())
}

/// Start a native file drag of a capture from the card: the page calls this from its `dragstart`, so AppKit's current
/// event is the press that began the drag. Dropping into Finder, Mail, Slack or an upload field attaches the file.
#[tauri::command]
pub fn shots_quick_drag(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, Shots>,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(path);
    known(&state, &path)?;
    if window.label() != Surface::Quick.label(0) {
        return Err("only the capture card can start a drag".into());
    }
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let view = window.ns_view().map_err(|e| e.to_string())? as usize;
        app.run_on_main_thread(move || {
            // SAFETY: Tauri's live NSView of the card's webview, on the main thread.
            let r = unsafe { super::mac::drag_file(view as *mut std::ffi::c_void, &path) };
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| "the drag did not start".to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, window);
        super::unavailable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page is a webview: its `path` is untrusted. Only files in the capture history may be acted on, because
    /// `discard` deletes the file.
    #[test]
    fn only_files_in_the_history_are_known() {
        let s = Shots::load();
        let e = super::super::history::Entry {
            path: "/shots/a.png".into(),
            kind: Kind::Screenshot,
            at: 1,
            width: 1,
            height: 1,
            bytes: 1,
            secs: None,
        };
        s.history.lock().entries.clear();
        s.history.lock().push(e.clone(), 10);
        assert_eq!(known(&s, Path::new("/shots/a.png")).unwrap(), e);
        for evil in [
            "/etc/passwd",
            "/shots/a.png/../../etc/passwd",
            "/shots/b.png",
            "",
            "a.png",
        ] {
            assert!(known(&s, Path::new(evil)).is_err(), "{evil}");
        }
    }

    #[test]
    fn actions_deserialize_from_the_names_the_page_sends() {
        for (name, a) in [
            ("copy", Action::Copy),
            ("save", Action::Save),
            ("annotate", Action::Annotate),
            ("pin", Action::Pin),
            ("ocr", Action::Ocr),
            ("send", Action::Send),
            ("reveal", Action::Reveal),
            ("close", Action::Close),
            ("discard", Action::Discard),
        ] {
            let got: Action = serde_json::from_value(serde_json::json!(name)).unwrap();
            assert_eq!(got, a);
        }
        assert!(serde_json::from_value::<Action>(serde_json::json!("rm")).is_err());
        for (name, a) in [
            ("copy", TextAction::Copy),
            ("send", TextAction::Send),
            ("close", TextAction::Close),
        ] {
            let got: TextAction = serde_json::from_value(serde_json::json!(name)).unwrap();
            assert_eq!(got, a);
        }
        // a text card has no pin / ocr / discard
        for bad in ["pin", "ocr", "discard", "save"] {
            assert!(serde_json::from_value::<TextAction>(serde_json::json!(bad)).is_err());
        }
    }

    fn capture(path: &str) -> Card {
        Card::Capture(CaptureCard {
            id: path.into(),
            entry: super::super::history::Entry {
                path: path.into(),
                kind: Kind::Screenshot,
                at: 1,
                width: 10,
                height: 10,
                bytes: 1,
                secs: None,
            },
            thumb: "data:image/png;base64,xx".into(),
            dismiss_secs: 6,
            copied: false,
        })
    }

    fn ids(s: &Stack) -> Vec<String> {
        s.list().iter().map(|c| c.id().to_string()).collect()
    }

    /// Cards stack oldest first, so the newest is last (nearest the bottom corner; the page flips it for the top).
    #[test]
    fn captures_stack_oldest_first() {
        let mut s = Stack::new();
        for p in ["/a.png", "/b.png", "/c.png"] {
            assert!(s.push(capture(p), 5).is_empty());
        }
        assert_eq!(ids(&s), ["/a.png", "/b.png", "/c.png"]);
    }

    /// Past the limit the OLDEST card goes, never the newest (the capture just taken must be visible), and what
    /// goes is reported oldest first.
    #[test]
    fn the_oldest_cards_are_evicted_beyond_the_limit() {
        let mut s = Stack::new();
        s.push(capture("/1.png"), 2);
        s.push(capture("/2.png"), 2);
        let gone = s.push(capture("/3.png"), 2);
        assert_eq!(gone.iter().map(Card::id).collect::<Vec<_>>(), ["/1.png"]);
        assert_eq!(ids(&s), ["/2.png", "/3.png"]);
        // a smaller limit evicts several at once
        let gone = s.push(capture("/4.png"), 1);
        assert_eq!(
            gone.iter().map(Card::id).collect::<Vec<_>>(),
            ["/2.png", "/3.png"]
        );
        assert_eq!(ids(&s), ["/4.png"]);
    }

    /// A limit of 0 (a corrupt setting) still keeps the card just taken.
    #[test]
    fn a_zero_limit_keeps_the_newest_card() {
        let mut s = Stack::new();
        s.push(capture("/1.png"), 0);
        assert_eq!(ids(&s), ["/1.png"]);
    }

    /// The same file pushed again is one card, moved to the newest position, not a duplicate.
    #[test]
    fn the_same_file_is_one_card_moved_to_the_corner() {
        let mut s = Stack::new();
        s.push(capture("/a.png"), 5);
        s.push(capture("/b.png"), 5);
        s.push(capture("/a.png"), 5);
        assert_eq!(ids(&s), ["/b.png", "/a.png"]);
    }

    #[test]
    fn removing_a_card_keeps_the_order_of_the_rest_and_an_unknown_id_is_nothing() {
        let mut s = Stack::new();
        for p in ["/a.png", "/b.png", "/c.png"] {
            s.push(capture(p), 5);
        }
        assert!(s.remove("/b.png").is_some());
        assert!(s.remove("/b.png").is_none());
        assert!(s.remove("/nope").is_none());
        assert_eq!(ids(&s), ["/a.png", "/c.png"]);
        s.remove("/a.png");
        s.remove("/c.png");
        assert!(
            s.is_empty(),
            "closing the last card empties the stack (the window then hides)"
        );
    }

    /// Captures and texts share one stack, ordered by arrival.
    #[test]
    fn text_cards_stack_with_captures_and_count_against_the_limit() {
        let mut s = Stack::new();
        s.push(capture("/a.png"), 2);
        s.push(
            Card::Text(text_card("text:1".into(), "hi", "hi", 1, 2, false, 6)),
            2,
        );
        let gone = s.push(capture("/b.png"), 2);
        assert_eq!(gone.iter().map(Card::id).collect::<Vec<_>>(), ["/a.png"]);
        assert_eq!(ids(&s), ["text:1", "/b.png"]);
        assert!(matches!(s.get("text:1"), Some(Card::Text(_))));
    }

    #[test]
    fn the_setting_is_clamped_to_one_through_eight_cards() {
        assert_eq!(stack_limit(0), 1);
        assert_eq!(stack_limit(1), 1);
        assert_eq!(stack_limit(5), 5);
        assert_eq!(stack_limit(8), 8);
        assert_eq!(stack_limit(9999), 8);
    }

    /// Contract with the page (`lib/shots/quick.ts`): the REAL serialized stack. A capture card carries `kind`,
    /// `id` (its path), `entry`, `thumb`, `dismissSecs` and `copied`; a text card `kind`, `id`, `preview`, `lines`,
    /// `chars`, `empty`, `dismissSecs`, and NEVER the full text (the page does not need it, Rust copies it by id).
    #[test]
    fn the_stack_serializes_to_the_keys_the_page_reads() {
        let v = StackView {
            position: Corner::TopLeft,
            cards: vec![
                capture("/a.png"),
                Card::Text(text_card(
                    "text:3".into(),
                    "secret body",
                    "secret…",
                    4,
                    11,
                    false,
                    6,
                )),
            ],
        };
        let j = serde_json::to_value(&v).unwrap();
        assert_eq!(j["position"], "topLeft");
        let c = &j["cards"][0];
        assert_eq!(c["kind"], "capture");
        assert_eq!(c["id"], "/a.png");
        assert_eq!(c["entry"]["path"], "/a.png");
        assert_eq!(c["entry"]["kind"], "screenshot");
        assert_eq!(c["thumb"], "data:image/png;base64,xx");
        assert_eq!(c["dismissSecs"], 6);
        assert_eq!(c["copied"], false);
        let t = &j["cards"][1];
        assert_eq!(t["kind"], "text");
        assert_eq!(t["id"], "text:3");
        assert_eq!(t["preview"], "secret…");
        assert_eq!(
            (t["lines"].as_u64(), t["chars"].as_u64()),
            (Some(4), Some(11))
        );
        assert_eq!(t["empty"], false);
        assert_eq!(t["dismissSecs"], 6);
        assert!(t.get("text").is_none(), "the full text stays in Rust: {t}");
        assert!(!j.to_string().contains("secret body"));
    }

    /// An empty read has nothing to copy or send: its card keeps no text at all, so no command can put anything
    /// from it on the clipboard, and the page shows "No text found".
    #[test]
    fn an_empty_read_keeps_no_text() {
        let t = text_card("text:1".into(), "leftover", "", 0, 0, true, 6);
        assert!(t.empty);
        assert!(t.text.is_empty());
        let j = serde_json::to_value(Card::Text(t)).unwrap();
        assert_eq!(j["empty"], true);
    }

    /// A text card goes to the agent as a file named from its id: unique per card and per process, and safe as a
    /// file name whatever the id holds.
    #[test]
    fn a_text_card_is_sent_as_a_file_with_a_safe_unique_name() {
        let dir = Path::new("/tmp");
        let a = text_file(dir, 42, "text:1");
        assert_eq!(a, Path::new("/tmp/atlas-shots-text-42-text-1.txt"));
        assert_ne!(a, text_file(dir, 42, "text:2"));
        assert_ne!(a, text_file(dir, 43, "text:1"));
        let evil = text_file(dir, 1, "../../etc/passwd");
        assert_eq!(evil.parent(), Some(dir), "{evil:?}");
    }
}
