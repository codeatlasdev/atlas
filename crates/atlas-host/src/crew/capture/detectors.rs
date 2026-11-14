//! TEMPORARY: the Rust side of the UI detectors (#215). The detectors themselves are
//! `detectors.mjs`, a local stand-in for `@codeatlasdev/ui/testing/detectors` (codeatlasdev/ui#1290)
//! with the same API; this file only reads what they wrote. When ui#1290 ships, `detectors.mjs`,
//! `fixtures/` and the section "UI detectors (temporary)" of docs/crew.md go, and the runner imports
//! the package instead. [`Finding`] and [`read`] stay: the shape is the package's.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The detector names, in the order `detectors.mjs` runs them. The same as ui#1290's.
pub const NAMES: [&str; 7] = [
    "empty-band",
    "gutter",
    "sibling-overlap",
    "raw-text",
    "action-hierarchy",
    "cls",
    "focus-after-tap",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
}

/// `{ detector, severity, message, rect?, selector? }`: ui#1290's `Finding`, field for field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub detector: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

/// `DOMRectInit`, in CSS px of the whole page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The findings file of one capture cell. A file that is not a list of findings is an error
/// naming it: a gate must not read garbage as "nothing wrong".
pub fn read(path: &Path) -> Result<Vec<Finding>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is not a list of findings: {e}", path.display()))
}

/// `<cell>.png` -> `<cell>.findings.json`, the file the runner writes next to the picture.
pub fn findings_path(png: &Path) -> std::path::PathBuf {
    png.with_extension("findings.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_finding_is_ui_1290s_shape_and_optional_fields_stay_out() {
        let f: Finding = serde_json::from_value(json!({
            "detector": "gutter", "severity": "error", "message": "4 px"
        }))
        .unwrap();
        assert_eq!(f.severity, Severity::Error);
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({"detector": "gutter", "severity": "error", "message": "4 px"})
        );
        let f: Finding = serde_json::from_value(json!({
            "detector": "cls", "severity": "warn", "message": "m", "selector": "#a",
            "rect": {"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0}
        }))
        .unwrap();
        assert_eq!(f.rect.unwrap().height, 4.0);
        assert!(
            serde_json::from_value::<Finding>(
                json!({"detector": "x", "severity": "fatal", "message": "m"})
            )
            .is_err(),
            "only error and warn exist"
        );
    }

    #[test]
    fn the_findings_file_sits_next_to_the_png_and_garbage_in_it_is_an_error() {
        assert_eq!(
            findings_path(Path::new("/o/webkit-390px-dark.png")),
            Path::new("/o/webkit-390px-dark.findings.json")
        );
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.findings.json");
        std::fs::write(&p, "[]").unwrap();
        assert!(read(&p).unwrap().is_empty());
        std::fs::write(&p, "{\"oops\": 1}").unwrap();
        assert!(read(&p).unwrap_err().contains("not a list of findings"));
        assert!(
            read(&tmp.path().join("missing"))
                .unwrap_err()
                .contains("cannot read")
        );
    }
}
