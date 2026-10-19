//! GPU terminal: one native view per tab laid over the pane, drawn by `atlas-term`.
//!
//! Per terminal, as in Ghostty: a reader task (hostd `pty_attach_raw` → `Vt`), a render
//! thread (snapshot → wgpu, woken only by new bytes or a resize), and writes (keys,
//! paste, VT replies) through the hostd connection. Clicks pass through the view to the
//! webview (`hitTest:` → nil), so focus and keyboard stay in the frontend for now.
//! Local hostd only: the raw stream is a dedicated socket connection. macOS only (AppKit view).
//!
//! This file is the registry of open terminals plus the pty/clipboard/main-thread helpers.
//! The `#[tauri::command]`s live by concern: `lifecycle` (open/bounds/close), `keys`
//! (input, scrolling, prompts), `mouse` (pointer reports to programs), `select` (selection,
//! links, search), `settings` (theme, prefs, font, focus, stats); `render_loop`, `mac` and
//! `shared` are the machinery.

mod keys;
mod lifecycle;
mod mac;
mod mouse;
mod render_loop;
mod select;
mod settings;
mod shared;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock};
use std::thread::JoinHandle;

use base64::Engine;
use parking_lot::Mutex;
use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::host::HostHub;
use shared::Shared;

pub use keys::*;
pub use lifecycle::*;
pub use mouse::*;
pub use select::*;
pub use settings::*;

/// `terminal.*` settings for a GPU terminal (defaults when absent).
#[derive(serde::Deserialize)]
#[serde(default)]
pub struct TermPrefs {
    pub(super) font_size: f32,
    pub(super) scrollback: usize,
    /// `block` | `bar` | `underline` | `hollow`.
    pub(super) cursor_shape: String,
    pub(super) cursor_blink: bool,
    /// OSC 52 (program writes the clipboard): `allow` | `ask` | `deny`.
    pub(super) osc52: String,
    /// Programming ligatures (calt/liga).
    pub(super) ligatures: bool,
    /// Font family candidates, first installed one wins; empty / none installed =
    /// the bundled JetBrains Mono.
    pub(super) font_family: Vec<String>,
    /// Embolden glyphs slightly (Ghostty `font-thicken`).
    pub(super) font_thicken: bool,
}

impl TermPrefs {
    pub(super) fn font_spec(&self) -> atlas_term::FontSpec {
        atlas_term::FontSpec {
            families: self
                .font_family
                .iter()
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect(),
            thicken: self.font_thicken,
        }
    }
}

impl Default for TermPrefs {
    fn default() -> Self {
        Self {
            font_size: 12.5,
            scrollback: 10_000,
            cursor_shape: "block".into(),
            cursor_blink: false,
            // Ghostty's default: a program may not overwrite the clipboard unasked.
            osc52: "ask".into(),
            ligatures: true,
            font_family: Vec::new(),
            font_thicken: false,
        }
    }
}

struct Term {
    pty: String,
    /// Retained `NSView*`; touched only on the main thread.
    view: usize,
    shared: Arc<Shared>,
    render: Option<JoinHandle<()>>,
    reader: tauri::async_runtime::JoinHandle<()>,
    /// Drains `Shared::writer` into hostd.
    writer: tauri::async_runtime::JoinHandle<()>,
}

static TERMS: LazyLock<Mutex<HashMap<String, Term>>> = LazyLock::new(Default::default);
/// Set at first `term_open`; lets non-command paths (VT callbacks) reach the app.
static APP: OnceLock<AppHandle> = OnceLock::new();

fn with_term(tab: &str) -> Result<Arc<Shared>, String> {
    let t = TERMS.lock();
    let t = t
        .get(tab)
        .ok_or_else(|| format!("no terminal for tab {tab}"))?;
    Ok(t.shared.clone())
}

/// The terminal of `tab`, or every open one when omitted.
fn targets(tab: Option<&str>) -> Result<Vec<Arc<Shared>>, String> {
    let terms = TERMS.lock();
    match tab {
        Some(t) => terms
            .get(t)
            .map(|t| vec![t.shared.clone()])
            .ok_or_else(|| format!("no terminal for tab {t}")),
        None => Ok(terms.values().map(|t| t.shared.clone()).collect()),
    }
}

/// The general pasteboard as a paste decision (⌘V). Read natively because the webview's paste
/// event only exposes text: a screenshot or a Finder file copy arrives as nothing.
fn read_clipboard() -> atlas_term::input::Clipboard {
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
        NSPasteboardTypeTIFF,
    };
    use objc2_foundation::NSURL;
    // SAFETY: the general pasteboard is safe to read off the main thread; the type constants
    // are static NSStrings.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let mut files = Vec::new();
        for item in pb.pasteboardItems().iter().flatten() {
            let Some(s) = item.stringForType(NSPasteboardTypeFileURL) else {
                continue;
            };
            if let Some(p) = NSURL::URLWithString(&s).and_then(|u| u.path()) {
                files.push(p.to_string());
            }
        }
        let text = pb
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string());
        let has_image = pb.types().is_some_and(|t| {
            t.iter().any(|t| {
                t.isEqualToString(NSPasteboardTypePNG) || t.isEqualToString(NSPasteboardTypeTIFF)
            })
        });
        atlas_term::input::Clipboard::classify(files, text, has_image)
    }
}

/// Text → the system pasteboard (⌘C, OSC 52).
fn write_clipboard(text: &str) {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    // SAFETY: NSPasteboard general pasteboard is safe off the main thread for plain writes;
    // `NSPasteboardTypeString` is a static NSString constant.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
}

/// Send queued pty input one request after the other, so it reaches the program in the
/// order it was typed (one task per write could swap a paste and its ⏎).
async fn drain<F: Future<Output = ()>>(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    mut send: impl FnMut(Vec<u8>) -> F,
) {
    while let Some(b) = rx.recv().await {
        send(b).await;
    }
}

/// The pty input queue of a terminal: a sender for `Shared::writer` and its drain task.
fn pty_writer(
    app: &AppHandle,
    pty: &str,
) -> (
    tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    tauri::async_runtime::JoinHandle<()>,
) {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let pty = pty.to_string();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tauri::async_runtime::spawn(drain(rx, move |b: Vec<u8>| {
        let (hub, pty) = (hub.clone(), pty.clone());
        async move {
            let data = base64::engine::general_purpose::STANDARD.encode(&b);
            if let Err(e) = hub
                .call("pty_write", json!({"pty": pty, "data": data}))
                .await
            {
                eprintln!("pty_write {pty}: {e}");
            }
        }
    }));
    (tx, task)
}

/// hostd came back (restart, live upgrade): reattach every GPU terminal's raw stream and
/// replay its output from scratch. A pty that died with the old daemon keeps its last screen.
pub async fn reattach_all() {
    let terms: Vec<(String, String, Arc<Shared>)> = TERMS
        .lock()
        .iter()
        .map(|(tab, t)| (tab.clone(), t.pty.clone(), t.shared.clone()))
        .collect();
    for (tab, pty, sh) in terms {
        match atlas_host::client::attach_raw(&atlas_host::paths::base(), &pty, true).await {
            Ok(raw) => {
                sh.vt.reset();
                sh.redraw();
                let reader = lifecycle::spawn_reader(sh, raw);
                match TERMS.lock().get_mut(&tab) {
                    Some(t) => std::mem::replace(&mut t.reader, reader).abort(),
                    None => reader.abort(),
                }
            }
            Err(e) => eprintln!("reattach terminal {tab} (pty {pty}): {e:#}"),
        }
    }
}

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    /// Pty input goes out in the order it was queued, however fast it arrives.
    #[tokio::test]
    async fn drain_sends_in_queue_order() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for i in 0..1000u32 {
            tx.send(i.to_le_bytes().to_vec()).unwrap();
        }
        drop(tx);
        let got = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let g = got.clone();
        super::drain(rx, move |b| {
            let g = g.clone();
            async move {
                // Later items finish sooner if they ran concurrently.
                let i = u32::from_le_bytes(b.try_into().unwrap());
                tokio::task::yield_now().await;
                g.lock().push(i);
            }
        })
        .await;
        assert_eq!(*got.lock(), (0..1000).collect::<Vec<_>>());
    }
}
