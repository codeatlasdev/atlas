//! Requests of the agent debug bridge v2 (`atlas desktop debug …`): parsing and validation, pure and
//! platform-independent. Everything an agent may ask is decided here, before any native or page call; the bridge
//! (`debug.rs`) only executes a parsed [`Req`].

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

/// Longest a single `wait`/`record --until` may block the bridge.
pub const MAX_WAIT_MS: u64 = 120_000;
pub const DEFAULT_WAIT_MS: u64 = 10_000;
/// Longest `record --seconds`.
pub const MAX_RECORD_SECS: u64 = 300;
/// Window size bounds (logical px) for `resize`: under the app's own minimum nothing lays out, over a few 5K
/// displays is a typo.
pub const MIN_WINDOW: f64 = 320.0;
pub const MAX_WINDOW: f64 = 5120.0;
pub const DEFAULT_PERF_SECS: u64 = 2;
pub const MAX_PERF_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The whole page (the webview's visible area).
    Page,
    /// The element at this CSS selector, clipped to the viewport.
    Selector(String),
    /// An explicit rect in CSS px.
    Rect { x: f64, y: f64, w: f64, h: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Req {
    Info,
    Logs {
        since: u64,
    },
    Eval {
        js: String,
    },
    State,
    Wait {
        selector: Option<String>,
        expr: Option<String>,
        timeout_ms: u64,
    },
    Click {
        selector: String,
    },
    Type {
        selector: String,
        text: String,
        delay_ms: u64,
    },
    A11y {
        selector: Option<String>,
    },
    /// WCAG contrast of every visible text node, optionally scoped to a selector.
    Contrast {
        selector: Option<String>,
    },
    /// Frame times and long tasks over `secs` seconds.
    Perf {
        secs: u64,
    },
    /// Record the raw trackpad scroll stream the app receives for `secs` seconds into a JSON trace at `out`.
    GestureRecord {
        out: PathBuf,
        secs: u64,
    },
    /// Replay a trace JSON through the swipe engine in-process (never OS input), at its recorded pace.
    GestureReplay {
        trace: PathBuf,
    },
    Shot {
        out: PathBuf,
        target: Target,
        /// The whole scrollable height of the page or element, not just what the viewport shows.
        full: bool,
        /// Include the native chrome (title bar, vibrancy): ScreenCaptureKit window capture instead of the page.
        window: bool,
    },
    Record {
        out: PathBuf,
        stop: Stop,
        fps: u32,
    },
    /// Resize the main window to `w` x `h` logical px (the page view follows); a flicker sweep resizes mid-recording.
    Resize {
        w: f64,
        h: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    AfterSecs(u64),
    /// Stop when this selector is visible.
    Selector(String),
}

fn str_field<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v[k].as_str().filter(|s| !s.is_empty())
}

fn out_path(v: &Value, exts: &[&str]) -> Result<PathBuf, String> {
    out_path_named(v, "out", exts)
}

fn out_path_named(v: &Value, key: &str, exts: &[&str]) -> Result<PathBuf, String> {
    let raw =
        str_field(v, key).ok_or_else(|| format!("{key} is required (an absolute file path)"))?;
    let p = Path::new(raw);
    if !p.is_absolute() {
        return Err(format!("{key} must be absolute, got {raw:?}"));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("{key} must not contain `..`"));
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !exts.contains(&ext.as_str()) {
        return Err(format!("{key} must end in .{}", exts.join(" or .")));
    }
    Ok(p.to_path_buf())
}

fn timeout(v: &Value) -> Result<u64, String> {
    match &v["timeout_ms"] {
        Value::Null => Ok(DEFAULT_WAIT_MS),
        t => {
            let n = t.as_u64().ok_or("timeout_ms must be a positive integer")?;
            if n == 0 || n > MAX_WAIT_MS {
                return Err(format!("timeout_ms must be 1..={MAX_WAIT_MS}"));
            }
            Ok(n)
        }
    }
}

fn target(v: &Value) -> Result<Target, String> {
    let sel = str_field(v, "selector");
    let rect = &v["rect"];
    match (sel, rect.is_null()) {
        (Some(_), false) => Err("give a selector or a rect, not both".into()),
        (Some(s), true) => Ok(Target::Selector(s.to_string())),
        (None, true) => Ok(Target::Page),
        (None, false) => {
            let n = |i: usize| rect.get(i).and_then(Value::as_f64);
            match (n(0), n(1), n(2), n(3)) {
                (Some(x), Some(y), Some(w), Some(h))
                    if w > 0.0 && h > 0.0 && x >= 0.0 && y >= 0.0 =>
                {
                    Ok(Target::Rect { x, y, w, h })
                }
                _ => {
                    Err("rect must be [x, y, width, height] in CSS px, width and height > 0".into())
                }
            }
        }
    }
}

pub fn parse(v: &Value) -> Result<Req, String> {
    match v["cmd"].as_str().unwrap_or_default() {
        "info" => Ok(Req::Info),
        "logs" => Ok(Req::Logs {
            since: v["since"].as_u64().unwrap_or(0),
        }),
        "eval" => Ok(Req::Eval {
            js: str_field(v, "js").ok_or("missing js")?.to_string(),
        }),
        "state" => Ok(Req::State),
        "a11y" => Ok(Req::A11y {
            selector: str_field(v, "selector").map(String::from),
        }),
        "contrast" => Ok(Req::Contrast {
            selector: str_field(v, "selector").map(String::from),
        }),
        "perf" => {
            let secs = match &v["seconds"] {
                Value::Null => DEFAULT_PERF_SECS,
                s => s
                    .as_u64()
                    .filter(|n| (1..=MAX_PERF_SECS).contains(n))
                    .ok_or_else(|| format!("seconds must be 1..={MAX_PERF_SECS}"))?,
            };
            Ok(Req::Perf { secs })
        }
        "gesture_record" => {
            let secs = v["seconds"]
                .as_u64()
                .filter(|n| (1..=MAX_RECORD_SECS).contains(n))
                .ok_or_else(|| format!("seconds must be 1..={MAX_RECORD_SECS}"))?;
            Ok(Req::GestureRecord {
                out: out_path(v, &["json"])?,
                secs,
            })
        }
        "gesture_replay" => Ok(Req::GestureReplay {
            trace: out_path_named(v, "trace", &["json"])?,
        }),
        "wait" => {
            let (selector, expr) = (str_field(v, "selector"), str_field(v, "expr"));
            if selector.is_some() == expr.is_some() {
                return Err("wait needs exactly one of selector or expr".into());
            }
            Ok(Req::Wait {
                selector: selector.map(String::from),
                expr: expr.map(String::from),
                timeout_ms: timeout(v)?,
            })
        }
        "click" => Ok(Req::Click {
            selector: str_field(v, "selector")
                .ok_or("click needs a selector")?
                .to_string(),
        }),
        "type" => Ok(Req::Type {
            selector: str_field(v, "selector")
                .ok_or("type needs a selector")?
                .to_string(),
            text: v["text"].as_str().ok_or("type needs text")?.to_string(),
            delay_ms: v["delay_ms"].as_u64().unwrap_or(0).min(1000),
        }),
        "shot" => {
            let window = v["window"].as_bool().unwrap_or(false);
            let full = v["full"].as_bool().unwrap_or(false);
            let target = target(v)?;
            if full && (window || matches!(target, Target::Rect { .. })) {
                return Err("--full is the whole scrollable page or element; it cannot be combined with --window or --rect".into());
            }
            if window && target != Target::Page {
                return Err("--window captures the whole window; it cannot be combined with a selector or rect".into());
            }
            Ok(Req::Shot {
                out: out_path(v, &["png"])?,
                target,
                full,
                window,
            })
        }
        "resize" => {
            let side = |k: &str| {
                v[k].as_f64()
                    .filter(|n| (MIN_WINDOW..=MAX_WINDOW).contains(n))
                    .ok_or_else(|| format!("{k} must be {MIN_WINDOW}..={MAX_WINDOW} logical px"))
            };
            Ok(Req::Resize {
                w: side("width")?,
                h: side("height")?,
            })
        }
        "record" => {
            let stop = match (v["seconds"].as_u64(), str_field(v, "until")) {
                (Some(_), Some(_)) => return Err("give --seconds or --until, not both".into()),
                (Some(s), None) if (1..=MAX_RECORD_SECS).contains(&s) => Stop::AfterSecs(s),
                (Some(_), None) => return Err(format!("seconds must be 1..={MAX_RECORD_SECS}")),
                (None, Some(sel)) => Stop::Selector(sel.to_string()),
                (None, None) => return Err("record needs --seconds N or --until <selector>".into()),
            };
            let fps = v["fps"].as_u64().unwrap_or(60) as u32;
            if ![24, 30, 60].contains(&fps) {
                return Err("fps must be 24, 30 or 60".into());
            }
            Ok(Req::Record {
                out: out_path(v, &["mov", "mp4"])?,
                stop,
                fps,
            })
        }
        other => Err(format!(
            "unknown cmd {other:?}; expected info|logs|eval|state|wait|click|type|a11y|contrast|perf|shot|record|resize|gesture_record|gesture_replay"
        )),
    }
}

// Unix absolute paths (`/tmp/…`): the debug bridge drives macOS agent instances.
#[cfg(all(test, unix))]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn gesture_requests_validate_their_paths_and_bound_the_recording() {
        assert_eq!(
            parse(&json!({"cmd":"gesture_record","out":"/tmp/a.json","seconds":5})).unwrap(),
            Req::GestureRecord {
                out: "/tmp/a.json".into(),
                secs: 5
            }
        );
        for bad in [
            json!(0),
            json!(MAX_RECORD_SECS + 1),
            json!("x"),
            Value::Null,
        ] {
            assert!(
                parse(&json!({"cmd":"gesture_record","out":"/tmp/a.json","seconds":bad})).is_err()
            );
        }
        // The recorder writes a file as the app: absolute, no `..`, .json only.
        for out in ["a.json", "/tmp/../etc/a.json", "/tmp/a.txt"] {
            assert!(
                parse(&json!({"cmd":"gesture_record","out":out,"seconds":5})).is_err(),
                "{out}"
            );
        }
        assert!(
            parse(&json!({"cmd":"gesture_replay"}))
                .unwrap_err()
                .contains("trace is required")
        );
        assert_eq!(
            parse(&json!({"cmd":"gesture_replay","trace":"/tmp/t.json"})).unwrap(),
            Req::GestureReplay {
                trace: "/tmp/t.json".into()
            }
        );
    }

    #[test]
    fn wait_needs_exactly_one_of_selector_or_expr_and_a_bounded_timeout() {
        assert!(
            parse(&json!({"cmd":"wait"}))
                .unwrap_err()
                .contains("exactly one")
        );
        assert!(
            parse(&json!({"cmd":"wait","selector":"a","expr":"b"}))
                .unwrap_err()
                .contains("exactly one")
        );
        assert_eq!(
            parse(&json!({"cmd":"wait","selector":".ready"})).unwrap(),
            Req::Wait {
                selector: Some(".ready".into()),
                expr: None,
                timeout_ms: DEFAULT_WAIT_MS
            }
        );
        for bad in [json!(0), json!(MAX_WAIT_MS + 1), json!("soon"), json!(-5)] {
            assert!(
                parse(&json!({"cmd":"wait","expr":"1","timeout_ms":bad})).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn shot_targets_the_page_an_element_or_a_rect_never_two_at_once() {
        let shot = |extra: Value| {
            let mut v = json!({"cmd":"shot","out":"/tmp/a.png"});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            parse(&v)
        };
        assert!(matches!(
            shot(json!({})).unwrap(),
            Req::Shot {
                target: Target::Page,
                window: false,
                ..
            }
        ));
        assert!(matches!(
            shot(json!({"selector":".card"})).unwrap(),
            Req::Shot { target: Target::Selector(s), .. } if s == ".card"
        ));
        assert!(matches!(
            shot(json!({"rect":[10,20,300,200]})).unwrap(),
            Req::Shot { target: Target::Rect { x, y, w, h }, .. } if (x, y, w, h) == (10.0, 20.0, 300.0, 200.0)
        ));
        assert!(
            shot(json!({"selector":"a","rect":[0,0,1,1]}))
                .unwrap_err()
                .contains("not both")
        );
        for bad in [
            json!([1, 2, 3]),
            json!([0, 0, 0, 5]),
            json!([-1, 0, 5, 5]),
            json!("x"),
        ] {
            assert!(shot(json!({"rect": bad})).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_window_shot_is_the_whole_window_and_cannot_have_a_target() {
        assert!(parse(&json!({"cmd":"shot","out":"/tmp/a.png","window":true})).is_ok());
        let e = parse(&json!({"cmd":"shot","out":"/tmp/a.png","window":true,"selector":".x"}))
            .unwrap_err();
        assert!(e.contains("whole window"), "{e}");
    }

    #[test]
    fn output_paths_are_absolute_without_parent_dirs_and_have_the_right_extension() {
        for out in [
            "a.png",
            "../a.png",
            "/tmp/../a.png",
            "/tmp/a.jpg",
            "/tmp/a",
            "",
        ] {
            assert!(parse(&json!({"cmd":"shot","out":out})).is_err(), "{out:?}");
        }
        assert!(
            parse(&json!({"cmd":"record","seconds":3,"out":"/tmp/a.png"})).is_err(),
            "a video is not a png"
        );
        assert!(parse(&json!({"cmd":"record","seconds":3,"out":"/tmp/a.mov"})).is_ok());
    }

    #[test]
    fn record_stops_after_seconds_or_when_a_selector_shows_exactly_one_of_them() {
        let rec = |extra: Value| {
            let mut v = json!({"cmd":"record","out":"/tmp/a.mov"});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            parse(&v)
        };
        assert!(matches!(
            rec(json!({"seconds":5})).unwrap(),
            Req::Record {
                stop: Stop::AfterSecs(5),
                fps: 60,
                ..
            }
        ));
        assert!(matches!(
            rec(json!({"until":"[data-intro=\"done\"]","fps":30})).unwrap(),
            Req::Record {
                stop: Stop::Selector(_),
                fps: 30,
                ..
            }
        ));
        assert!(rec(json!({})).unwrap_err().contains("--seconds"));
        assert!(
            rec(json!({"seconds":5,"until":"a"}))
                .unwrap_err()
                .contains("not both")
        );
        assert!(rec(json!({"seconds":0})).is_err());
        assert!(rec(json!({"seconds":MAX_RECORD_SECS + 1})).is_err());
        assert!(rec(json!({"seconds":5,"fps":59})).is_err());
    }

    /// Sweep gap: no way to resize the window while a recording runs, so "window resize" was the one surface a flicker
    /// sweep could not exercise. The size is bounded: a typo must not ask for a 100000 px window.
    #[test]
    fn resize_takes_a_bounded_positive_logical_size() {
        let r = |w: Value, h: Value| parse(&json!({"cmd":"resize","width":w,"height":h}));
        assert_eq!(
            r(json!(1100), json!(700)).unwrap(),
            Req::Resize {
                w: 1100.0,
                h: 700.0
            }
        );
        assert!(r(json!(1100.5), json!(700.25)).is_ok());
        assert!(parse(&json!({"cmd":"resize"})).is_err());
        assert!(r(json!(1100), Value::Null).is_err());
        assert!(r(json!(0), json!(700)).is_err());
        assert!(r(json!(-5), json!(700)).is_err());
        assert!(r(json!(MIN_WINDOW - 1.0), json!(700)).is_err());
        assert!(r(json!(MAX_WINDOW + 1.0), json!(700)).is_err());
        assert!(r(json!("1100"), json!(700)).is_err());
        assert!(r(json!(MIN_WINDOW), json!(MAX_WINDOW)).is_ok());
    }

    #[test]
    fn click_and_type_need_a_selector_and_type_needs_text_with_a_capped_delay() {
        assert!(parse(&json!({"cmd":"click"})).is_err());
        assert!(parse(&json!({"cmd":"type","selector":"input"})).is_err());
        assert_eq!(
            parse(&json!({"cmd":"type","selector":"input","text":"orb","delay_ms":99999})).unwrap(),
            Req::Type {
                selector: "input".into(),
                text: "orb".into(),
                delay_ms: 1000
            }
        );
        // empty text is a valid "clear nothing, type nothing", but a missing key is a mistake
        assert!(parse(&json!({"cmd":"type","selector":"input","text":""})).is_ok());
    }

    #[test]
    fn a_full_shot_is_the_whole_scrollable_page_and_not_a_rect() {
        let v = |extra: Value| {
            let mut v = json!({"cmd":"shot","out":"/tmp/a.png","full":true});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            parse(&v)
        };
        assert!(matches!(
            v(json!({})).unwrap(),
            Req::Shot { full: true, .. }
        ));
        assert!(matches!(
            v(json!({"selector":".list"})).unwrap(),
            Req::Shot {
                full: true,
                target: Target::Selector(_),
                ..
            }
        ));
        assert!(
            v(json!({"rect":[0,0,10,10]}))
                .unwrap_err()
                .contains("--full")
        );
        assert!(v(json!({"window":true})).unwrap_err().contains("--full"));
    }

    #[test]
    fn perf_runs_for_a_bounded_number_of_seconds() {
        assert_eq!(
            parse(&json!({"cmd":"perf"})).unwrap(),
            Req::Perf {
                secs: DEFAULT_PERF_SECS
            }
        );
        assert_eq!(
            parse(&json!({"cmd":"perf","seconds":3})).unwrap(),
            Req::Perf { secs: 3 }
        );
        for bad in [json!(0), json!(MAX_PERF_SECS + 1), json!("x"), json!(-1)] {
            assert!(
                parse(&json!({"cmd":"perf","seconds":bad})).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a11y_may_be_scoped_to_a_selector() {
        assert_eq!(
            parse(&json!({"cmd":"a11y"})).unwrap(),
            Req::A11y { selector: None }
        );
        assert_eq!(
            parse(&json!({"cmd":"a11y","selector":"nav"})).unwrap(),
            Req::A11y {
                selector: Some("nav".into())
            }
        );
    }

    #[test]
    fn contrast_may_be_scoped_to_a_selector() {
        assert_eq!(
            parse(&json!({"cmd":"contrast"})).unwrap(),
            Req::Contrast { selector: None }
        );
        assert_eq!(
            parse(&json!({"cmd":"contrast","selector":"aside.sidebar"})).unwrap(),
            Req::Contrast {
                selector: Some("aside.sidebar".into())
            }
        );
    }

    #[test]
    fn an_unknown_command_lists_what_exists() {
        let e = parse(&json!({"cmd":"poke"})).unwrap_err();
        assert!(
            e.contains("unknown cmd")
                && e.contains("shot")
                && e.contains("a11y")
                && e.contains("perf"),
            "{e}"
        );
        assert!(parse(&json!({})).is_err());
    }

    #[test]
    fn no_command_sends_an_arbitrary_native_selector_to_the_app() {
        // The earlier investigation probe `poke` (any ObjC selector at the webview) must not exist in any build.
        assert!(parse(&json!({"cmd":"poke","selector":"_setThrottleStateForTesting:"})).is_err());
        assert!(parse(&json!({"cmd":"webviews"})).is_err());
    }
}
