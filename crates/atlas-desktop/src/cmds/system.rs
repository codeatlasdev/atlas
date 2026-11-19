//! Hand things to the OS: the editor, the browser and the file manager. macOS: `open`. Linux: `xdg-open`, the
//! file manager's `ShowItems` D-Bus call and the first GUI editor found. Windows: VS Code's `code`, the default
//! browser and File Explorer.
//!
//! What to run is decided by pure functions of an [`Os`] ([`url_launch`], [`reveal_argv`], [`editor_argvs`]), so every
//! OS's command line is tested on any machine; [`run`] only starts it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::{REMOTE_UNSUPPORTED, root_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    /// Every unix that is not macOS speaks `xdg-open`.
    const fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

type Argv = Vec<OsString>;

fn av<'a>(parts: impl IntoIterator<Item = &'a OsStr>) -> Argv {
    parts.into_iter().map(OsStr::to_os_string).collect()
}

/// Windows spells paths with backslashes, and Explorer's `/select,` takes nothing else.
fn win(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\")
}

/// What a link is handed to the OS as. Windows never gets a command line: Explorer parses commas (`/select,`) and
/// other switches out of whatever string follows it, so a link would be able to open a second object.
#[derive(Debug, PartialEq)]
enum Launch {
    Argv(Argv),
    /// `ShellExecuteW("open", url)`: one string, no parsing of it.
    ShellExecute(String),
}

/// The only links that open in the browser: `http`/`https` with a host and no user info, and `mailto` with an
/// address. Never `file:` (it would open or run local things from a link in a terminal or a README), `smb:`,
/// `javascript:` and the like. Returns the URL as the parser re-serializes it (scheme lowercased, host as
/// punycode, characters escaped), which is what the OS gets: not the caller's string.
fn checked_url(raw: &str) -> Result<String, String> {
    let refuse = |why: &str| format!("refusing to open {raw}: {why}");
    if raw != raw.trim() || raw.chars().any(char::is_control) {
        return Err(refuse("it has spaces around it or control characters"));
    }
    let u = url::Url::parse(raw).map_err(|e| refuse(&e.to_string()))?;
    match u.scheme() {
        "http" | "https" => {
            // The parser forgives `https:/x`, `https:///x` and `https:\\x`; a link that is not spelled
            // `scheme://host` is not one we opened before and is not opened now.
            let authority = raw
                .split_once("://")
                .map(|(_, rest)| rest.split(['/', '?', '#', '\\']).next().unwrap_or(""))
                .ok_or_else(|| refuse("a web link is spelled scheme://host"))?;
            if authority.is_empty() || authority.contains('@') {
                return Err(refuse("a web link needs a host and carries no user name"));
            }
            if u.host_str().is_none_or(str::is_empty) {
                return Err(refuse("a web link needs a host"));
            }
        }
        "mailto" => {
            let to = u.path();
            if to.is_empty() || to.starts_with(',') || to.contains('\\') {
                return Err(refuse("a mail link needs an address"));
            }
        }
        _ => {
            return Err(refuse(
                "only http, https and mailto links open in the browser",
            ));
        }
    }
    Ok(u.into())
}

fn url_launch(os: Os, url: &str) -> Result<Launch, String> {
    let url = checked_url(url)?;
    Ok(match os {
        Os::Mac => Launch::Argv(av([OsStr::new("open"), OsStr::new(&url)])),
        Os::Linux => Launch::Argv(av([OsStr::new("xdg-open"), OsStr::new(&url)])),
        Os::Windows => Launch::ShellExecute(url),
    })
}

fn start(launch: Launch) -> Result<(), String> {
    match launch {
        Launch::Argv(argv) => run(&argv),
        #[cfg(windows)]
        Launch::ShellExecute(url) => {
            atlas_core::sys::shell_open(&url).map_err(|e| format!("could not open {url}: {e}"))
        }
        #[cfg(not(windows))]
        Launch::ShellExecute(_) => Err("ShellExecute exists on Windows only".into()),
    }
}

/// The command that shows `target` in the file manager: macOS and Windows select it; Linux has no portable
/// "select", so this is the fallback for when the file manager's `ShowItems` is unavailable: open the folder
/// that holds it.
fn reveal_argv(os: Os, target: &Path) -> Argv {
    match os {
        Os::Mac => av([OsStr::new("open"), OsStr::new("-R"), target.as_os_str()]),
        Os::Linux => {
            let dir = target.parent().unwrap_or(target);
            av([OsStr::new("xdg-open"), dir.as_os_str()])
        }
        Os::Windows => vec![
            "explorer.exe".into(),
            format!("/select,{}", win(target)).into(),
        ],
    }
}

/// Editors a Linux desktop starts as a window: tried in this order on `PATH` (`zeditor` is the only name Arch's
/// Zed package installs).
const LINUX_EDITORS: [&str; 4] = ["code", "codium", "zed", "zeditor"];

/// Programs `$VISUAL`/`$EDITOR` may name and still count as a window editor (no terminal to run vim in).
const GUI_EDITORS: [&str; 20] = [
    "code",
    "code-insiders",
    "codium",
    "vscodium",
    "cursor",
    "zed",
    "zeditor",
    "subl",
    "lapce",
    "gedit",
    "gnome-text-editor",
    "kate",
    "kwrite",
    "mousepad",
    "xed",
    "pluma",
    "gvim",
    "geany",
    "neovide",
    "nvim-qt",
];

/// The window editor to start on Linux: the first of [`LINUX_EDITORS`] on `PATH`, then `$VISUAL` and `$EDITOR`
/// when they name a window editor (their arguments, such as `--wait`, are dropped: the editor is detached).
fn linux_editor(
    which: &dyn Fn(&str) -> Option<PathBuf>,
    visual: Option<&str>,
    editor: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(p) = LINUX_EDITORS.iter().find_map(|e| which(e)) {
        return Ok(p);
    }
    let mut rejected = String::new();
    for (var, value) in [("VISUAL", visual), ("EDITOR", editor)] {
        let Some(program) = value.and_then(|v| v.split_whitespace().next()) else {
            continue;
        };
        let name = Path::new(program).file_name().and_then(OsStr::to_str);
        if name.is_some_and(|n| GUI_EDITORS.contains(&n)) {
            let found = if program.contains('/') {
                Some(PathBuf::from(program)).filter(|p| atlas_core::paths::is_executable(p))
            } else {
                which(program)
            };
            if let Some(p) = found {
                return Ok(p);
            }
        } else {
            rejected.push_str(&format!(" ${var}={program} is not a window editor."));
        }
    }
    Err(format!(
        "no editor to open the project in: install VS Code (`code`), VSCodium (`codium`) or Zed (`zed`) so it is on \
         your PATH, or set $VISUAL to a window editor.{rejected}"
    ))
}

/// The commands to try, in order, until one starts the editor on `root`.
fn editor_argvs(
    os: Os,
    root: &Path,
    which: &dyn Fn(&str) -> Option<PathBuf>,
    visual: Option<&str>,
    editor: Option<&str>,
) -> Result<Vec<Argv>, String> {
    Ok(match os {
        Os::Mac => vec![
            av([
                OsStr::new("open"),
                OsStr::new("-a"),
                OsStr::new("Visual Studio Code"),
                root.as_os_str(),
            ]),
            av([OsStr::new("open"), root.as_os_str()]),
        ],
        Os::Linux => {
            let program = linux_editor(which, visual, editor)?;
            vec![av([program.as_os_str(), root.as_os_str()])]
        }
        Os::Windows => {
            let r = win(root);
            match which("code") {
                Some(code) => vec![vec![code.into_os_string(), r.into()]],
                None => vec![vec!["explorer.exe".into(), r.into()]],
            }
        }
    })
}

/// How long a launcher gets to fail before it is taken to have worked: `xdg-open` can stay alive as long as the
/// browser it started, and a failure (no handler, a missing tool) comes back at once.
#[cfg(unix)]
const GRACE: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(unix)]
fn run(argv: &[OsString]) -> Result<(), String> {
    run_within(argv, GRACE)
}

/// Start `argv` detached (own session, no stdin: it never dies with the app nor blocks it) and report a start that
/// fails or an exit that fails within `grace`. The child is reaped by a thread whenever it ends.
#[cfg(unix)]
fn run_within(argv: &[OsString], grace: std::time::Duration) -> Result<(), String> {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let (program, args) = argv.split_first().ok_or("nothing to run")?;
    let name = Path::new(program)
        .file_name()
        .unwrap_or(program)
        .to_string_lossy()
        .into_owned();
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if Os::current() == Os::Linux {
        // The user's PATH and BROWSER: a desktop-entry launch has neither.
        atlas_core::login_env::apply(&mut cmd);
    }
    let mut child = atlas_core::sys::spawn_detached(&mut cmd).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{name} is not installed (or not on PATH)")
        } else {
            format!("{name}: {e}")
        }
    })?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    match rx.recv_timeout(grace) {
        Ok(Ok(st)) if st.success() => Ok(()),
        Ok(Ok(st)) => Err(exit_error(&name, st)),
        Ok(Err(e)) => Err(format!("{name}: {e}")),
        Err(_) => Ok(()),
    }
}

#[cfg(unix)]
fn exit_error(name: &str, st: std::process::ExitStatus) -> String {
    match (name, st.code()) {
        ("xdg-open", Some(2)) => "xdg-open: that file or folder does not exist".into(),
        ("xdg-open", Some(3)) => {
            "xdg-open: a tool it needs is missing (is a browser or file manager installed?)".into()
        }
        ("xdg-open", Some(4)) => {
            "xdg-open: no application is set to open this (set a default browser or file manager)"
                .into()
        }
        (_, Some(c)) => format!("{name} exited with status {c}"),
        (_, None) => format!("{name} was killed ({st})"),
    }
}

/// Explorer's exit code is 1 even on success: only a failed start is an error.
#[cfg(windows)]
fn run(argv: &[OsString]) -> Result<(), String> {
    use atlas_core::sys::CommandExt as _;
    let (program, args) = argv.split_first().ok_or("nothing to run")?;
    std::process::Command::new(program)
        .args(args)
        .no_window()
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", Path::new(program).display()))
}

/// The first of `argvs` that starts; the last one's error when none does.
fn run_first(argvs: &[Argv]) -> Result<(), String> {
    let mut last = Err("nothing to run".to_string());
    for a in argvs {
        last = run(a);
        if last.is_ok() {
            break;
        }
    }
    last
}

/// Ask the session's file manager (`org.freedesktop.FileManager1`: Nautilus, Dolphin, Nemo, Thunar, Caja, PCManFM-Qt)
/// to show `target` selected in its folder.
#[cfg(target_os = "linux")]
fn show_items(target: &Path) -> Result<(), String> {
    use gio::prelude::*;
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .map_err(|e| format!("no session bus: {e}"))?;
    let uri = gio::File::for_path(target).uri().to_string();
    bus.call_sync(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        "ShowItems",
        Some(&(vec![uri], "").to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        None::<&gio::Cancellable>,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// `ShowItems` first; when the file manager cannot be asked, open the containing folder with `xdg-open`.
#[cfg(target_os = "linux")]
fn reveal_linux(
    target: &Path,
    show: impl FnOnce(&Path) -> Result<(), String>,
    grace: std::time::Duration,
) -> Result<(), String> {
    let Err(why) = show(target) else {
        return Ok(());
    };
    run_within(&reveal_argv(Os::Linux, target), grace)
        .map_err(|e| format!("{e} (the file manager's D-Bus ShowItems failed too: {why})"))
}

fn reveal_local(target: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return reveal_linux(target, show_items, GRACE);
    #[cfg(not(target_os = "linux"))]
    run(&reveal_argv(Os::current(), target))
}

/// A launcher can wait for its child for seconds: never on the thread that serves the window.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// An environment variable as the user's shell sets it (a desktop launch has none of the rc files' exports).
fn user_env(name: &str) -> Option<String> {
    atlas_core::login_env::vars()
        .into_iter()
        .find_map(|(k, v)| (k == name).then_some(v))
        .or_else(|| std::env::var(name).ok())
        .filter(|v| !v.trim().is_empty())
}

#[tauri::command]
pub async fn open_in_editor(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    crate::native_ui::launch("open in the editor", &root.to_string_lossy())?;
    blocking(move || {
        let argvs = editor_argvs(
            Os::current(),
            &root,
            &atlas_core::login_env::which,
            user_env("VISUAL").as_deref(),
            user_env("EDITOR").as_deref(),
        )?;
        run_first(&argvs)
    })
    .await
}

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    let launch = url_launch(Os::current(), &url)?;
    crate::native_ui::launch("open URL", &url)?;
    blocking(move || start(launch)).await
}

/// Show a project's `atlas.yaml` (its folder on Linux when it has none) in the file manager.
#[tauri::command]
pub async fn reveal(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    crate::native_ui::launch("reveal in the file manager", &root.to_string_lossy())?;
    blocking(move || {
        let file = root.join("atlas.yaml");
        reveal_local(if cfg!(target_os = "linux") && !file.exists() {
            &root
        } else {
            &file
        })
    })
    .await
}

/// Show a file or folder of this machine in the file manager (the explorer's "Reveal"). A path on an SSH host has
/// no file manager here.
#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_absolute() {
        return Err(format!("{path} is not an absolute path"));
    }
    if crate::host::host_for_path(&path) != crate::host::LOCAL {
        return Err(REMOTE_UNSUPPORTED.into());
    }
    crate::native_ui::launch("reveal in the file manager", &path)?;
    blocking(move || {
        if !p.exists() {
            return Err(format!("{} does not exist", p.display()));
        }
        reveal_local(&p)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(a: &[&str]) -> Argv {
        a.iter().map(OsString::from).collect()
    }

    fn found(names: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
        move |n| {
            names
                .contains(&n)
                .then(|| PathBuf::from(format!("/opt/bin/{n}")))
        }
    }

    #[test]
    fn urls_open_with_each_oss_own_opener() {
        let u = "https://github.com/codeatlasdev/atlas/issues/305";
        assert_eq!(
            url_launch(Os::Mac, u).unwrap(),
            Launch::Argv(argv(&["open", u]))
        );
        assert_eq!(
            url_launch(Os::Linux, u).unwrap(),
            Launch::Argv(argv(&["xdg-open", u]))
        );
    }

    // Security review of #620: Explorer parses commas and switches out of a raw string (`/select,`), so on Windows a
    // link must never be a command line: it is ONE string for ShellExecuteW.
    #[test]
    fn windows_opens_a_link_with_shell_execute_never_with_explorer() {
        let u = "https://github.com/codeatlasdev/atlas/issues/305";
        assert_eq!(
            url_launch(Os::Windows, u).unwrap(),
            Launch::ShellExecute(u.into())
        );
        // a comma or `/select,` inside a valid link stays inside the one string
        for ok in ["mailto:a@b.c", "https://a.b/x,y?z=/select,C:"] {
            assert_eq!(
                url_launch(Os::Windows, ok).unwrap(),
                Launch::ShellExecute(checked_url(ok).unwrap()),
                "{ok}"
            );
        }
    }

    // Regression #305: on Linux every link, "Open in editor" and "Reveal" ran macOS's `open`, which Linux lacks.
    #[test]
    fn linux_never_runs_macos_open() {
        let os = Os::Linux;
        assert_eq!(
            url_launch(os, "http://a.b").unwrap(),
            Launch::Argv(argv(&["xdg-open", "http://a.b/"]))
        );
        assert_ne!(reveal_argv(os, Path::new("/a/b"))[0], "open");
        let ed = editor_argvs(os, Path::new("/a"), &found(&["code"]), None, None).unwrap();
        assert_ne!(ed[0][0], "open");
    }

    // Only web and mail links go to the OS opener, as the parser spells them: `file:` would open or run local things
    // from a link in a terminal or a README, `smb:` reaches a share, and the rest are script/handler schemes.
    #[test]
    fn only_http_https_and_mailto_reach_the_opener_as_the_parser_spells_them() {
        let spelled = [
            ("http://a.b/c", "http://a.b/c"),
            ("https://a.b", "https://a.b/"),
            ("HTTPS://A.B", "https://a.b/"),
            ("mailto:a@b.c", "mailto:a@b.c"),
            ("MAILTO:a@b.c?subject=x", "mailto:a@b.c?subject=x"),
            ("https://a.b/a b", "https://a.b/a%20b"),
            // an IDN host goes out as punycode, the form every opener and the Windows shell take
            ("https://bücher.de/x", "https://xn--bcher-kva.de/x"),
            ("https://a.b:8080/p?q=1#f", "https://a.b:8080/p?q=1#f"),
        ];
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            for (raw, want) in spelled {
                let got = checked_url(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
                assert_eq!(got, want, "{os:?} {raw}");
                assert!(url_launch(os, raw).is_ok(), "{os:?} {raw}");
            }
            for bad in [
                "file:///etc/passwd",
                "FILE:///etc/passwd",
                "file://host/share/x.exe",
                "smb://host/share",
                "\\\\host\\share\\x.exe",
                "javascript:alert(1)",
                "JaVaScRiPt:alert(1)",
                "data:text/html,<script>1</script>",
                "ssh://host",
                "ms-msdt:/id x",
                "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
                "/etc/passwd",
                "--version",
                "-https://a.b",
                "http:/a.b",
                "https",
                "",
                "é",
                // the review's payload: passes a scheme check, and Explorer would split it at the comma
                "mailto:a,\\\\attacker\\share\\x.exe",
                "mailto:,x",
                "mailto:",
                "mailto:?subject=x",
                // a web link with no host, or with a user name in front of it
                "https:///x",
                "https://@x",
                "https://user:pw@x",
                "https://x@evil.test",
                "https:\\\\x",
                "https://",
                "http://:80",
                // the parser strips these: a link must not smuggle one
                " https://a.b",
                "https://a.b ",
                "https://a.b/\n/select,x",
                "https://a\tb",
                // forbidden host characters, and an IDN that does not normalize
                "https://a b",
                "https://a%2fb",
                "https://xn--/",
            ] {
                let e = checked_url(bad).unwrap_err();
                assert!(e.contains("refusing to open"), "{bad}: {e}");
                assert!(url_launch(os, bad).is_err(), "{os:?} {bad}");
            }
        }
    }

    #[test]
    fn reveal_argv_per_os() {
        let p = Path::new("/home/me/proj/atlas.yaml");
        assert_eq!(
            reveal_argv(Os::Mac, p),
            argv(&["open", "-R", "/home/me/proj/atlas.yaml"])
        );
        // No portable "select" on Linux: the fallback opens the folder that holds it.
        assert_eq!(
            reveal_argv(Os::Linux, p),
            argv(&["xdg-open", "/home/me/proj"])
        );
        assert_eq!(
            reveal_argv(Os::Windows, Path::new("C:/p/atlas.yaml")),
            argv(&["explorer.exe", "/select,C:\\p\\atlas.yaml"])
        );
    }

    #[test]
    fn linux_reveal_of_the_filesystem_root_opens_the_root() {
        assert_eq!(
            reveal_argv(Os::Linux, Path::new("/")),
            argv(&["xdg-open", "/"])
        );
    }

    #[test]
    fn mac_editor_tries_vs_code_then_whatever_opens_the_folder() {
        assert_eq!(
            editor_argvs(Os::Mac, Path::new("/p"), &found(&[]), None, None).unwrap(),
            vec![
                argv(&["open", "-a", "Visual Studio Code", "/p"]),
                argv(&["open", "/p"])
            ]
        );
    }

    #[test]
    fn windows_editor_is_code_or_explorer() {
        assert_eq!(
            editor_argvs(
                Os::Windows,
                Path::new("C:/p"),
                &found(&["code"]),
                None,
                None
            )
            .unwrap(),
            vec![argv(&["/opt/bin/code", "C:\\p"])]
        );
        assert_eq!(
            editor_argvs(Os::Windows, Path::new("C:/p"), &found(&[]), None, None).unwrap(),
            vec![argv(&["explorer.exe", "C:\\p"])]
        );
    }

    #[test]
    fn linux_editor_is_the_first_of_code_codium_zed() {
        let all = found(&["zed", "codium", "code"]);
        assert_eq!(
            linux_editor(&all, None, None).unwrap(),
            PathBuf::from("/opt/bin/code")
        );
        assert_eq!(
            linux_editor(&found(&["zed", "codium"]), None, None).unwrap(),
            PathBuf::from("/opt/bin/codium")
        );
        assert_eq!(
            linux_editor(&found(&["zed"]), None, None).unwrap(),
            PathBuf::from("/opt/bin/zed")
        );
        // Arch's pacman Zed installs only /usr/bin/zeditor.
        assert_eq!(
            linux_editor(&found(&["zeditor"]), None, None).unwrap(),
            PathBuf::from("/opt/bin/zeditor")
        );
        assert_eq!(
            linux_editor(&found(&["zeditor", "zed"]), None, None).unwrap(),
            PathBuf::from("/opt/bin/zed")
        );
        assert_eq!(
            editor_argvs(Os::Linux, Path::new("/p"), &all, None, None).unwrap(),
            vec![argv(&["/opt/bin/code", "/p"])]
        );
    }

    #[test]
    fn linux_editor_falls_back_to_a_window_editor_from_visual_then_editor() {
        let w = found(&["gedit", "kate"]);
        // A window editor in $VISUAL wins over $EDITOR; the arguments are dropped (the editor is detached).
        assert_eq!(
            linux_editor(&w, Some("kate --new"), Some("gedit")).unwrap(),
            PathBuf::from("/opt/bin/kate")
        );
        // A terminal editor in $VISUAL does not hide a window editor in $EDITOR.
        assert_eq!(
            linux_editor(&w, Some("vim"), Some("gedit -w")).unwrap(),
            PathBuf::from("/opt/bin/gedit")
        );
        // The GUI editors on PATH rank above the environment.
        assert_eq!(
            linux_editor(&found(&["zed", "kate"]), Some("kate"), None).unwrap(),
            PathBuf::from("/opt/bin/zed")
        );
    }

    #[test]
    fn linux_editor_says_why_when_there_is_no_window_editor() {
        let e = linux_editor(&found(&[]), Some("vim"), Some("nano -w")).unwrap_err();
        assert!(e.contains("`code`") && e.contains("`zed`"), "{e}");
        assert!(
            e.contains("$VISUAL=vim is not a window editor")
                && e.contains("$EDITOR=nano is not a window editor"),
            "{e}"
        );
        // A window editor that is named but not installed is no editor either.
        assert!(linux_editor(&found(&[]), Some("kate"), None).is_err());
        // The terminal front ends of vim are window editors; plain vim and nvim are not.
        let all = found(&["gvim", "geany", "neovide", "nvim-qt", "nvim"]);
        for gui in ["gvim", "geany", "neovide", "nvim-qt"] {
            assert!(linux_editor(&all, Some(gui), None).is_ok(), "{gui}");
        }
        assert!(linux_editor(&all, Some("nvim"), None).is_err());
        assert!(linux_editor(&found(&[]), Some("   "), Some("")).is_err());
        assert!(
            editor_argvs(Os::Linux, Path::new("/p"), &found(&[]), None, None).is_err(),
            "no editor must be an error, not a silent no-op"
        );
    }

    #[test]
    fn linux_editor_accepts_a_full_path_only_when_it_is_executable() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("zeditor");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let program = bin.to_string_lossy().into_owned();
        assert!(linux_editor(&found(&[]), Some(&program), None).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(
                linux_editor(&found(&[]), Some(&program), None).unwrap(),
                bin
            );
        }
    }
}

/// Real launches against fake `xdg-open` scripts on the login PATH: the argv that really arrives, and what comes
/// back when the launcher fails, is missing or outlives us.
#[cfg(all(test, target_os = "linux"))]
mod launch_tests {
    use atlas_core::login_env::{Installed, Probe};
    use atlas_core::scaling::HANG;

    use super::*;

    /// A temp dir of fake programs that is the whole `PATH`, installed as the login probe.
    struct Fake {
        dir: tempfile::TempDir,
        _probe: Installed,
    }

    fn fake() -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let probe = Probe {
            shell: "sh".into(),
            source: "default",
            flags: None,
            error: None,
            took_ms: 0,
            vars: vec![("PATH".into(), dir.path().to_string_lossy().into_owned())],
        };
        Fake {
            _probe: atlas_core::login_env::install(probe),
            dir,
        }
    }

    impl Fake {
        /// A program `name` that appends its argv (one per line) to `calls` and then runs `tail`.
        fn program(&self, name: &str, tail: &str) {
            use std::os::unix::fs::PermissionsExt;
            let p = self.dir.path().join(name);
            let log = self.calls();
            std::fs::write(
                &p,
                format!(
                    "#!/bin/sh\nfor a in \"$@\"; do echo \"{name} $a\" >> '{}'; done\n{tail}\n",
                    log.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn calls(&self) -> PathBuf {
            self.dir.path().join("calls.log")
        }

        fn recorded(&self) -> Vec<String> {
            std::fs::read_to_string(self.calls())
                .unwrap_or_default()
                .lines()
                .map(String::from)
                .collect()
        }
    }

    // How long a launcher gets to fail: a ceiling, so a slow runner does not turn a launcher that exits into one
    // that "outlived" it.
    const WAIT: std::time::Duration = HANG;

    fn link_argv(url: &str) -> Argv {
        match url_launch(Os::current(), url).unwrap() {
            Launch::Argv(a) => a,
            other => panic!("a unix link is a command line, not {other:?}"),
        }
    }

    #[test]
    fn a_link_runs_xdg_open_with_the_url_as_one_argument() {
        let f = fake();
        f.program("xdg-open", "exit 0");
        let url = "https://example.com/a b?x=1&y=2";
        run_within(&link_argv(url), WAIT).unwrap();
        // what the OS gets is the parser's spelling of the link, as one argument
        assert_eq!(
            f.recorded(),
            vec!["xdg-open https://example.com/a%20b?x=1&y=2"]
        );
    }

    #[test]
    fn xdg_open_failing_is_an_error_that_names_the_cause() {
        let f = fake();
        f.program("xdg-open", "exit 4");
        let e = run_within(&link_argv("https://a.b"), WAIT).unwrap_err();
        assert!(e.contains("no application is set"), "{e}");
        f.program("xdg-open", "exit 9");
        let e = run_within(&link_argv("https://a.b"), WAIT).unwrap_err();
        assert!(e.contains("xdg-open exited with status 9"), "{e}");
    }

    #[test]
    fn a_missing_launcher_says_it_is_not_installed() {
        let _f = fake();
        let e = run_within(&link_argv("https://a.b"), WAIT).unwrap_err();
        assert!(e.contains("xdg-open is not installed"), "{e}");
    }

    #[test]
    fn reveal_falls_back_to_the_folder_when_the_file_manager_cannot_be_asked() {
        let f = fake();
        f.program("xdg-open", "exit 0");
        reveal_linux(
            Path::new("/home/me/proj/src/main.rs"),
            |_| Err("no session bus".into()),
            WAIT,
        )
        .unwrap();
        assert_eq!(f.recorded(), vec!["xdg-open /home/me/proj/src"]);
    }

    #[test]
    fn reveal_through_the_file_manager_does_not_open_a_second_window() {
        let f = fake();
        f.program("xdg-open", "exit 0");
        let seen = std::cell::RefCell::new(None);
        reveal_linux(
            Path::new("/home/me/a.txt"),
            |p| {
                *seen.borrow_mut() = Some(p.to_path_buf());
                Ok(())
            },
            WAIT,
        )
        .unwrap();
        assert_eq!(seen.into_inner(), Some(PathBuf::from("/home/me/a.txt")));
        assert!(f.recorded().is_empty());
    }

    #[test]
    fn reveal_reports_both_failures() {
        let _f = fake();
        let e =
            reveal_linux(Path::new("/a/b"), |_| Err("no session bus".into()), WAIT).unwrap_err();
        assert!(
            e.contains("xdg-open is not installed") && e.contains("no session bus"),
            "{e}"
        );
    }

    #[test]
    fn the_editor_is_started_detached_with_the_folder() {
        let f = fake();
        f.program("code", "exit 0");
        let argvs = editor_argvs(
            Os::current(),
            Path::new("/p/my app"),
            &atlas_core::login_env::which,
            None,
            None,
        )
        .unwrap();
        run_first(&argvs).unwrap();
        assert_eq!(f.recorded(), vec!["code /p/my app"]);
    }
}

/// `run_within` against a real child: what comes back while the launcher is still running.
#[cfg(all(test, unix))]
mod detached_tests {
    use std::process::Command;
    use std::time::Duration;

    use atlas_core::scaling::HANG;

    use super::*;

    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .output()
            .is_ok_and(|o| o.status.success())
    }

    // xdg-open stays alive while the browser it started does: that is success, and it must not block the app
    // (#417). The launcher here runs for longer than the hang ceiling and tells its pid; the call must have returned
    // while that process is still running (a call that waited for the launcher would return only once it was gone),
    // so the guard is the child's state at the return, not a stopwatch.
    #[test]
    fn a_launcher_that_outlives_the_grace_is_success_without_blocking() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("launcher");
        let pid_file = dir.path().join("pid");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho $$ > '{0}.tmp' && mv '{0}.tmp' '{0}'\nexec /bin/sleep {1}\n",
                pid_file.display(),
                HANG.as_secs() + 30
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        run_within(&[script.into_os_string()], Duration::from_millis(200)).unwrap();

        // The script may not have started yet on a loaded machine: wait for its pid (a real event).
        let end = std::time::Instant::now() + HANG;
        let pid = loop {
            if let Ok(p) = std::fs::read_to_string(&pid_file) {
                break p.trim().to_string();
            }
            assert!(
                std::time::Instant::now() < end,
                "the launcher never started"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let still_running = alive(&pid);
        let _ = Command::new("kill").arg(&pid).output();
        assert!(
            still_running,
            "the call returned only after the launcher was gone: it blocked on it"
        );
    }
}
