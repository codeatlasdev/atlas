//! "Capture text": read the words in a screenshot with Apple Vision (`VNRecognizeTextRequest`, on-device, nothing
//! leaves the Mac), put them on the clipboard and tell the person with a card that shows the first lines.
//!
//! The pure parts (reading order, preview, what goes on the clipboard) are tested everywhere; the Vision call is
//! macOS only and is exercised by the agent-instance check in `docs/desktop.md`.

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// One recognized line: its text and where it sits in the image, Vision's normalized box (origin bottom-left, 0–1).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub h: f64,
}

/// What the page shows after a read (`shots://text`, camelCase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextResult {
    /// The whole text, as copied.
    pub text: String,
    /// The first lines, shortened, for the toast.
    pub preview: String,
    pub lines: usize,
    pub chars: usize,
    /// Nothing was recognized (the toast says so; the clipboard is left alone).
    pub empty: bool,
}

const PREVIEW_LINES: usize = 3;
const PREVIEW_CHARS: usize = 160;

/// The lines in reading order: top to bottom, and left to right when two sit on the same row. Vision returns its
/// boxes with the origin at the bottom, so a larger `y` is higher up. Lines whose vertical centres are within half
/// the taller one's height are one row.
pub fn order_lines(mut lines: Vec<Line>) -> Vec<Line> {
    lines.retain(|l| !l.text.trim().is_empty());
    // top first
    lines.sort_by(|a, b| (b.y + b.h / 2.0).total_cmp(&(a.y + a.h / 2.0)));
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for l in lines {
        let centre = l.y + l.h / 2.0;
        match rows.last_mut() {
            Some(row)
                if row.iter().any(|r| {
                    let rc = r.y + r.h / 2.0;
                    (rc - centre).abs() <= r.h.max(l.h) / 2.0
                }) =>
            {
                row.push(l)
            }
            _ => rows.push(vec![l]),
        }
    }
    rows.into_iter()
        .flat_map(|mut r| {
            r.sort_by(|a, b| a.x.total_cmp(&b.x));
            r
        })
        .collect()
}

/// Lines of one row are joined with a space, rows with a newline.
pub fn join(lines: &[Line]) -> String {
    let mut out = String::new();
    let mut prev: Option<&Line> = None;
    for l in lines {
        if let Some(p) = prev {
            let same_row = ((p.y + p.h / 2.0) - (l.y + l.h / 2.0)).abs() <= p.h.max(l.h) / 2.0;
            out.push(if same_row { ' ' } else { '\n' });
        }
        out.push_str(l.text.trim());
        prev = Some(l);
    }
    out
}

/// The toast's text: at most [`PREVIEW_LINES`] lines and [`PREVIEW_CHARS`] characters, with an ellipsis when cut.
pub fn preview(text: &str) -> String {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.by_ref().take(PREVIEW_LINES).collect();
    let mut out = head.join("\n");
    let mut cut = lines.next().is_some();
    if out.chars().count() > PREVIEW_CHARS {
        out = out.chars().take(PREVIEW_CHARS).collect();
        cut = true;
    }
    if cut {
        out.push('…');
    }
    out
}

/// The result of reading `lines`.
pub fn result(lines: Vec<Line>) -> TextResult {
    let lines = order_lines(lines);
    let text = join(&lines);
    TextResult {
        preview: preview(&text),
        lines: text.lines().count(),
        chars: text.chars().count(),
        empty: text.trim().is_empty(),
        text,
    }
}

/// Read the text of the PNG at `path`, copy it, and tell the page. An image without text says so instead of
/// emptying the clipboard.
pub fn from_file(app: &AppHandle, path: &Path) -> Result<(), String> {
    let lines = recognize(path)?;
    let r = result(lines);
    if !r.empty {
        super::mac::copy_text(&r.text)?;
    }
    let _ = app.emit("shots://text", &r);
    super::quick::show_text(app, &r)?;
    Ok(())
}

/// Run Vision on the image at `path` (accurate level, language correction on, the system's languages).
fn recognize(path: &Path) -> Result<Vec<Line>, String> {
    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };

    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let data = NSData::with_bytes(&bytes);
    // SAFETY: documented Vision calls on objects created here; the request and handler live until the end of the
    // function, and `performRequests` is synchronous, so `results` is filled when it returns.
    unsafe {
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );
        let as_request: Retained<VNRequest> =
            Retained::into_super(Retained::into_super(request.clone()));
        handler
            .performRequests_error(&NSArray::from_retained_slice(&[as_request]))
            .map_err(|e| {
                format!(
                    "Vision could not read the image: {}",
                    e.localizedDescription()
                )
            })?;
        let mut out = Vec::new();
        for obs in request.results().into_iter().flat_map(|r| r.to_vec()) {
            let Some(top) = obs.topCandidates(1).firstObject() else {
                continue;
            };
            let b = obs.boundingBox();
            out.push(Line {
                text: top.string().to_string(),
                x: b.origin.x,
                y: b.origin.y,
                h: b.size.height,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(text: &str, x: f64, y: f64, h: f64) -> Line {
        Line {
            text: text.into(),
            x,
            y,
            h,
        }
    }

    /// Vision's origin is bottom-left: the line with the larger `y` is the top one. Returned in any order, the
    /// text must read top to bottom.
    #[test]
    fn lines_read_top_to_bottom_whatever_order_vision_returns_them() {
        let got = order_lines(vec![
            l("third", 0.1, 0.1, 0.05),
            l("first", 0.1, 0.8, 0.05),
            l("second", 0.1, 0.4, 0.05),
        ]);
        let t: Vec<_> = got.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(t, ["first", "second", "third"]);
    }

    /// Two boxes on one row (a table, a toolbar) read left to right and join with a space, not a newline.
    #[test]
    fn boxes_on_one_row_read_left_to_right_and_share_a_line() {
        let got = order_lines(vec![
            l("B", 0.5, 0.801, 0.05),
            l("A", 0.1, 0.8, 0.05),
            l("below", 0.1, 0.5, 0.05),
        ]);
        assert_eq!(join(&got), "A B\nbelow");
    }

    #[test]
    fn blank_recognitions_are_dropped() {
        assert!(order_lines(vec![l("  ", 0.0, 0.0, 0.1), l("", 0.0, 0.5, 0.1)]).is_empty());
        assert!(result(vec![]).empty);
    }

    #[test]
    fn a_short_text_previews_whole_and_a_long_one_is_cut_with_an_ellipsis() {
        assert_eq!(preview("a\nb"), "a\nb");
        assert_eq!(preview("1\n2\n3"), "1\n2\n3");
        assert_eq!(preview("1\n2\n3\n4"), "1\n2\n3…");
        let long = "x".repeat(500);
        let p = preview(&long);
        assert_eq!(p.chars().count(), PREVIEW_CHARS + 1);
        assert!(p.ends_with('…'));
        // multibyte text is cut on characters, not bytes
        let accents = "é".repeat(300);
        assert!(preview(&accents).ends_with('…'));
    }

    #[test]
    fn the_result_counts_lines_and_characters_and_serializes_camel_case() {
        let r = result(vec![l("hello", 0.1, 0.9, 0.05), l("wörld", 0.1, 0.5, 0.05)]);
        assert_eq!((r.lines, r.chars, r.empty), (2, 11, false));
        assert_eq!(r.text, "hello\nwörld");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["preview"], "hello\nwörld");
        assert!(v.get("lines").is_some() && v.get("chars").is_some() && v.get("empty").is_some());
    }
}
