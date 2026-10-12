//! Open a URL or a `file:line[:col]` from a log line without blocking the
//! event loop: every launcher is spawned and reaped on a background task.
//!
//! Files go to the first editor that applies, in this order: `$VISUAL`,
//! `$EDITOR` (terminal editors would fight the TUI for the screen, so only
//! GUI-capable ones are used directly — see `gui_editor_args`), then `code -g`,
//! `zed`, `cursor -g` found on `PATH`; the caller falls back to copying.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// How to ask a known GUI editor for `file:line:col`. `None` = unknown/terminal
/// editor (not launched from inside the TUI).
fn gui_editor_args(editor: &str, path: &str, line: u32, col: Option<u32>) -> Option<Vec<String>> {
    let name = Path::new(editor.split_whitespace().next()?)
        .file_name()?
        .to_str()?
        .to_ascii_lowercase();
    let loc = match col {
        Some(c) => format!("{path}:{line}:{c}"),
        None => format!("{path}:{line}"),
    };
    Some(match name.as_str() {
        "code" | "code-insiders" | "cursor" | "windsurf" | "codium" => {
            vec!["-g".into(), loc]
        }
        // Zed and Sublime take `file:line:col` directly.
        "zed" | "subl" => vec![loc],
        // JetBrains launchers: `--line N file`.
        "idea" | "webstorm" | "goland" | "pycharm" | "rustrover" | "clion" => {
            vec!["--line".into(), line.to_string(), path.to_string()]
        }
        _ => return None,
    })
}

/// Command + args that open `path` at `line`, from the environment and `PATH`.
pub fn editor_command(
    path: &str,
    line: u32,
    col: Option<u32>,
    env: impl Fn(&str) -> Option<String>,
    on_path: impl Fn(&str) -> bool,
) -> Option<(String, Vec<String>)> {
    for var in ["VISUAL", "EDITOR"] {
        if let Some(ed) = env(var).filter(|e| !e.trim().is_empty())
            && let Some(args) = gui_editor_args(&ed, path, line, col)
        {
            let bin = ed.split_whitespace().next()?.to_string();
            return Some((bin, args));
        }
    }
    for bin in ["code", "zed", "cursor"] {
        if on_path(bin) {
            let args = gui_editor_args(bin, path, line, col)?;
            return Some((bin.to_string(), args));
        }
    }
    None
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// Spawn `cmd` detached from the TUI's terminal and reap it in the background.
fn spawn_reaped(mut cmd: Command) -> bool {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            true
        }
        Err(_) => false,
    }
}

/// Open `url` in the system browser. Returns whether a launcher started.
pub fn open_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    let mut cmd = Command::new(opener);
    cmd.arg(url);
    spawn_reaped(cmd)
}

/// Open `path` at `line[:col]` in an editor. Returns whether one started.
pub fn open_path(path: &str, line: u32, col: Option<u32>) -> bool {
    let Some((bin, args)) = editor_command(path, line, col, |k| std::env::var(k).ok(), on_path)
    else {
        return false;
    };
    let mut cmd = Command::new(bin);
    cmd.args(args);
    spawn_reaped(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn visual_beats_editor_and_gui_editors_get_line_args() {
        let e = env(&[("VISUAL", "/usr/local/bin/code"), ("EDITOR", "zed")]);
        let (bin, args) = editor_command("/p/a.rs", 3, Some(9), e, |_| false).unwrap();
        assert_eq!(bin, "/usr/local/bin/code");
        assert_eq!(args, ["-g", "/p/a.rs:3:9"]);
        let (bin, args) =
            editor_command("/p/a.rs", 3, None, env(&[("EDITOR", "zed")]), |_| false).unwrap();
        assert_eq!((bin.as_str(), args), ("zed", vec!["/p/a.rs:3".to_string()]));
        let (_, args) =
            editor_command("/p/a.kt", 7, None, env(&[("VISUAL", "idea")]), |_| false).unwrap();
        assert_eq!(args, ["--line", "7", "/p/a.kt"]);
    }

    #[test]
    fn terminal_editors_are_skipped_and_path_fallbacks_apply() {
        // vim would take over the TUI's screen: skip it, use what is on PATH.
        let e = env(&[("EDITOR", "vim")]);
        let (bin, args) = editor_command("/p/a.rs", 1, None, e, |b| b == "zed").unwrap();
        assert_eq!((bin.as_str(), args), ("zed", vec!["/p/a.rs:1".to_string()]));
        // Nothing usable: the caller copies instead.
        assert!(editor_command("/p/a.rs", 1, None, env(&[("EDITOR", "vim")]), |_| false).is_none());
        assert!(editor_command("/p/a.rs", 1, None, env(&[]), |_| false).is_none());
    }
}
