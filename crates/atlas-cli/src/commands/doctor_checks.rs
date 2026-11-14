//! Plain `atlas doctor`: the first-run checks #265 asked for. Is `atlas` reachable from a new
//! terminal (the login shell's `PATH`, probed like the app does) and from this one, is Atlas
//! Desktop installed and on the CLI's version, does the terminal host answer. Each check says
//! what is wrong and the one command that fixes it. Facts are gathered once ([`Facts::gather`]);
//! [`evaluate`] is pure, so tests feed it a machine.

use std::path::{Path, PathBuf};

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
    pub login_error: Option<String>,
    /// `PATH` of this process.
    pub process_path: Option<String>,
    /// The `atlas` that `PATH` resolves first in a new terminal (`None`: not found).
    pub login_atlas: Option<PathBuf>,
    /// Desktop app: path and its version, if installed.
    pub app: Option<(PathBuf, Option<String>)>,
    /// A graphical session (Linux) or always true on macOS/Windows.
    pub gui: bool,
    /// hostd answered the handshake; `Err` carries why not.
    pub hostd: Result<(), String>,
}

fn on_path(path: Option<&str>, dir: &Path) -> bool {
    path.is_some_and(|p| std::env::split_paths(p).any(|e| e == dir))
}

pub fn evaluate(f: &Facts) -> Vec<Check> {
    let mut v = Vec::new();
    let dir = f.install_dir.display();

    // A new terminal: what users hit after the installer ("command not found", #265).
    v.push(match (&f.login_path, &f.login_atlas) {
        (None, _) => Check {
            name: "new terminal",
            status: Status::Warn,
            detail: format!(
                "could not ask your login shell for its PATH ({})",
                f.login_error.as_deref().unwrap_or("unknown error")
            ),
            fix: Some("open a new terminal and run: command -v atlas".into()),
        },
        // This install, directly or through the `~/.local/bin/atlas` link install makes.
        (Some(_), Some(found))
            if found.starts_with(&f.install_dir) || found.ends_with(".local/bin/atlas") =>
        {
            Check {
                name: "new terminal",
                status: Status::Ok,
                detail: format!("`atlas` is {}", found.display()),
                fix: None,
            }
        }
        (Some(_), Some(found)) => Check {
            name: "new terminal",
            status: Status::Warn,
            detail: format!("`atlas` resolves to {}, not this install", found.display()),
            fix: Some(format!(
                "put {dir} first on PATH, or remove {}",
                found.display()
            )),
        },
        (Some(_), None) => Check {
            name: "new terminal",
            status: Status::Fail,
            detail: format!("a new terminal does not find `atlas` ({dir} is not on its PATH)"),
            fix: Some(
                "re-run the installer: curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh"
                    .into(),
            ),
        },
    });

    v.push(if on_path(f.process_path.as_deref(), &f.install_dir) {
        Check {
            name: "this terminal",
            status: Status::Ok,
            detail: format!("{dir} is on PATH"),
            fix: None,
        }
    } else {
        Check {
            name: "this terminal",
            status: Status::Warn,
            detail: format!("{dir} is not on this shell's PATH (a new terminal may have it)"),
            fix: Some(format!("export PATH=\"{dir}:$PATH\"")),
        }
    });

    v.push(match &f.app {
        Some((p, Some(ver))) if *ver == f.cli_version => Check {
            name: "desktop app",
            status: Status::Ok,
            detail: format!("{} ({ver})", p.display()),
            fix: None,
        },
        Some((p, ver)) => Check {
            name: "desktop app",
            status: Status::Warn,
            detail: format!(
                "{} is {}, the CLI is {}",
                p.display(),
                ver.as_deref().unwrap_or("an unknown version"),
                f.cli_version
            ),
            fix: Some("atlas update".into()),
        },
        None if f.gui => Check {
            name: "desktop app",
            status: Status::Warn,
            detail: "Atlas Desktop is not installed".into(),
            fix: Some("atlas desktop".into()),
        },
        None => Check {
            name: "desktop app",
            status: Status::Ok,
            detail: "not installed (no graphical session)".into(),
            fix: None,
        },
    });

    v.push(match &f.hostd {
        Ok(()) => Check {
            name: "terminal host",
            status: Status::Ok,
            detail: "answering".into(),
            fix: None,
        },
        Err(e) => Check {
            name: "terminal host",
            status: Status::Warn,
            detail: format!(
                "not answering ({e}); Atlas Desktop and `atlas dev` start it on demand"
            ),
            fix: Some("atlas hostd".into()),
        },
    });
    v
}

/// The text `atlas doctor` prints.
pub fn render(checks: &[Check]) -> String {
    let mut s = String::new();
    for c in checks {
        let mark = match c.status {
            Status::Ok => "\x1b[32m✓\x1b[0m",
            Status::Warn => "\x1b[33m◆\x1b[0m",
            Status::Fail => "\x1b[31m✗\x1b[0m",
        };
        s.push_str(&format!("{mark} {:<14} {}\n", c.name, c.detail));
        if let Some(fix) = &c.fix {
            s.push_str(&format!("  {:<14} fix: {fix}\n", ""));
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
            None => (None, None, Some("probe did not run".into())),
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
            Ok(Err(e)) => Err(format!("{e:#}")),
            Err(_) => Err("no answer in 3 s".into()),
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
fn terminal_probe() -> Result<(String, Option<PathBuf>), String> {
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
        .map_err(|e| format!("cannot start {shell}: {e}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err(format!("{shell} did not answer in 6 s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |k: &str| {
        text.lines()
            .rev()
            .find_map(|l| l.strip_prefix(k))
            .map(|s| s.trim().to_owned())
    };
    let path = field("@@PATH ").ok_or_else(|| format!("{shell} printed no PATH"))?;
    let atlas = field("@@ATLAS ")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    Ok((path, atlas))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Facts {
        let dir = PathBuf::from("/home/u/.atlas/bin");
        Facts {
            install_dir: dir.clone(),
            cli_version: "1.5.0".into(),
            login_path: Some("/home/u/.atlas/bin:/usr/bin".into()),
            login_atlas: Some(dir.join("atlas")),
            process_path: Some("/home/u/.atlas/bin:/usr/bin".into()),
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
        assert!(t.fix.as_deref().unwrap().contains("install.sh"));
    }

    #[test]
    fn another_atlas_first_on_path_is_named() {
        let mut f = healthy();
        f.login_path = Some("/usr/bin:/home/u/.atlas/bin".into());
        f.login_atlas = Some(PathBuf::from("/usr/bin/atlas"));
        let t = evaluate(&f)
            .into_iter()
            .find(|c| c.name == "new terminal")
            .unwrap();
        assert_eq!(t.status, Status::Warn);
        assert!(t.fix.unwrap().contains("/usr/bin/atlas"));
    }

    #[test]
    fn the_local_bin_link_counts_as_this_install() {
        let mut f = healthy();
        f.login_path = Some("/home/u/.local/bin:/usr/bin".into());
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
        f.login_error = Some("timeout".into());
        f.hostd = Err("connection refused".into());
        let c = evaluate(&f);
        assert_eq!(get(&c, "new terminal").status, Status::Warn);
        assert_eq!(get(&c, "terminal host").status, Status::Warn);
        assert!(!c.iter().any(|c| c.status == Status::Fail));
    }
}
