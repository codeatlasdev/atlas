//! Validation of `capture_start|stop|shot` requests (ctl socket and debug bridge). Pure and
//! platform-independent: everything that can be refused is refused here, before any native call.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

pub const FPS_CHOICES: [u32; 3] = [24, 30, 60];
pub const MIN_SIDE: u32 = 320;
pub const MAX_SIDE: u32 = 7680;
/// WebKit renders the page of a window that is not key at 30 Hz, whatever the preferences say (measured, see
/// `occlusion.rs`); the video container still runs at the requested fps.
pub const UNFOCUSED_PAGE_FPS: u32 = 30;

/// The rate the page content of a recording can reach, `None` when it is not capped below the requested `fps`.
pub fn page_fps_cap(window_is_key: bool, fps: u32) -> Option<u32> {
    (!window_is_key && fps > UNFOCUSED_PAGE_FPS).then_some(UNFOCUSED_PAGE_FPS)
}

/// A recording that nobody stopped is stopped by the host after this long.
pub const MAX_RECORD_SECS: u64 = 30 * 60;

/// How a still is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotMode {
    /// ScreenCaptureKit window capture: what the compositor shows (native chrome, vibrancy), needs
    /// Screen Recording permission, window must be on screen.
    Window,
    /// `WKWebView.takeSnapshot`: the page only, deterministic, no permission, works occluded.
    Snapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    pub out: PathBuf,
    pub fps: u32,
    pub window: String,
    /// Logical size to set the window to first; `None` keeps the current one.
    pub size: Option<(u32, u32)>,
    pub cursor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub out: PathBuf,
    pub mode: ShotMode,
    pub window: String,
    pub size: Option<(u32, u32)>,
}

fn out_path(args: &Value, exts: &[&str]) -> Result<PathBuf, String> {
    let raw = args["out"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("out is required (an absolute file path)")?;
    let p = Path::new(raw);
    if !p.is_absolute() {
        return Err(format!("out must be absolute, got {raw:?}"));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("out must not contain `..`".into());
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !exts.contains(&ext.as_str()) {
        return Err(format!("out must end in .{}", exts.join(" or .")));
    }
    if p.file_name().is_none() {
        return Err("out must name a file".into());
    }
    Ok(p.to_path_buf())
}

fn window(args: &Value) -> Result<String, String> {
    match &args["window"] {
        Value::Null => Ok("main".into()),
        Value::String(s) if !s.is_empty() && s.len() <= 64 => Ok(s.clone()),
        _ => Err("window must be a window label (default \"main\")".into()),
    }
}

fn size(args: &Value) -> Result<Option<(u32, u32)>, String> {
    let s = &args["size"];
    if s.is_null() {
        return Ok(None);
    }
    let side = |k: &str| {
        s[k].as_u64()
            .filter(|v| (MIN_SIDE as u64..=MAX_SIDE as u64).contains(v) && v % 2 == 0)
            .map(|v| v as u32)
            .ok_or_else(|| {
                format!("size.{k} must be an even integer {MIN_SIDE}..{MAX_SIDE} (video encoders reject odd sizes)")
            })
    };
    Ok(Some((side("width")?, side("height")?)))
}

pub fn parse_start(args: &Value) -> Result<Start, String> {
    let fps = match &args["fps"] {
        Value::Null => 60,
        v => v
            .as_u64()
            .map(|f| f as u32)
            .filter(|f| FPS_CHOICES.contains(f))
            .ok_or("fps must be 24, 30 or 60")?,
    };
    let cursor = match &args["cursor"] {
        Value::Null => false,
        Value::Bool(b) => *b,
        _ => return Err("cursor must be a boolean".into()),
    };
    Ok(Start {
        out: out_path(args, &["mov", "mp4"])?,
        fps,
        window: window(args)?,
        size: size(args)?,
        cursor,
    })
}

pub fn parse_shot(args: &Value) -> Result<Shot, String> {
    let mode = match args["mode"].as_str() {
        None | Some("window") => ShotMode::Window,
        Some("snapshot") => ShotMode::Snapshot,
        Some(other) => {
            return Err(format!(
                "mode must be \"window\" or \"snapshot\", got {other:?}"
            ));
        }
    };
    Ok(Shot {
        out: out_path(args, &["png"])?,
        mode,
        window: window(args)?,
        size: size(args)?,
    })
}

/// Pixel size of the output for a window of `logical` points on a display of `scale` pixels per point.
pub fn pixel_size(logical: (f64, f64), scale: f64) -> (u32, u32) {
    // Video codecs want even dimensions; round to the nearest even, never below 2.
    let even = |v: f64| (((v * scale) / 2.0).round() as u32).max(1) * 2;
    (even(logical.0), even(logical.1))
}

// Unix absolute paths (`/tmp/…`): capture is for the macOS app.
#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn an_unfocused_window_caps_the_page_content_at_30_and_a_key_window_does_not() {
        use super::page_fps_cap;
        assert_eq!(page_fps_cap(false, 60), Some(30));
        assert_eq!(
            page_fps_cap(false, 30),
            None,
            "30 fps is not capped below itself"
        );
        assert_eq!(page_fps_cap(false, 24), None);
        assert_eq!(page_fps_cap(true, 60), None);
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn start_defaults_are_60fps_main_window_no_cursor() {
        let s = parse_start(&json!({"out": "/tmp/a.mov"})).unwrap();
        assert_eq!(s.fps, 60);
        assert_eq!(s.window, "main");
        assert_eq!(s.size, None);
        assert!(!s.cursor);
    }

    #[test]
    fn out_must_be_absolute_without_parent_dirs_and_a_known_extension() {
        for bad in [
            "rel/a.mov",
            "/tmp/../etc/a.mov",
            "/tmp/a.txt",
            "/tmp/a",
            "/tmp/",
            "",
        ] {
            assert!(
                parse_start(&json!({ "out": bad })).is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(parse_start(&json!({})).is_err());
        assert!(parse_start(&json!({"out": 5})).is_err());
        assert!(parse_start(&json!({"out": "/tmp/a.MP4"})).is_ok());
    }

    #[test]
    fn shots_are_png_only_and_videos_are_not_png() {
        assert!(parse_shot(&json!({"out": "/tmp/a.png"})).is_ok());
        assert!(parse_shot(&json!({"out": "/tmp/a.mov"})).is_err());
        assert!(parse_start(&json!({"out": "/tmp/a.png"})).is_err());
    }

    #[test]
    fn fps_is_one_of_the_supported_rates() {
        for ok in [24, 30, 60] {
            assert_eq!(
                parse_start(&json!({"out": "/tmp/a.mov", "fps": ok}))
                    .unwrap()
                    .fps,
                ok
            );
        }
        for bad in [
            json!(0),
            json!(45),
            json!(120),
            json!("60"),
            json!(-1),
            json!(29.5),
        ] {
            assert!(parse_start(&json!({"out": "/tmp/a.mov", "fps": bad})).is_err());
        }
    }

    #[test]
    fn size_is_even_and_bounded_on_both_sides() {
        let ok = parse_start(&json!({"out": "/tmp/a.mov", "size": {"width": 1280, "height": 800}}))
            .unwrap();
        assert_eq!(ok.size, Some((1280, 800)));
        for bad in [
            json!({"width": 1281, "height": 800}),
            json!({"width": 1280, "height": 801}),
            json!({"width": 100, "height": 800}),
            json!({"width": 9000, "height": 800}),
            json!({"width": 1280}),
            json!({"width": "1280", "height": 800}),
        ] {
            assert!(parse_start(&json!({"out": "/tmp/a.mov", "size": bad})).is_err());
        }
    }

    #[test]
    fn window_label_defaults_and_rejects_non_strings() {
        assert_eq!(
            parse_shot(&json!({"out": "/tmp/a.png", "window": "main"}))
                .unwrap()
                .window,
            "main"
        );
        assert!(parse_shot(&json!({"out": "/tmp/a.png", "window": 3})).is_err());
        assert!(parse_shot(&json!({"out": "/tmp/a.png", "window": ""})).is_err());
    }

    #[test]
    fn shot_mode_defaults_to_window_and_refuses_unknown() {
        assert_eq!(
            parse_shot(&json!({"out": "/tmp/a.png"})).unwrap().mode,
            ShotMode::Window
        );
        assert_eq!(
            parse_shot(&json!({"out": "/tmp/a.png", "mode": "snapshot"}))
                .unwrap()
                .mode,
            ShotMode::Snapshot
        );
        assert!(parse_shot(&json!({"out": "/tmp/a.png", "mode": "screen"})).is_err());
    }

    #[test]
    fn pixel_size_scales_and_stays_even() {
        assert_eq!(pixel_size((1280.0, 800.0), 2.0), (2560, 1600));
        assert_eq!(pixel_size((1281.0, 799.0), 1.0), (1282, 800));
        assert_eq!(pixel_size((0.0, 0.0), 2.0), (2, 2));
        let (w, h) = pixel_size((1000.5, 333.3), 2.0);
        assert!(w % 2 == 0 && h % 2 == 0);
    }
}
