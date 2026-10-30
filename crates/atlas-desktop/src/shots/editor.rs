//! The annotation editor's backend: it opens a screenshot from the capture history in its own window, hands the page
//! the pixels, and saves what the page draws. Annotating never touches the original: the result is a sibling
//! `… (edited).png` (`naming::edited`), listed in the history like any capture.
//!
//! The page is a webview and its arguments are untrusted: `path` must be a capture of the history, and the PNG is
//! checked (base64, a real PNG signature, a sane size) before a byte reaches the disk or the clipboard.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::history::Entry;
use super::naming::{self, Kind};
use super::surface::{self, Surface};
use super::{Shots, pixels};

/// The largest edited image accepted (decoded bytes): a 16k × 16k screenshot is far under it, a runaway page is not.
pub const MAX_PNG_BYTES: usize = 128 * 1024 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// What `shots_editor_save` does with the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveMode {
    /// A new `(edited)` file.
    File,
    /// The clipboard only; no file.
    Copy,
    /// Both.
    Both,
}

impl SaveMode {
    pub fn writes_file(self) -> bool {
        matches!(self, SaveMode::File | SaveMode::Both)
    }

    pub fn copies(self) -> bool {
        matches!(self, SaveMode::Copy | SaveMode::Both)
    }
}

/// The PNG the page sent (base64, no `data:` prefix), checked: valid base64, within [`MAX_PNG_BYTES`], with the PNG
/// signature and a readable header. Returns the bytes and the pixel size.
pub fn decode_payload(b64: &str) -> Result<(Vec<u8>, (u32, u32)), String> {
    // base64 is 4/3 of the data: refuse an absurd string before allocating for it
    if b64.len() > MAX_PNG_BYTES / 3 * 4 + 8 {
        return Err("The edited image is too large.".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| "The edited image is not valid base64.".to_string())?;
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..8] != PNG_SIGNATURE {
        return Err("The edited image is not a PNG.".into());
    }
    let img =
        pixels::decode_png(&bytes).map_err(|e| format!("The edited image cannot be read: {e}"))?;
    if img.size.0 == 0 || img.size.1 == 0 {
        return Err("The edited image is empty.".into());
    }
    Ok((bytes, img.size))
}

/// The file an edit of `original` is saved to.
pub fn target_for(original: &Path) -> PathBuf {
    naming::edited(original)
}

/// `shots_editor_load()` for the page: the path, the pixel size and the picture as a data URL (the page's canvas
/// needs a same-origin source to read pixels back for blur and pixelate).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorDoc {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub src: String,
}

/// A data URL of the PNG file at `path`, with its size.
pub fn load(path: &Path) -> Result<EditorDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let img = pixels::decode_png(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(EditorDoc {
        path: path.to_path_buf(),
        width: img.size.0,
        height: img.size.1,
        src: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
    })
}

/// Open the editor on `path` (a capture of the history): builds the window with the path in its query and shows it.
pub fn open(app: &AppHandle, path: &Path) -> Result<(), String> {
    // A previous editor window is replaced: one document at a time.
    if let Some(w) = app.get_webview_window(&Surface::Editor.label(0)) {
        let _ = w.destroy();
    }
    let query = format!("path={}", urlencode(&path.to_string_lossy()));
    let win = surface::build(app, Surface::Editor, 0, &query)?;
    surface::show(app, &win);
    Ok(())
}

/// Percent-encode everything but unreserved characters (the path goes in a query string).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[tauri::command]
pub fn shots_editor_load(state: State<'_, Shots>, path: String) -> Result<EditorDoc, String> {
    let path = PathBuf::from(path);
    let e = super::quick::known(&state, &path)?;
    if e.kind != Kind::Screenshot {
        return Err("Only screenshots can be annotated.".into());
    }
    load(&path)
}

#[tauri::command]
pub fn shots_editor_save(
    app: AppHandle,
    state: State<'_, Shots>,
    path: String,
    png: String,
    mode: SaveMode,
) -> Result<Entry, String> {
    let original = PathBuf::from(path);
    let orig = super::quick::known(&state, &original)?;
    if orig.kind != Kind::Screenshot {
        return Err("Only screenshots can be annotated.".into());
    }
    let (bytes, size) = decode_payload(&png)?;
    let prefs = state.prefs.lock().clone();
    // `Copy` writes a scratch file only to reuse the pasteboard code (image + file URL); it is the temp dir's, never
    // the capture folder's, and is not listed in the history.
    let dest = if mode.writes_file() {
        target_for(&original)
    } else {
        std::env::temp_dir().join(format!("atlas-shots-edit-{}.png", std::process::id()))
    };
    std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    if mode.copies() {
        #[cfg(target_os = "macos")]
        super::mac::copy_image(&dest)?;
        #[cfg(not(target_os = "macos"))]
        return super::unavailable();
    }
    let entry = Entry {
        path: dest.clone(),
        kind: Kind::Screenshot,
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        width: size.0,
        height: size.1,
        bytes: bytes.len() as u64,
        secs: None,
    };
    if mode.writes_file() {
        state
            .history
            .lock()
            .push(entry.clone(), prefs.history as usize);
        state.save_history();
    }
    let _ = &app;
    Ok(entry)
}

#[tauri::command]
pub fn shots_editor_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(&Surface::Editor.label(0)) {
        let _ = w.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(&vec![200u8; (w * h * 4) as usize])
            .unwrap();
        drop(wr);
        out
    }

    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }

    #[test]
    fn a_real_png_decodes_with_its_size() {
        let (bytes, size) = decode_payload(&b64(&png(5, 3))).unwrap();
        assert_eq!(size, (5, 3));
        assert_eq!(bytes[..8], PNG_SIGNATURE);
    }

    /// The page is a webview: whatever it sends is untrusted. Nothing that is not a PNG reaches the disk (the file
    /// would be named `.png` and opened by other apps) or the clipboard.
    #[test]
    fn anything_that_is_not_a_png_is_refused() {
        assert!(
            decode_payload("not base64 !!!")
                .unwrap_err()
                .contains("base64")
        );
        assert!(
            decode_payload(&b64(b"GIF89a...."))
                .unwrap_err()
                .contains("not a PNG")
        );
        assert!(decode_payload("").unwrap_err().contains("not a PNG"));
        // a PNG signature with a truncated body
        let mut cut = png(8, 8);
        cut.truncate(20);
        assert!(
            decode_payload(&b64(&cut))
                .unwrap_err()
                .contains("cannot be read")
        );
        // a data: URL prefix is the page's job to strip: it is not base64
        assert!(decode_payload(&format!("data:image/png;base64,{}", b64(&png(2, 2)))).is_err());
    }

    #[test]
    fn an_absurd_payload_is_refused_before_it_is_decoded() {
        let huge = "A".repeat(MAX_PNG_BYTES / 3 * 4 + 100);
        assert!(decode_payload(&huge).unwrap_err().contains("too large"));
    }

    /// Annotating must never overwrite the capture the person may already have pasted or shared.
    #[test]
    fn the_edit_goes_to_a_sibling_file_never_over_the_original() {
        let p = Path::new("/shots/Screenshot 2026-10-26 at 14.03.22.png");
        let t = target_for(p);
        assert_ne!(t, p);
        assert_eq!(t.parent(), p.parent());
        assert_eq!(
            target_for(&t),
            t,
            "editing an edit overwrites the edit, not the original"
        );
    }

    #[test]
    fn save_modes_deserialize_from_the_names_the_page_sends() {
        for (name, m) in [
            ("file", SaveMode::File),
            ("copy", SaveMode::Copy),
            ("both", SaveMode::Both),
        ] {
            assert_eq!(
                serde_json::from_value::<SaveMode>(serde_json::json!(name)).unwrap(),
                m
            );
        }
        assert!(serde_json::from_value::<SaveMode>(serde_json::json!("overwrite")).is_err());
        assert!(SaveMode::Both.writes_file() && SaveMode::Both.copies());
        assert!(!SaveMode::Copy.writes_file() && SaveMode::Copy.copies());
        assert!(SaveMode::File.writes_file() && !SaveMode::File.copies());
    }

    /// A PNG with an alpha channel (rounded corners are transparent when no background is chosen) is a valid payload.
    #[test]
    fn a_png_with_transparent_corners_is_accepted() {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        let mut px = vec![255u8; 4 * 4 * 4];
        px[3] = 0; // the top-left pixel is fully transparent
        wr.write_image_data(&px).unwrap();
        drop(wr);
        let (_, size) = decode_payload(&b64(&out)).unwrap();
        assert_eq!(size, (4, 4));
    }

    #[test]
    fn the_query_path_is_percent_encoded() {
        assert_eq!(
            urlencode("/a b/c&d=e#f.png"),
            "%2Fa%20b%2Fc%26d%3De%23f.png"
        );
        assert_eq!(urlencode("Screenshot-1_2.png"), "Screenshot-1_2.png");
    }

    /// The page reads `path, width, height, src` (`EditorDoc` in `lib/shots/types.ts`).
    #[test]
    fn the_document_serializes_with_the_keys_the_page_reads() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.png");
        std::fs::write(&f, png(4, 2)).unwrap();
        let v = serde_json::to_value(load(&f).unwrap()).unwrap();
        assert_eq!(
            (v["width"].as_u64(), v["height"].as_u64()),
            (Some(4), Some(2))
        );
        assert!(
            v["src"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(v["path"], f.to_string_lossy().as_ref());
    }
}
