//! Plain `atlas doctor`: the first-run checks #265 asked for. Is `atlas` reachable from a new
//! terminal (the login shell's `PATH`, probed like the app does) and from this one, is Atlas
//! Desktop installed and on the CLI's version, does the terminal host answer. Each check says
//! what is wrong and the one command that fixes it. Facts are gathered once ([`Facts::gather`]);
//! [`evaluate`] is pure, so tests feed it a machine.

use std::path::{Path, PathBuf};

use atlas_i18n::t_in;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
    /// The command (or step) that fixes it; `None` when nothing is wrong.
    pub fix: Option<String>,
}

/// What the checks look at.
#[derive(Debug, Clone)]
pub struct Facts {
    /// `~/.atlas/bin`.
    pub install_dir: PathBuf,
    pub cli_version: String,
    /// `PATH` of a new terminal (the login shell's), `None` when the probe failed.
    pub login_path: Option<String>,
    /// Why the login-shell probe failed.
    pub login_error: Option<Why>,
    /// `PATH` of this process.
    pub process_path: Option<String>,
    /// The `atlas` that `PATH` resolves first in a new terminal (`None`: not found).
    pub login_atlas: Option<PathBuf>,
    /// Desktop app: path and its version, if installed.
    pub app: Option<(PathBuf, Option<String>)>,
    /// A graphical session (Linux) or always true on macOS/Windows.
    pub gui: bool,
    /// hostd answered the handshake; `Err` carries why not.
    pub hostd: Result<(), Why>,
}

/// Why a probe of the machine failed. The causes this binary words are messages; what the system said (an I/O
/// error, a refused connection) is shown as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    RegistryUnreadable,
    ProbeDidNotRun,
    ShellStart { shell: String, error: String },
    ShellSilent { shell: String, secs: u64 },
    ShellNoPath { shell: String },
    HostdSilent { secs: u64 },
    Other(String),
}

impl Why {
    fn text(&self, tag: &str) -> String {
        match self {
            Self::RegistryUnreadable => t_in!(tag, "cmd_misc_dc_why_registry"),
            Self::ProbeDidNotRun => t_in!(tag, "cmd_misc_dc_why_probe_not_run"),
            Self::ShellStart { shell, error } => {
                t_in!(
                    tag,
                    "cmd_misc_dc_why_start_failed",
                    shell = shell,
                    error = error
                )
            }
            Self::ShellSilent { shell, secs } => {
                t_in!(
                    tag,
                    "cmd_misc_dc_why_shell_silent",
                    shell = shell,
                    secs = *secs
                )
            }
            Self::ShellNoPath { shell } => {
                t_in!(tag, "cmd_misc_dc_why_shell_no_path", shell = shell)
            }
            Self::HostdSilent { secs } => {
                t_in!(tag, "cmd_misc_dc_why_hostd_silent", secs = *secs)
            }
            Self::Other(text) => text.clone(),
        }
    }
}

/// The installer line a machine without `atlas` on its PATH runs again.
#[cfg(not(windows))]
const INSTALL_COMMAND: &str = "curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh";
#[cfg(windows)]
const INSTALL_COMMAND: &str = "irm https://atlas.codeatlas.com.br/install.ps1 | iex";

/// Whether `found` (the `atlas` a new terminal runs) is this install: under `~/.atlas/bin`, or the
/// `~/.local/bin/atlas` link install makes. Windows compares like Windows: any case, `/` or `\`.
fn is_this_install(found: &Path, install_dir: &Path) -> bool {
    if cfg!(windows) {
        return super::doctor_winpath::is_under_win(
            &found.to_string_lossy(),
            &install_dir.to_string_lossy(),
        );
    }
    found.starts_with(install_dir) || found.ends_with(".local/bin/atlas")
}

fn on_path(path: Option<&str>, dir: &Path) -> bool {
    path.is_some_and(|p| std::env::split_paths(p).any(|e| e == dir))
}

/// The checks in English: what `--json` prints, whatever the display language.
pub fn evaluate(f: &Facts) -> Vec<Check> {
    evaluate_in("en", f)
}

/// The checks with `detail` and `fix` in `tag`; `name` is a stable id in every language.
pub fn evaluate_in(tag: &str, f: &Facts) -> Vec<Check> {
    let mut v = Vec::new();
    let dir = f.install_dir.display().to_string();

    // A new terminal: what users hit after the installer ("command not found", #265).
    v.push(match (&f.login_path, &f.login_atlas) {
        (None, _) => Check {
            name: "new terminal",
            status: Status::Warn,
            detail: t_in!(
                tag,
                "cmd_misc_dc_probe_failed",
                why = f
                    .login_error
                    .as_ref()
                    .map_or_else(|| t_in!(tag, "cmd_misc_dc_unknown_error"), |w| w.text(tag))
            ),
            fix: Some(t_in!(tag, "cmd_misc_dc_probe_failed_fix")),
        },
        // This install, directly or through the `~/.local/bin/atlas` link install makes.
        (Some(_), Some(found)) if is_this_install(found, &f.install_dir) => Check {
            name: "new terminal",
            status: Status::Ok,
            detail: t_in!(
                tag,
                "cmd_misc_dc_atlas_found",
                path = found.display().to_string()
            ),
            fix: None,
        },
        (Some(_), Some(found)) => Check {
            name: "new terminal",
            status: Status::Warn,
            detail: t_in!(
                tag,
                "cmd_misc_dc_atlas_elsewhere",
                found = found.display().to_string()
            ),
            fix: Some(t_in!(
                tag,
                "cmd_misc_dc_atlas_elsewhere_fix",
                dir = &dir,
                found = found.display().to_string()
            )),
        },
        (Some(_), None) => Check {
            name: "new terminal",
            status: Status::Fail,
            detail: t_in!(tag, "cmd_misc_dc_atlas_missing", dir = &dir),
            fix: Some(t_in!(
                tag,
                "cmd_misc_dc_atlas_missing_fix",
                command = INSTALL_COMMAND
            )),
        },
    });

    v.push(if on_path(f.process_path.as_deref(), &f.install_dir) {
        Check {
            name: "this terminal",
            status: Status::Ok,
            detail: t_in!(tag, "cmd_misc_dc_path_ok", dir = &dir),
            fix: None,
        }
    } else {
        Check {
            name: "this terminal",
            status: Status::Warn,
            detail: t_in!(tag, "cmd_misc_dc_path_missing", dir = &dir),
            fix: Some(format!("export PATH=\"{dir}:$PATH\"")),
        }
    });

    v.push(match &f.app {
        Some((p, Some(ver))) if *ver == f.cli_version => Check {
            name: "desktop app",
            status: Status::Ok,
            detail: t_in!(
                tag,
                "cmd_misc_dc_app_ok",
                path = p.display().to_string(),
                version = ver
            ),
            fix: None,
        },
        Some((p, ver)) => Check {
            name: "desktop app",
            status: Status::Warn,
            detail: t_in!(
                tag,
                "cmd_misc_dc_app_mismatch",
                path = p.display().to_string(),
                version = ver
                    .clone()
                    .unwrap_or_else(|| t_in!(tag, "cmd_misc_dc_unknown_version")),
                cli = &f.cli_version
            ),
            fix: Some("atlas update".into()),
        },
        None if f.gui => Check {
            name: "desktop app",
            status: Status::Warn,
            detail: t_in!(tag, "cmd_misc_dc_app_missing"),
            fix: Some("atlas desktop".into()),
        },
        None => Check {
            name: "desktop app",
            status: Status::Ok,
            detail: t_in!(tag, "cmd_misc_dc_app_none"),
            fix: None,
        },
    });

    v.push(match &f.hostd {
        Ok(()) => Check {
            name: "terminal host",
            status: Status::Ok,
            detail: t_in!(tag, "cmd_misc_dc_host_ok"),
            fix: None,
        },
        Err(why) => Check {
            name: "terminal host",
            status: Status::Warn,
            detail: t_in!(tag, "cmd_misc_dc_host_silent", why = why.text(tag)),
            fix: Some("atlas hostd".into()),
        },
    });
    v
}

/// The label of a check in `tag`; an id this build does not know is shown as it is.
fn label(tag: &str, name: &str) -> String {
    match name {
        "new terminal" => t_in!(tag, "cmd_misc_dc_name_new_terminal"),
        "this terminal" => t_in!(tag, "cmd_misc_dc_name_this_terminal"),
        "desktop app" => t_in!(tag, "cmd_misc_dc_name_desktop_app"),
        "terminal host" => t_in!(tag, "cmd_misc_dc_name_terminal_host"),
        other => other.to_string(),
    }
}

/// The text `atlas doctor` prints, in `tag`.
pub fn render(tag: &str, checks: &[Check]) -> String {
    let labels: Vec<String> = checks.iter().map(|c| label(tag, c.name)).collect();
    // The labels are translated, so the column is as wide as the longest one (never less than 14).
    let width = labels
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(14);
    let fix_label = t_in!(tag, "cmd_misc_dc_fix_label");
    let mut s = String::new();
    for (c, l) in checks.iter().zip(&labels) {
        let mark = match c.status {
            Status::Ok => "\x1b[32m✓\x1b[0m",
            Status::Warn => "\x1b[33m◆\x1b[0m",
            Status::Fail => "\x1b[31m✗\x1b[0m",
        };
        s.push_str(&format!("{mark} {l:<width$} {}\n", c.detail));
        if let Some(fix) = &c.fix {
            s.push_str(&format!("  {:<width$} {fix_label} {fix}\n", ""));
        }
    }
    s
}

impl Facts {
    pub async fn gather() -> Self {
        let install_dir = atlas_core::paths::atlas_home().join("bin");
        let probe = tokio::task::spawn_blocking(terminal_probe).await.ok();
        let (login_path, login_atlas, login_error) = match probe {
            Some(Ok((path, atlas))) => (Some(path), atlas, None),
            Some(Err(e)) => (None, None, Some(e)),
            None => (None, None, Some(Why::ProbeDidNotRun)),
        };
        let app = super::desktop::installed_app()
            .or_else(atlas_core::paths::packaged_app)
            .map(|a| {
                let v = super::desktop::app_version(&a);
                (a, v)
            });
        let gui = !cfg!(target_os = "linux")
            || std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var_os("DISPLAY").is_some();
        let hostd = match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            atlas_host::client::HostClient::connect_dir(&atlas_host::paths::base(), "doctor"),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(Why::Other(format!("{e:#}"))),
            Err(_) => Err(Why::HostdSilent { secs: 3 }),
        };
        Self {
            install_dir,
            cli_version: env!("CARGO_PKG_VERSION").into(),
            login_error,
            login_path,
            process_path: std::env::var("PATH").ok(),
            login_atlas,
            app,
            gui,
            hostd,
        }
    }
}

/// Start the account's shell the way a terminal emulator does (Linux: interactive, not login;
/// macOS: login, as Terminal.app) and ask it for its `PATH` and which `atlas` it runs. Not
/// `login_env`'s `-l -i` probe: that reads `~/.bash_profile`, which a Linux terminal never does,
/// and would call #265's broken install healthy. Returns `(PATH, atlas or None)`.
fn terminal_probe() -> Result<(String, Option<PathBuf>), Why> {
    if cfg!(windows) {
        // A new terminal there reads the registry's system and user Path, not a shell rc (#340).
        return super::doctor_winpath::terminal_view().ok_or(Why::RegistryUnreadable);
    }
    let (shell, _) = atlas_core::login_env::resolve_shell();
    let name = Path::new(&shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let script = match name {
        "nu" => {
            "print $'@@PATH ($env.PATH | str join (char esep))'; print $'@@ATLAS (which atlas | get -o 0.path | default \"\")'"
        }
        "fish" => "echo \"@@PATH \"(string join : $PATH); echo \"@@ATLAS \"(command -v atlas)",
        _ => "echo \"@@PATH $PATH\"; echo \"@@ATLAS $(command -v atlas)\"",
    };
    let flags: &[&str] = if cfg!(target_os = "macos") {
        &["-l", "-i", "-c"]
    } else {
        &["-i", "-c"]
    };
    let mut child = std::process::Command::new(&shell)
        .args(flags)
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Why::ShellStart {
            shell: shell.clone(),
            error: e.to_string(),
        })?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    loop {
        if child
            .try_wait()
            .map_err(|e| Why::Other(e.to_string()))?
            .is_some()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err(Why::ShellSilent {
                shell: shell.clone(),
                secs: 6,
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = child
        .wait_with_output()
        .map_err(|e| Why::Other(e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |k: &str| {
        text.lines()
            .rev()
            .find_map(|l| l.strip_prefix(k))
            .map(|s| s.trim().to_owned())
    };
    let path = field("@@PATH ").ok_or_else(|| Why::ShellNoPath {
        shell: shell.clone(),
    })?;
    let atlas = field("@@ATLAS ")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    Ok((path, atlas))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `PATH` value in this OS's separator (`:` here, `;` on Windows).
    fn p(dirs: &[&str]) -> String {
        std::env::join_paths(dirs).unwrap().into_string().unwrap()
    }

    fn healthy() -> Facts {
        let dir = PathBuf::from("/home/u/.atlas/bin");
        Facts {
            install_dir: dir.clone(),
            cli_version: "1.5.0".into(),
            login_path: Some(p(&["/home/u/.atlas/bin", "/usr/bin"])),
            login_atlas: Some(dir.join("atlas")),
            process_path: Some(p(&["/home/u/.atlas/bin", "/usr/bin"])),
            app: Some((
                PathBuf::from("/home/u/.local/bin/atlas-desktop"),
                Some("1.5.0".into()),
            )),
            gui: true,
            hostd: Ok(()),
            login_error: None,
        }
    }

    fn get<'a>(c: &'a [Check], name: &str) -> &'a Check {
        c.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn a_healthy_machine_has_no_fix_to_suggest() {
        let c = evaluate(&healthy());
        assert!(
            c.iter().all(|c| c.status == Status::Ok && c.fix.is_none()),
            "{c:#?}"
        );
    }

    /// #265: the installer said "Added to PATH" and a new terminal still said "command not found".
    #[test]
    fn a_new_terminal_without_atlas_fails_and_names_the_installer() {
        let mut f = healthy();
        f.login_path = Some("/usr/bin".into());
        f.login_atlas = None;
        let c = evaluate(&f);
        let t = get(&c, "new terminal");
        assert_eq!(t.status, Status::Fail);
        assert!(t.fix.as_deref().unwrap().contains(if cfg!(windows) {
            "install.ps1"
        } else {
            "install.sh"
        }));
    }

    #[test]
    fn another_atlas_first_on_path_is_named() {
        let mut f = healthy();
        f.login_path = Some(p(&["/usr/bin", "/home/u/.atlas/bin"]));
        f.login_atlas = Some(PathBuf::from("/usr/bin/atlas"));
        let t = evaluate(&f)
            .into_iter()
            .find(|c| c.name == "new terminal")
            .unwrap();
        assert_eq!(t.status, Status::Warn);
        assert!(t.fix.unwrap().contains("/usr/bin/atlas"));
    }

    /// #340, on Windows itself: the registry's `Path` spells the folder with another case and `/`
    /// than `atlas_home()` does, and the installer's copy must still count as this install.
    #[cfg(windows)]
    #[test]
    fn windows_compares_the_installers_copy_without_case_or_slash_direction() {
        let mut f = healthy();
        f.install_dir = PathBuf::from(r"C:\Users\Ana\.atlas\bin");
        f.login_atlas = Some(PathBuf::from("c:/users/ana/.ATLAS/bin/atlas.exe"));
        assert_eq!(get(&evaluate(&f), "new terminal").status, Status::Ok);
        f.login_atlas = Some(PathBuf::from(r"C:\Program Files\Git\cmd\atlas.exe"));
        assert_eq!(get(&evaluate(&f), "new terminal").status, Status::Warn);
        f.login_atlas = None;
        let t = evaluate(&f)
            .into_iter()
            .find(|c| c.name == "new terminal")
            .unwrap();
        assert!(
            t.fix.unwrap().contains("install.ps1"),
            "the fix names the PowerShell installer"
        );
    }

    /// `atlas install` makes the `~/.local/bin/atlas` link on Unix only.
    #[cfg(unix)]
    #[test]
    fn the_local_bin_link_counts_as_this_install() {
        let mut f = healthy();
        f.login_path = Some(p(&["/home/u/.local/bin", "/usr/bin"]));
        f.login_atlas = Some(PathBuf::from("/home/u/.local/bin/atlas"));
        assert_eq!(get(&evaluate(&f), "new terminal").status, Status::Ok);
    }

    #[test]
    fn desktop_missing_on_a_gui_or_out_of_date_suggests_the_command() {
        let mut f = healthy();
        f.app = None;
        assert_eq!(
            get(&evaluate(&f), "desktop app").fix.as_deref(),
            Some("atlas desktop")
        );
        f.gui = false;
        assert_eq!(get(&evaluate(&f), "desktop app").status, Status::Ok);
        f.app = Some((PathBuf::from("/x"), Some("1.4.0".into())));
        assert_eq!(
            get(&evaluate(&f), "desktop app").fix.as_deref(),
            Some("atlas update")
        );
    }

    #[test]
    fn a_failed_probe_or_silent_hostd_is_a_warning_not_a_failure() {
        let mut f = healthy();
        f.login_path = None;
        f.login_error = Some(Why::Other("timeout".into()));
        f.hostd = Err(Why::Other("connection refused".into()));
        let c = evaluate(&f);
        assert_eq!(get(&c, "new terminal").status, Status::Warn);
        assert_eq!(get(&c, "terminal host").status, Status::Warn);
        assert!(!c.iter().any(|c| c.status == Status::Fail));
    }

    #[test]
    fn the_causes_this_binary_words_are_messages_and_the_systems_text_is_kept() {
        let shell = || "/bin/zsh".to_string();
        for tag in atlas_i18n::locales() {
            for why in [
                Why::RegistryUnreadable,
                Why::ProbeDidNotRun,
                Why::ShellStart {
                    shell: shell(),
                    error: "No such file".into(),
                },
                Why::ShellSilent {
                    shell: shell(),
                    secs: 6,
                },
                Why::ShellNoPath { shell: shell() },
                Why::HostdSilent { secs: 3 },
            ] {
                let text = why.text(tag);
                assert!(!text.starts_with("cmd_misc_"), "{tag}: {why:?} -> {text}");
            }
        }
        assert_eq!(
            Why::Other("connection refused".into()).text("pt-BR"),
            "connection refused"
        );
        assert_eq!(Why::HostdSilent { secs: 3 }.text("en"), "no answer in 3 s");
    }

    #[test]
    fn json_is_english_and_a_check_keeps_its_id_in_every_language() {
        let mut f = healthy();
        f.login_atlas = None;
        let en = evaluate(&f);
        for tag in atlas_i18n::locales() {
            let c = evaluate_in(tag, &f);
            assert_eq!(
                c.iter().map(|c| (c.name, c.status)).collect::<Vec<_>>(),
                en.iter().map(|c| (c.name, c.status)).collect::<Vec<_>>()
            );
            // Every id has a label that is not a missing key.
            for check in &c {
                let l = label(tag, check.name);
                assert!(!l.is_empty() && !l.starts_with("cmd_misc_"), "{tag}: {l}");
            }
        }
        assert_eq!(
            en[0].detail,
            "a new terminal does not find `atlas` (/home/u/.atlas/bin is not on its PATH)"
        );
    }
}
