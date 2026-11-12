use anyhow::{Context, Result};
use atlas_core::util::expand_tilde;
use clap::Args;
use std::path::{Path, PathBuf};

use super::AtlasCommand;
use crate::commands::skills;

const INSTALL_DIR: &str = ".atlas/bin";

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(long)]
    pub dir: Option<String>,
    /// Only bring an existing `~/.atlas/bin/atlas` up to this binary's version: replace it when it
    /// is older or unreadable, leave a newer one and a dev-build link alone. No PATH or skills
    /// changes, nothing printed but one line. What the Windows installer runs after every update.
    #[arg(long, conflicts_with = "dir")]
    pub upgrade: bool,
}

impl AtlasCommand for InstallArgs {
    async fn run(self) -> Result<()> {
        if self.upgrade {
            return upgrade_existing();
        }
        handle(self.dir).await
    }
}

/// What `install --upgrade` decides about the installed CLI.
#[derive(Debug, PartialEq, Eq)]
enum Upgrade {
    /// Nothing is installed: `atlas install` is the first-install path, not this one.
    Absent,
    /// A symlink: `atlas link` (a dev build).
    Linked,
    /// Same or newer than this binary.
    Current(String),
    /// Older, or no readable version: replace it.
    Replace(String),
}

/// Judge the CLI at `dest` against `own` (this binary's version). `installed` runs `dest` and
/// returns the version it reports (`None` when it cannot).
fn judge_upgrade(
    dest: &Path,
    own: &str,
    installed: impl FnOnce(&Path) -> Option<String>,
) -> Upgrade {
    let Ok(meta) = std::fs::symlink_metadata(dest) else {
        return Upgrade::Absent;
    };
    if meta.file_type().is_symlink() {
        return Upgrade::Linked;
    }
    let Some(v) = installed(dest) else {
        return Upgrade::Replace("unreadable".into());
    };
    use atlas_core::channel;
    match (channel::parse_tag(&v), channel::parse_tag(own)) {
        (Some(have), Some(own)) if channel::precedence(&have, &own).is_ge() => Upgrade::Current(v),
        _ => Upgrade::Replace(v),
    }
}

/// `<exe> version` first line: `atlas 1.5.0-alpha.24`, within 5 s (a wedged binary must not hang
/// an installer).
fn installed_version(exe: &Path) -> Option<String> {
    use atlas_core::sys::CommandExt as _;
    let mut child = std::process::Command::new(exe)
        .arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .no_window()
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    out.lines()
        .next()?
        .strip_prefix("atlas ")
        .map(|v| v.trim().to_string())
}

/// `atlas install --upgrade`: see [`InstallArgs::upgrade`]. Never fails because the existing CLI
/// is odd: an error is only a copy that could not be written.
fn upgrade_existing() -> Result<()> {
    let dest = dirs::home_dir()
        .context("no home directory")?
        .join(INSTALL_DIR)
        .join(atlas_core::paths::exe("atlas"));
    let own = env!("CARGO_PKG_VERSION");
    match judge_upgrade(&dest, own, installed_version) {
        Upgrade::Absent => println!(
            "atlas is not installed in {}; run `atlas install`",
            dest.display()
        ),
        Upgrade::Linked => println!("{} is linked to a dev build; left alone", dest.display()),
        Upgrade::Current(v) => println!("atlas {v} in {} is current", dest.display()),
        Upgrade::Replace(old) => {
            let me =
                std::env::current_exe().context("Could not determine current executable path")?;
            // Running the installed copy itself (`~/.atlas/bin/atlas install --upgrade`): it is
            // the one being judged, so a newer self is simply not there to copy from.
            if me == dest {
                println!(
                    "atlas {old} in {} is this binary; nothing to copy",
                    dest.display()
                );
                return Ok(());
            }
            replace_binary(&me, &dest)?;
            println!("atlas {old} → {own} in {}", dest.display());
        }
    }
    Ok(())
}

/// Copy `src` over `dest` through a temp file in the same folder. Windows: a running `atlas.exe`
/// (hostd, a dev session, an MCP server another tool spawned) cannot be overwritten or deleted,
/// only renamed, so the old file is moved aside first; the next install sweeps the leftovers.
fn replace_binary(src: &Path, dest: &Path) -> Result<()> {
    let dir = dest.parent().context("install path has no folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let tmp = dir.join(".atlas-install-tmp");
    std::fs::copy(src, &tmp)
        .with_context(|| format!("Could not copy binary to {}", tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }

    #[cfg(windows)]
    if dest.exists() {
        sweep_old(dir);
        let _ = std::fs::rename(
            dest,
            dir.join(format!(".atlas-old-{}.exe", std::process::id())),
        );
    }
    std::fs::rename(&tmp, dest).with_context(|| format!("Could not install to {}", dest.display()))
}

/// Windows: delete the `.atlas-old-<pid>.exe` files earlier installs moved aside; one still in use
/// refuses and waits for the next run.
#[cfg(windows)]
fn sweep_old(dir: &Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(".atlas-old-") && name.ends_with(".exe") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub async fn handle(dir: Option<String>) -> Result<()> {
    let install_dir = match dir {
        Some(d) => PathBuf::from(expand_tilde(&d)),
        None => dirs::home_dir()
            .context("no home directory")?
            .join(INSTALL_DIR),
    };

    println!("\x1b[1;34m◆\x1b[0m Atlas installer");
    println!("  Version:  v{}", env!("CARGO_PKG_VERSION"));
    println!("  Target:   {}", install_dir.display());
    println!();

    let current_exe =
        std::env::current_exe().context("Could not determine current executable path")?;
    let dest = install_dir.join(atlas_core::paths::exe("atlas"));
    replace_binary(&current_exe, &dest)?;

    println!("\x1b[32m✓\x1b[0m Installed → {}", dest.display());

    let already_in_path = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|p| p == install_dir));

    if already_in_path {
        println!("\x1b[32m✓\x1b[0m Already in PATH");
    } else if cfg!(windows) {
        #[cfg(windows)]
        match add_user_path(&install_dir) {
            Ok(_) => println!("\x1b[32m✓\x1b[0m Added to PATH  [user environment]"),
            Err(e) => println!(
                "\x1b[33m◆\x1b[0m Add {} to your PATH manually ({e:#})",
                install_dir.display()
            ),
        }
    } else {
        let written = setup_path_all_shells(&install_dir);
        if written.is_empty() {
            println!(
                "\x1b[33m◆\x1b[0m Add to PATH manually:\n  export PATH=\"{}:$PATH\"",
                install_dir.display()
            );
        } else {
            for shell in &written {
                println!("\x1b[32m✓\x1b[0m Added to PATH  [{shell}]");
            }
        }
    }

    if cfg!(windows) {
        println!("\x1b[1;32m✓\x1b[0m Done! Open a new terminal to use `atlas`.");
    } else {
        println!("\x1b[1;32m✓\x1b[0m Done! Restart your shell or run:");
        println!("  export PATH=\"{}:$PATH\"", install_dir.display());
    }
    println!();

    // Install the skills bundled in this binary into ~/.agents/skills
    println!("\x1b[1;34m◆\x1b[0m Installing skills...");
    if let Err(e) = skills::install::handle(vec![], true) {
        eprintln!("\x1b[33m◆\x1b[0m Skills install failed (non-fatal): {e}");
    }

    println!();
    println!("  \x1b[2mGet started:\x1b[0m");
    println!("  atlas dev              start the TUI");
    println!("  atlas update           keep atlas up to date");
    println!(
        "  atlas setup            wire atlas into omp, Claude Code or Codex (MCP + skills + hooks)"
    );

    Ok(())
}

/// Windows: put `dir` on the user's `Path` (registry, `HKCU\Environment`), the way installers do,
/// so new terminals find `atlas`. `Ok(false)` when it is already there. PowerShell's
/// `SetEnvironmentVariable` writes the full value (never `setx`, which truncates at 1024
/// characters) and tells running programs the environment changed.
#[cfg(windows)]
pub(crate) fn add_user_path(dir: &Path) -> Result<bool> {
    edit_user_path(dir, "add")
}

/// Windows: take `dir` off the user's `Path`. `Ok(false)` when it was not there.
#[cfg(windows)]
pub(crate) fn remove_user_path(dir: &Path) -> Result<bool> {
    edit_user_path(dir, "remove")
}

#[cfg(windows)]
fn edit_user_path(dir: &Path, op: &str) -> Result<bool> {
    use atlas_core::sys::CommandExt as _;
    // The dir travels in the environment, never inside the script text.
    const SCRIPT: &str = "$d = $env:ATLAS_PATH_DIR; $op = $env:ATLAS_PATH_OP; \
        $cur = [Environment]::GetEnvironmentVariable('Path', 'User'); \
        $parts = @(if ($cur) { $cur -split ';' | Where-Object { $_ -ne '' } }); \
        $has = @($parts | Where-Object { $_.TrimEnd('\\') -ieq $d.TrimEnd('\\') }).Count -gt 0; \
        if ($op -eq 'add') { if ($has) { 'same'; exit } ; $parts += $d } \
        else { if (-not $has) { 'same'; exit } ; $parts = @($parts | Where-Object { $_.TrimEnd('\\') -ine $d.TrimEnd('\\') }) } \
        [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User'); 'changed'";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("ATLAS_PATH_DIR", dir)
        .env("ATLAS_PATH_OP", op)
        .no_window()
        .stdin(std::process::Stdio::null())
        .output()
        .context("cannot run powershell")?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "changed")
}

#[cfg_attr(windows, allow(dead_code))]
fn setup_path_all_shells(install_dir: &Path) -> Vec<String> {
    let mut written = Vec::new();
    if shell_exists("fish")
        && let Ok(label) = setup_fish(install_dir)
    {
        written.push(label);
    }
    if shell_exists("zsh")
        && let Ok(label) = setup_zsh(install_dir)
    {
        written.push(label);
    }
    if shell_exists("bash")
        && let Ok(label) = setup_bash(install_dir)
    {
        written.push(label);
    }
    written
}

#[cfg_attr(windows, allow(dead_code))]
fn shell_exists(name: &str) -> bool {
    let shell_var = std::env::var("SHELL").unwrap_or_default();
    if std::path::Path::new(&shell_var)
        .file_name()
        .map(|n| n.to_string_lossy())
        .as_deref()
        == Some(name)
    {
        return true;
    }
    which_exists(name)
}

fn which_exists(cmd: &str) -> bool {
    atlas_core::paths::which(cmd).is_some()
}

fn setup_fish(install_dir: &Path) -> Result<String> {
    let conf_d = fish_conf_d()?;
    std::fs::create_dir_all(&conf_d)
        .with_context(|| format!("Could not create {}", conf_d.display()))?;
    let rc = conf_d.join("atlas.fish");
    let line = format!("fish_add_path {}", install_dir.display());
    if rc.exists() {
        let existing = std::fs::read_to_string(&rc).unwrap_or_default();
        if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
            return Ok("fish  (already set)".to_string());
        }
    }
    std::fs::write(&rc, format!("# Atlas CLI\n{line}\n"))
        .with_context(|| format!("Could not write {}", rc.display()))?;
    Ok(format!("fish  {}", rc.display()))
}

fn fish_conf_d() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    Ok(base.join("fish").join("conf.d"))
}

fn setup_zsh(install_dir: &Path) -> Result<String> {
    let rc = zsh_env_file()?;
    let line = format!("export PATH=\"{}:$PATH\"", install_dir.display());
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
        return Ok("zsh   (already set)".to_string());
    }
    append_line(&rc, &line)?;
    Ok(format!("zsh   {}", rc.display()))
}

fn zsh_env_file() -> Result<PathBuf> {
    let zdotdir = std::env::var("ZDOTDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home)
        });
    Ok(zdotdir.join(".zshenv"))
}

fn setup_bash(install_dir: &Path) -> Result<String> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    let home = PathBuf::from(home);
    let candidates = [".bash_profile", ".bash_login", ".bashrc"];
    let rc = candidates
        .iter()
        .map(|f| home.join(f))
        .find(|p| p.is_file());
    let rc = match rc {
        Some(r) => r,
        None => return Ok("bash  (no rc file found)".to_string()),
    };
    let line = format!("export PATH=\"{}:$PATH\"", install_dir.display());
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
        return Ok("bash  (already set)".to_string());
    }
    append_line(&rc, &line)?;
    Ok(format!("bash  {}", rc.display()))
}

fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Could not open {}", path.display()))?;
    writeln!(f, "\n# Atlas CLI\n{line}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_dir_default() {
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let home = std::env::var("HOME").unwrap();
        let dir = PathBuf::from(home).join(INSTALL_DIR);
        assert_eq!(dir, PathBuf::from("/home/dev/.atlas/bin"));
    }

    #[test]
    fn test_shell_exists_via_path() {
        assert!(which_exists(if cfg!(windows) { "cmd" } else { "sh" }));
        assert!(!which_exists("atlas-no-such-shell"));
    }

    #[test]
    fn test_fish_conf_d_default() {
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let dir = fish_conf_d().unwrap();
        assert_eq!(dir, PathBuf::from("/home/dev/.config/fish/conf.d"));
    }

    #[test]
    fn test_zsh_env_file_default() {
        unsafe { std::env::remove_var("ZDOTDIR") };
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let f = zsh_env_file().unwrap();
        assert_eq!(f, PathBuf::from("/home/dev/.zshenv"));
    }

    fn judge(own: &str, installed: Option<&str>) -> Upgrade {
        let t = tempfile::tempdir().unwrap();
        let dest = t.path().join("atlas.exe");
        std::fs::write(&dest, "x").unwrap();
        judge_upgrade(&dest, own, |_| installed.map(str::to_string))
    }

    /// Incident (1.5.0-alpha.21..24, Windows): the installer only installed the CLI when absent,
    /// so `~/.atlas/bin/atlas.exe` and the hostd it starts stayed on an older build.
    #[test]
    fn upgrade_replaces_only_an_older_or_unreadable_cli() {
        assert_eq!(
            judge("1.5.0-alpha.24", Some("1.5.0-alpha.9")),
            Upgrade::Replace("1.5.0-alpha.9".into()),
            "alpha.9 is older than alpha.24 (numeric, not text order)"
        );
        assert_eq!(
            judge("1.5.0", Some("1.5.0-alpha.24")),
            Upgrade::Replace("1.5.0-alpha.24".into())
        );
        assert_eq!(
            judge("1.5.0-alpha.24", None),
            Upgrade::Replace("unreadable".into())
        );
        assert_eq!(
            judge("1.5.0-alpha.24", Some("garbage")),
            Upgrade::Replace("garbage".into())
        );
        assert_eq!(
            judge("1.5.0-alpha.24", Some("1.5.0-alpha.24")),
            Upgrade::Current("1.5.0-alpha.24".into())
        );
        assert_eq!(
            judge("1.5.0-alpha.24", Some("1.6.0")),
            Upgrade::Current("1.6.0".into()),
            "never a downgrade"
        );
    }

    #[test]
    fn upgrade_leaves_a_missing_cli_to_install_and_never_runs_a_link() {
        let t = tempfile::tempdir().unwrap();
        let missing = t.path().join("atlas.exe");
        assert_eq!(
            judge_upgrade(&missing, "1.0.0", |_| panic!("nothing to run")),
            Upgrade::Absent
        );
        #[cfg(unix)]
        {
            let real = t.path().join("target-atlas");
            std::fs::write(&real, "x").unwrap();
            let link = t.path().join("atlas");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert_eq!(
                judge_upgrade(&link, "1.0.0", |_| panic!("a link is a dev build")),
                Upgrade::Linked
            );
        }
    }

    /// A program that prints `stdout` and exits `code`, as a real file `installed_version` can run.
    #[cfg(windows)]
    fn stub(dir: &Path, name: &str, stdout: &str, code: i32) -> PathBuf {
        let p = dir.join(format!("{name}.cmd"));
        let echo: String = stdout.lines().map(|l| format!("@echo {l}\r\n")).collect();
        std::fs::write(&p, format!("{echo}@exit /b {code}\r\n")).unwrap();
        p
    }

    #[cfg(unix)]
    fn stub(dir: &Path, name: &str, stdout: &str, code: i32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(format!("{name}.sh"));
        let echo: String = stdout.lines().map(|l| format!("echo '{l}'\n")).collect();
        std::fs::write(&p, format!("#!/bin/sh\n{echo}exit {code}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// `installed_version` runs the real file and reads the first line of what old and new builds
    /// print for `atlas version`; a program that fails or prints something else is "unreadable".
    #[test]
    fn installed_version_reads_what_a_real_process_prints() {
        let t = tempfile::tempdir().unwrap();
        let ok = stub(t.path(), "ok", "atlas 1.5.0-alpha.9\n  os: x", 0);
        let bad = stub(t.path(), "bad", "atlas 1.5.0-alpha.9", 3);
        let junk = stub(t.path(), "junk", "not atlas", 0);
        assert_eq!(installed_version(&ok).as_deref(), Some("1.5.0-alpha.9"));
        assert_eq!(installed_version(&bad), None, "a failing run is unreadable");
        assert_eq!(installed_version(&junk), None);
        assert_eq!(installed_version(&t.path().join("missing")), None);
        // And through the decision: the version it read is older than a newer own build.
        assert_eq!(
            judge_upgrade(&ok, "1.5.0-alpha.24", installed_version),
            Upgrade::Replace("1.5.0-alpha.9".into())
        );
    }

    #[test]
    fn replace_binary_swaps_the_file_and_leaves_no_temp_behind() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("new");
        let dest = t.path().join("bin").join("atlas.exe");
        std::fs::write(&src, "new").unwrap();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, "old").unwrap();
        replace_binary(&src, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new");
        assert!(!dest.parent().unwrap().join(".atlas-install-tmp").exists());
    }
}
