//! `atlas desktop debug …`: the one surface agents use to see and drive a debug build of Atlas Desktop
//! (`crates/atlas-desktop/src/debug.rs`, one JSON line per request over a unix socket). Everything works while the
//! window is hidden, unfocused or on the private agent display: shots are `WKWebView` snapshots, `click`/`type` are
//! DOM events inside the webview, never OS input. The bridge validates every request (`debug_req.rs`); this side
//! only maps flags to it.
//!
//! The socket is `$ATLAS_DEBUG_SOCK` or `~/.atlas/desktop-debug.sock`; `atlas desktop --agent` prints the one of
//! the instance it started.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde_json::{Value, json};

/// Replies of long operations (record, wait, perf) outlive the default read timeout by their own duration.
const BASE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Subcommand)]
pub enum DebugCmd {
    /// pid, URL, visibility, native window id, whether this is an agent instance, and the socket
    Info,
    /// Webview console, uncaught errors and CSP violations (level "csp")
    Logs {
        /// Only entries with seq >= this (print `seq` from a previous call to follow)
        #[arg(long, default_value_t = 0)]
        since: u64,
        /// Only this level: log|info|warn|error|debug|csp
        #[arg(long)]
        level: Option<String>,
    },
    /// Evaluate JS in the page and print the JSON result.
    /// An expression (`document.title`) or a body with `return` (`const n = 1; return n`).
    Eval { js: String },
    /// What the app is doing: projects with service states, worktrees, selection
    State,
    /// Screenshot the page (WKWebView snapshot: no permission, works occluded) and print the PNG path
    Shot {
        #[arg(default_value = "/tmp/atlas-desktop.png")]
        path: PathBuf,
        /// Only this element (CSS selector), clipped to the viewport
        #[arg(long, conflicts_with_all = ["rect", "window"])]
        selector: Option<String>,
        /// Only this rect, in CSS px: x,y,width,height
        #[arg(long, value_name = "x,y,w,h", value_parser = parse_rect, conflicts_with_all = ["full", "window"])]
        rect: Option<[f64; 4]>,
        /// The whole scrollable page (or element): the view grows to fit the content for the snapshot
        #[arg(long, conflicts_with = "window")]
        full: bool,
        /// The whole native window as the compositor shows it (needs Screen Recording permission)
        #[arg(long)]
        window: bool,
    },
    /// Record the window to a .mov with the app's own recorder (needs Screen Recording permission)
    Record {
        path: PathBuf,
        /// Record this many seconds
        #[arg(long, conflicts_with = "until")]
        seconds: Option<u64>,
        /// Record until this selector is visible
        #[arg(long)]
        until: Option<String>,
        #[arg(long, default_value_t = 60)]
        fps: u32,
    },
    /// Block until a selector is visible, or a JS expression is truthy
    Wait {
        /// CSS selector, or (with --expr) a JS expression
        target: String,
        /// Treat `target` as a JS expression instead of a selector
        #[arg(long)]
        expr: bool,
        /// Give up after this many milliseconds
        #[arg(long, default_value_t = 10_000)]
        timeout: u64,
    },
    /// Click the first visible element matching a selector (a DOM click, never OS input)
    Click { selector: String },
    /// Type into an input or textarea (DOM input events, never OS input)
    Type {
        selector: String,
        text: String,
        /// Pause between characters, in milliseconds
        #[arg(long, default_value_t = 0)]
        delay: u64,
    },
    /// Accessibility tree: roles, names, nesting; controls without a name are flagged
    A11y {
        /// Only this subtree (CSS selector)
        selector: Option<String>,
    },
    /// WCAG contrast of every visible text node (fg x opacity chain vs composited bg): failures with selector,
    /// ratio, required ratio and the theme tokens involved; backgrounds that are images/gradients are `unknown`
    Contrast {
        /// Only this subtree (CSS selector)
        selector: Option<String>,
    },
    /// Frame times (rAF/s, p50/p95/max) and long tasks over a few seconds
    Perf {
        #[arg(long, default_value_t = 2)]
        seconds: u64,
    },
    /// Record the raw trackpad scroll stream (phases, deltas, pointer) for N seconds into a JSON trace. Do real
    /// swipes in the app meanwhile; add the expectation (`expect`) to the file and keep it as a fixture.
    GestureRecord {
        path: PathBuf,
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Replay a trace in-process through the swipe engine at its recorded pace (never OS input)
    GestureReplay { trace: PathBuf },
}

/// `x,y,w,h` in CSS px; the bridge rejects non-positive sizes, this rejects what is not four numbers.
pub fn parse_rect(s: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return Err("expected x,y,width,height, e.g. 0,0,400,300".into());
    }
    let mut r = [0.0; 4];
    for (slot, p) in r.iter_mut().zip(&parts) {
        *slot = p
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{p:?} is not a number"))?;
    }
    Ok(r)
}

/// An absolute path: the app writes the file, so a relative one would land in the app's working directory.
fn absolute(p: &Path) -> Result<PathBuf> {
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(p))
}

/// The bridge request for a command, and how long to wait for its reply beyond the base timeout.
pub fn to_request(cmd: &DebugCmd) -> Result<(Value, Duration)> {
    let extra = |ms: u64| Duration::from_millis(ms);
    Ok(match cmd {
        DebugCmd::Info => (json!({ "cmd": "info" }), Duration::ZERO),
        DebugCmd::Logs { since, .. } => (json!({ "cmd": "logs", "since": since }), Duration::ZERO),
        DebugCmd::Eval { js } => (json!({ "cmd": "eval", "js": js }), Duration::ZERO),
        DebugCmd::State => (json!({ "cmd": "state" }), Duration::ZERO),
        DebugCmd::Shot {
            path,
            selector,
            rect,
            full,
            window,
        } => {
            let mut v = json!({ "cmd": "shot", "out": absolute(path)? });
            if let Some(s) = selector {
                v["selector"] = json!(s);
            }
            if let Some(r) = rect {
                v["rect"] = json!(r);
            }
            if *full {
                v["full"] = json!(true);
            }
            if *window {
                v["window"] = json!(true);
            }
            (v, extra(20_000))
        }
        DebugCmd::Record {
            path,
            seconds,
            until,
            fps,
        } => {
            let mut v = json!({ "cmd": "record", "out": absolute(path)?, "fps": fps });
            if let Some(s) = seconds {
                v["seconds"] = json!(s);
            }
            if let Some(u) = until {
                v["until"] = json!(u);
            }
            // The reply comes after the recording: its length (or, until a selector, the 5 min cap) plus encoding.
            (v, extra(seconds.unwrap_or(300) * 1000 + 30_000))
        }
        DebugCmd::Wait {
            target,
            expr,
            timeout,
        } => {
            let key = if *expr { "expr" } else { "selector" };
            (
                json!({ "cmd": "wait", key: target, "timeout_ms": timeout }),
                extra(*timeout + 5_000),
            )
        }
        DebugCmd::Click { selector } => (
            json!({ "cmd": "click", "selector": selector }),
            Duration::ZERO,
        ),
        DebugCmd::Type {
            selector,
            text,
            delay,
        } => (
            json!({ "cmd": "type", "selector": selector, "text": text, "delay_ms": delay }),
            extra(delay.saturating_mul(text.chars().count() as u64)),
        ),
        DebugCmd::A11y { selector } | DebugCmd::Contrast { selector } => {
            let cmd = if matches!(cmd, DebugCmd::A11y { .. }) {
                "a11y"
            } else {
                "contrast"
            };
            let mut v = json!({ "cmd": cmd });
            if let Some(s) = selector {
                v["selector"] = json!(s);
            }
            (v, Duration::ZERO)
        }
        DebugCmd::Perf { seconds } => (
            json!({ "cmd": "perf", "seconds": seconds }),
            extra(seconds * 1000),
        ),
        DebugCmd::GestureRecord { path, seconds } => (
            json!({ "cmd": "gesture_record", "out": absolute(path)?, "seconds": seconds }),
            extra(seconds * 1000),
        ),
        DebugCmd::GestureReplay { trace } => (
            json!({ "cmd": "gesture_replay", "trace": absolute(trace)? }),
            extra(30_000),
        ),
    })
}

pub fn run(cmd: DebugCmd) -> Result<()> {
    let (req, extra) = to_request(&cmd)?;
    let result = request(&req, BASE_TIMEOUT + extra)?;
    match cmd {
        DebugCmd::Logs { level, .. } => {
            for l in result.as_array().into_iter().flatten() {
                if level.as_deref().is_some_and(|want| l["level"] != want) {
                    continue;
                }
                println!(
                    "{} {} [{}] {}",
                    l["seq"],
                    l["ts"].as_str().unwrap_or(""),
                    l["level"].as_str().unwrap_or(""),
                    l["msg"].as_str().unwrap_or("")
                );
            }
        }
        DebugCmd::Shot { .. } | DebugCmd::Record { .. } => {
            println!("{}", result["out"].as_str().unwrap_or_default());
            eprintln!("{}", serde_json::to_string(&result)?);
        }
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

fn request(req: &Value, timeout: Duration) -> Result<Value> {
    let path = atlas_core::paths::desktop_debug_socket();
    let stream = atlas_core::ipc::connect_sync(&path).with_context(|| {
        format!(
            "no debug bridge at {} — start an agent instance (`atlas desktop --agent`, or `bun run desktop:agent`) and point ATLAS_DEBUG_SOCK at the socket it prints",
            path.display()
        )
    })?;
    stream.set_read_timeout(Some(timeout))?;
    (&stream).write_all(format!("{req}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    if line.is_empty() {
        bail!("the debug bridge closed the connection (the app exited or crashed)");
    }
    let reply: Value = serde_json::from_str(&line).context("bad reply from debug bridge")?;
    if reply["ok"] != true {
        bail!(
            "{}",
            reply["error"].as_str().unwrap_or("debug bridge error")
        );
    }
    Ok(reply["result"].clone())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        cmd: DebugCmd,
    }

    fn req(args: &[&str]) -> Value {
        let cli = Cli::try_parse_from(std::iter::once("debug").chain(args.iter().copied()))
            .unwrap_or_else(|e| panic!("{args:?}: {e}"));
        to_request(&cli.cmd).unwrap().0
    }

    fn refused(args: &[&str]) -> bool {
        Cli::try_parse_from(std::iter::once("debug").chain(args.iter().copied())).is_err()
    }

    // Unix absolute paths (`/tmp/…`); the debug bridge is for macOS agent instances.
    #[cfg(unix)]
    #[test]
    fn gesture_record_and_replay_map_to_the_bridge_with_absolute_paths() {
        let v = req(&["gesture-record", "/tmp/s.json", "--seconds", "8"]);
        assert_eq!(
            v,
            json!({"cmd":"gesture_record","out":"/tmp/s.json","seconds":8})
        );
        // seconds defaults to 10
        assert_eq!(req(&["gesture-record", "/tmp/s.json"])["seconds"], 10);
        assert_eq!(
            req(&["gesture-replay", "/tmp/s.json"]),
            json!({"cmd":"gesture_replay","trace":"/tmp/s.json"})
        );
        assert!(refused(&["gesture-record"]));
    }

    // Unix absolute paths (`/tmp/…`); the debug bridge is for macOS agent instances.
    #[cfg(unix)]
    #[test]
    fn shot_flags_become_the_bridge_fields_and_the_path_is_made_absolute() {
        let v = req(&["shot", "/tmp/a.png", "--selector", ".card"]);
        assert_eq!(
            v,
            json!({"cmd":"shot","out":"/tmp/a.png","selector":".card"})
        );
        let v = req(&["shot", "/tmp/a.png", "--rect", "10, 20,300,200"]);
        assert_eq!(v["rect"], json!([10.0, 20.0, 300.0, 200.0]));
        let v = req(&["shot", "--full"]);
        assert_eq!(v["full"], true);
        assert_eq!(v["out"], "/tmp/atlas-desktop.png");
        let v = req(&["shot", "rel/x.png"]);
        let out = v["out"].as_str().unwrap();
        assert!(
            Path::new(out).is_absolute() && out.ends_with("rel/x.png"),
            "{out}"
        );
    }

    #[test]
    fn shot_targets_that_cannot_be_combined_are_refused_before_any_request() {
        assert!(refused(&["shot", "--selector", "a", "--rect", "0,0,1,1"]));
        assert!(refused(&["shot", "--rect", "0,0,1,1", "--full"]));
        assert!(refused(&["shot", "--window", "--selector", "a"]));
        assert!(refused(&["shot", "--window", "--full"]));
    }

    #[test]
    fn a_rect_is_exactly_four_numbers() {
        assert_eq!(parse_rect("1,2,3,4").unwrap(), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(parse_rect("0.5,0,10,10").unwrap()[0], 0.5);
        for bad in ["", "1,2,3", "1,2,3,4,5", "a,b,c,d", "1,2,3,"] {
            assert!(parse_rect(bad).is_err(), "{bad:?}");
        }
    }

    // Unix absolute paths (`/tmp/…`); the debug bridge is for macOS agent instances.
    #[cfg(unix)]
    #[test]
    fn record_takes_seconds_or_a_selector_never_both() {
        let v = req(&["record", "/tmp/a.mov", "--seconds", "5"]);
        assert_eq!(
            v,
            json!({"cmd":"record","out":"/tmp/a.mov","fps":60,"seconds":5})
        );
        let v = req(&[
            "record",
            "/tmp/a.mov",
            "--until",
            "[data-intro=\"done\"]",
            "--fps",
            "30",
        ]);
        assert_eq!(v["until"], "[data-intro=\"done\"]");
        assert_eq!(v["fps"], 30);
        assert!(refused(&[
            "record",
            "/tmp/a.mov",
            "--seconds",
            "5",
            "--until",
            "a"
        ]));
    }

    #[test]
    fn a_long_command_waits_for_its_own_duration_before_giving_up_on_the_reply() {
        let t = |args: &[&str]| {
            let cli =
                Cli::try_parse_from(std::iter::once("debug").chain(args.iter().copied())).unwrap();
            to_request(&cli.cmd).unwrap().1
        };
        assert!(t(&["record", "/tmp/a.mov", "--seconds", "60"]) >= Duration::from_secs(60));
        assert!(t(&["wait", ".x", "--timeout", "90000"]) >= Duration::from_secs(90));
        assert!(t(&["perf", "--seconds", "10"]) >= Duration::from_secs(10));
        assert_eq!(t(&["info"]), Duration::ZERO);
    }

    #[test]
    fn wait_is_a_selector_unless_it_is_an_expression() {
        assert_eq!(
            req(&["wait", ".ready"]),
            json!({"cmd":"wait","selector":".ready","timeout_ms":10000})
        );
        let v = req(&[
            "wait",
            "window.__atlas.ws.loaded",
            "--expr",
            "--timeout",
            "500",
        ]);
        assert_eq!(v["expr"], "window.__atlas.ws.loaded");
        assert!(v.get("selector").is_none());
        assert_eq!(v["timeout_ms"], 500);
    }

    #[test]
    fn click_type_a11y_perf_state_map_to_their_commands() {
        assert_eq!(
            req(&["click", "button.go"]),
            json!({"cmd":"click","selector":"button.go"})
        );
        assert_eq!(
            req(&["type", "input", "orb", "--delay", "20"]),
            json!({"cmd":"type","selector":"input","text":"orb","delay_ms":20})
        );
        assert_eq!(req(&["a11y"]), json!({"cmd":"a11y"}));
        assert_eq!(
            req(&["a11y", "nav"]),
            json!({"cmd":"a11y","selector":"nav"})
        );
        assert_eq!(req(&["contrast"]), json!({"cmd":"contrast"}));
        assert_eq!(
            req(&["contrast", "aside.sidebar"]),
            json!({"cmd":"contrast","selector":"aside.sidebar"})
        );
        assert_eq!(
            req(&["perf", "--seconds", "3"]),
            json!({"cmd":"perf","seconds":3})
        );
        assert_eq!(req(&["state"]), json!({"cmd":"state"}));
    }
}
