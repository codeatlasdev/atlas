use anyhow::{Context, Result, bail};
use atlas_core::util::expand_tilde;
use clap::Args;
use std::path::{Path, PathBuf};

use super::AtlasCommand;
use crate::commands::shellrc::{self, ShellEnv};
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

/// Where an install by process `pid` stages the new binary: beside `dest`'s folder (so the final
/// rename is atomic), under a name no other running installer shares.
fn staging_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!(".atlas-install-tmp-{pid}"))
}

/// Removes the staging file when dropped (also after the successful rename, where it is gone), so
/// no error path leaves a half-copied binary in `~/.atlas/bin` (a full disk is when it hurts most).
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Copy `src` over `dest` through a temp file in the same folder.
///
/// Windows: a running `atlas.exe` (hostd, a dev session, an MCP server another tool spawned)
/// cannot be overwritten or deleted, only renamed. So an existing file is moved aside to
/// `.atlas-old-<pid>.exe`, the new one takes its place, and the aside file is deleted right away;
/// when it is still running that fails and the next install sweeps it (`sweep_old`). If the new
/// file cannot be put in place the old one is moved back, so there is never no `atlas`.
fn replace_binary(src: &Path, dest: &Path) -> Result<()> {
    let dir = dest.parent().context("install path has no folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let tmp = Staging(staging_path(dir, std::process::id()));
    std::fs::copy(src, &tmp.0)
        .with_context(|| format!("Could not copy binary to {}", tmp.0.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp.0, std::fs::Permissions::from_mode(0o755))?;
    }

    // Only a file is moved aside: anything else at `dest` is left for the rename to refuse.
    #[cfg(windows)]
    let aside = {
        let aside = dir.join(format!(".atlas-old-{}.exe", std::process::id()));
        sweep_old(dir);
        (dest.is_file() && std::fs::rename(dest, &aside).is_ok()).then_some(aside)
    };
    let installed = std::fs::rename(&tmp.0, dest)
        .with_context(|| format!("Could not install to {}", dest.display()));
    #[cfg(windows)]
    if let Some(aside) = aside {
        if installed.is_ok() {
            let _ = std::fs::remove_file(&aside);
        } else {
            let _ = std::fs::rename(&aside, dest);
        }
    }
    installed
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

/// What [`install_binary`] did with the CLI at the destination.
#[derive(Debug, PartialEq, Eq)]
enum Installed {
    /// This binary was copied there.
    Copied,
    /// A dev-build link (`atlas link`) is there: left alone, never overwritten.
    KeptLink,
    /// A build with higher SemVer precedence is already there: left alone.
    KeptNewer(String),
}

/// Put `src` at `dest` unless something better is already there. `atlas install` is what the
/// installer scripts run with the *bootstrap* binary (the newest Stable, which may be older than
/// what the machine runs), so re-running the installer must neither overwrite a developer's
/// `atlas link` symlink nor downgrade a newer install (and strand it there when the update that
/// follows cannot reach the network). Same judgment as `install --upgrade`: [`judge_upgrade`].
fn install_binary(
    src: &Path,
    dest: &Path,
    own: &str,
    installed: impl FnOnce(&Path) -> Option<String>,
) -> Result<Installed> {
    match judge_upgrade(dest, own, installed) {
        Upgrade::Linked => Ok(Installed::KeptLink),
        // Same version re-copies (a reinstall repairs a damaged file); only a strictly newer one stays.
        Upgrade::Current(v) if is_newer(&v, own) => Ok(Installed::KeptNewer(v)),
        Upgrade::Current(_) | Upgrade::Absent | Upgrade::Replace(_) => {
            replace_binary(src, dest)?;
            Ok(Installed::Copied)
        }
    }
}

/// `have` has higher SemVer precedence than `own` (both parsed; anything else is not newer).
fn is_newer(have: &str, own: &str) -> bool {
    use atlas_core::channel;
    matches!(
        (channel::parse_tag(have), channel::parse_tag(own)),
        (Some(h), Some(o)) if channel::precedence(&h, &o).is_gt()
    )
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
    match install_binary(
        &current_exe,
        &dest,
        env!("CARGO_PKG_VERSION"),
        installed_version,
    )? {
        Installed::Copied => println!("\x1b[32m✓\x1b[0m Installed → {}", dest.display()),
        Installed::KeptLink => println!(
            "\x1b[32m✓\x1b[0m Kept      {} (linked to a dev build; `atlas unlink` goes back)",
            dest.display()
        ),
        Installed::KeptNewer(v) => println!(
            "\x1b[32m✓\x1b[0m Kept      {} (v{v} is newer than this v{}; never a downgrade)",
            dest.display(),
            env!("CARGO_PKG_VERSION")
        ),
    }

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

/// Windows: whether `dir` is on the user's `Path`. Reads only; nothing is written.
#[cfg(windows)]
pub(crate) fn has_user_path(dir: &Path) -> Result<bool> {
    edit_user_path(dir, "has")
}

/// Runs the one PowerShell script behind add/remove/has. Its answer is `changed` (written),
/// `same` (nothing to do) or, for `has`, `yes`/`no`; `Ok(true)` is `changed` or `yes`.
#[cfg(windows)]
fn edit_user_path(dir: &Path, op: &str) -> Result<bool> {
    use atlas_core::sys::CommandExt as _;
    // The dir travels in the environment, never inside the script text.
    const SCRIPT: &str = "$d = $env:ATLAS_PATH_DIR; $op = $env:ATLAS_PATH_OP; \
        $cur = [Environment]::GetEnvironmentVariable('Path', 'User'); \
        $parts = @(if ($cur) { $cur -split ';' | Where-Object { $_ -ne '' } }); \
        $has = @($parts | Where-Object { $_.TrimEnd('\\') -ieq $d.TrimEnd('\\') }).Count -gt 0; \
        if ($op -eq 'has') { if ($has) { 'yes' } else { 'no' }; exit }; \
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
    let answer = String::from_utf8_lossy(&out.stdout);
    Ok(matches!(answer.trim(), "changed" | "yes"))
}

/// What putting `dir` on `PATH` did for one shell.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
enum PathSetup {
    /// The startup file was written (or created).
    Written(PathBuf),
    /// The startup file already puts `dir` on `PATH`.
    AlreadySet(PathBuf),
    /// The shell has no startup file to append to (bash with none of its login files).
    NoFile,
}

/// One line per shell for the installer's report; `NoFile` is not a success and says so.
#[cfg_attr(windows, allow(dead_code))]
fn setup_label(shell: &str, s: &PathSetup) -> Option<String> {
    match s {
        PathSetup::Written(p) => Some(format!("{shell}  {}", p.display())),
        PathSetup::AlreadySet(p) => Some(format!("{shell}  {} (already set)", p.display())),
        PathSetup::NoFile => None,
    }
}

/// Put `install_dir` on `PATH` for every shell on this machine. Returns the report lines of the
/// shells that now have it; empty means the user must do it by hand.
#[cfg_attr(windows, allow(dead_code))]
fn setup_path_all_shells(install_dir: &Path) -> Vec<String> {
    let env = ShellEnv::current();
    let mut written = Vec::new();
    for (name, setup) in [
        (
            "fish",
            setup_fish as fn(&ShellEnv, &Path) -> Result<PathSetup>,
        ),
        ("zsh", setup_zsh),
        ("bash", setup_bash),
    ] {
        if shell_exists(name)
            && let Ok(s) = setup(&env, install_dir)
            && let Some(label) = setup_label(name, &s)
        {
            written.push(label);
        }
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

/// fish: `conf.d/atlas.fish`, a file of Atlas's own. One that is not what Atlas writes is the
/// user's (their own `atlas` wrapper, say): it is left alone and `PATH` is reported as not set.
#[cfg_attr(windows, allow(dead_code))]
fn setup_fish(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    let rc = env.fish_file();
    if let Ok(existing) = std::fs::read_to_string(&rc) {
        if shellrc::is_fish_file_ours(&existing, install_dir) {
            return Ok(PathSetup::AlreadySet(rc));
        }
        bail!(
            "{} exists and is not the file Atlas writes; add `{}` to it yourself",
            rc.display(),
            shellrc::fish_line(install_dir)
        );
    }
    let conf_d = env.fish_conf_d();
    std::fs::create_dir_all(&conf_d)
        .with_context(|| format!("Could not create {}", conf_d.display()))?;
    std::fs::write(&rc, shellrc::fish_contents(install_dir))
        .with_context(|| format!("Could not write {}", rc.display()))?;
    Ok(PathSetup::Written(rc))
}

/// zsh: `$ZDOTDIR/.zshenv` (else `~/.zshenv`), which every zsh reads.
#[cfg_attr(windows, allow(dead_code))]
fn setup_zsh(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    append_block(env.zsh_file(), install_dir)
}

/// bash: the first login file that exists. None is not an error, but not a success either.
#[cfg_attr(windows, allow(dead_code))]
fn setup_bash(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    match env.bash_file() {
        Some(rc) => append_block(rc, install_dir),
        None => Ok(PathSetup::NoFile),
    }
}

/// Append the PATH block to `rc` unless it already has the exact line.
#[cfg_attr(windows, allow(dead_code))]
fn append_block(rc: PathBuf, install_dir: &Path) -> Result<PathSetup> {
    use std::io::Write;
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if shellrc::has_path_entry(&existing, install_dir) {
        return Ok(PathSetup::AlreadySet(rc));
    }
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&rc)
        .with_context(|| format!("Could not open {}", rc.display()))?;
    // A file that does not end in a newline would glue the marker to its last line.
    let lead = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    write!(f, "{lead}{}", shellrc::block(install_dir))
        .with_context(|| format!("Could not write {}", rc.display()))?;
    Ok(PathSetup::Written(rc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_exists_via_path() {
        assert!(which_exists(if cfg!(windows) { "cmd" } else { "sh" }));
        assert!(!which_exists("atlas-no-such-shell"));
    }

    /// A home in a temp dir with `~/.atlas/bin` as the install dir.
    fn shell_home() -> (tempfile::TempDir, ShellEnv, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        let env = ShellEnv {
            home: home.clone(),
            zdotdir: None,
            xdg_config: None,
        };
        (t, env, home.join(".atlas/bin"))
    }

    /// Re-running the installer must not stack `# Atlas CLI` blocks: the second run reports
    /// "already set" and leaves the file byte for byte as it was.
    #[test]
    fn path_setup_is_idempotent_for_every_shell() {
        let (_t, env, bin) = shell_home();
        std::fs::write(env.home.join(".bashrc"), "alias ll='ls -l'\n").unwrap();
        for setup in [setup_zsh, setup_bash, setup_fish] {
            let first = setup(&env, &bin).unwrap();
            assert!(matches!(first, PathSetup::Written(_)), "{first:?}");
        }
        let files = [env.zsh_file(), env.bash_file().unwrap(), env.fish_file()];
        let before: Vec<_> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();
        for setup in [setup_zsh, setup_bash, setup_fish] {
            let again = setup(&env, &bin).unwrap();
            assert!(matches!(again, PathSetup::AlreadySet(_)), "{again:?}");
        }
        let after: Vec<_> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();
        assert_eq!(before, after);
        let bashrc = std::fs::read_to_string(env.bash_file().unwrap()).unwrap();
        assert_eq!(bashrc.matches("# Atlas CLI").count(), 1, "{bashrc}");
    }

    /// Incident class: "already set" was a substring test, so a longer path (`bin-old`) or a comment
    /// that mentions the folder made the installer skip the line and leave `atlas` off `PATH`.
    #[test]
    fn a_lookalike_in_the_rc_file_does_not_count_as_already_set() {
        let (_t, env, bin) = shell_home();
        let rc = env.zsh_file();
        let mine = format!(
            "export PATH=\"{}-old:$PATH\"\n# {} is where atlas lives\n",
            bin.display(),
            bin.display()
        );
        std::fs::write(&rc, &mine).unwrap();
        assert_eq!(
            setup_zsh(&env, &bin).unwrap(),
            PathSetup::Written(rc.clone())
        );
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(
            text.starts_with(&mine),
            "the user's lines stay first: {text}"
        );
        assert!(shellrc::has_path_entry(&text, &bin));
    }

    /// What install writes, uninstall removes, leaving the user's file as it was, also when it
    /// had no trailing newline.
    #[test]
    fn install_then_strip_restores_the_rc_file() {
        let (_t, env, bin) = shell_home();
        for original in ["", "export A=1\n", "export A=1"] {
            std::fs::write(env.zsh_file(), original).unwrap();
            setup_zsh(&env, &bin).unwrap();
            let written = std::fs::read_to_string(env.zsh_file()).unwrap();
            assert!(written.contains("# Atlas CLI"), "{written:?}");
            let back = shellrc::strip_block(&written, &bin).unwrap();
            let want = if original.is_empty() || original.ends_with('\n') {
                original.to_string()
            } else {
                format!("{original}\n")
            };
            assert_eq!(back, want, "{original:?}");
        }
    }

    /// A last line without a newline must not swallow the marker.
    #[test]
    fn the_block_starts_on_its_own_line_after_a_file_without_trailing_newline() {
        let (_t, env, bin) = shell_home();
        std::fs::write(env.zsh_file(), "export A=1").unwrap();
        setup_zsh(&env, &bin).unwrap();
        let text = std::fs::read_to_string(env.zsh_file()).unwrap();
        assert!(text.starts_with("export A=1\n"), "{text:?}");
        assert!(text.lines().any(|l| l == "# Atlas CLI"), "{text:?}");
    }

    #[test]
    fn zsh_follows_zdotdir() {
        let (_t, mut env, bin) = shell_home();
        env.zdotdir = Some(env.home.join("zdot"));
        std::fs::create_dir_all(env.home.join("zdot")).unwrap();
        let s = setup_zsh(&env, &bin).unwrap();
        assert_eq!(s, PathSetup::Written(env.home.join("zdot/.zshenv")));
        assert!(!env.home.join(".zshenv").exists());
    }

    /// Incident class: with none of bash's login files the installer printed `Added to PATH [bash
    /// (no rc file found)]` as if it had worked. It is not a success and must not be reported as one.
    #[test]
    fn bash_without_a_login_file_is_not_reported_as_set() {
        let (_t, env, bin) = shell_home();
        let s = setup_bash(&env, &bin).unwrap();
        assert_eq!(s, PathSetup::NoFile);
        assert_eq!(setup_label("bash", &s), None);
        assert!(
            !env.home.join(".bashrc").exists(),
            "nothing is created for bash"
        );
        let done = setup_label("zsh", &PathSetup::Written(env.zsh_file())).unwrap();
        assert!(
            done.starts_with("zsh") && !done.contains("already"),
            "{done}"
        );
        let had = setup_label("zsh", &PathSetup::AlreadySet(env.zsh_file())).unwrap();
        assert!(had.contains("already set"), "{had}");
    }

    /// fish's file is Atlas's own: one the user wrote under that name is never overwritten.
    #[test]
    fn fish_never_overwrites_a_file_it_did_not_write() {
        let (_t, env, bin) = shell_home();
        let rc = env.fish_file();
        std::fs::create_dir_all(rc.parent().unwrap()).unwrap();
        let mine = "function atlas\n  echo my wrapper\nend\n";
        std::fs::write(&rc, mine).unwrap();
        let e = setup_fish(&env, &bin).unwrap_err().to_string();
        assert!(e.contains("not the file Atlas writes"), "{e}");
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), mine);
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

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
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
        assert_eq!(names(dest.parent().unwrap()), ["atlas.exe"]);
    }

    /// Incident class: the staging file was `.atlas-install-tmp`, left behind (up to a whole
    /// half-written binary) whenever the final rename failed, and shared by two installers at once.
    #[test]
    fn a_failed_replace_leaves_no_staging_file_and_keeps_the_old_binary() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("new");
        std::fs::write(&src, "new").unwrap();
        let bin = t.path().join("bin");
        // A non-empty directory where the binary goes: the final rename cannot replace it.
        let dest = bin.join("atlas");
        std::fs::create_dir_all(dest.join("inside")).unwrap();
        assert!(replace_binary(&src, &dest).is_err());
        assert_eq!(names(&bin), ["atlas"], "no staging file left in {bin:?}");
        assert!(dest.join("inside").is_dir(), "what was there is untouched");
    }

    /// Windows contract: a running `atlas.exe` cannot be deleted, only renamed. The install still
    /// goes through, the old image waits aside while it runs, and the next install sweeps it.
    #[cfg(windows)]
    #[test]
    fn a_running_image_is_moved_aside_and_swept_by_the_next_install() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("new");
        std::fs::write(&src, "new").unwrap();
        let bin = t.path().join("bin");
        let dest = bin.join("atlas.exe");
        std::fs::create_dir_all(&bin).unwrap();
        // A real running image (a handle with FILE_SHARE_DELETE would allow the delete and prove
        // nothing): a copy of ping.exe, kept running from `dest`.
        let ping = std::path::Path::new(&std::env::var("SystemRoot").unwrap())
            .join("System32")
            .join("PING.EXE");
        std::fs::copy(&ping, &dest).unwrap();
        let mut running = std::process::Command::new(&dest)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();

        let first = replace_binary(&src, &dest);
        let aside: Vec<_> = names(&bin)
            .into_iter()
            .filter(|n| n.starts_with(".atlas-old-"))
            .collect();
        let _ = running.kill();
        let _ = running.wait();
        first.unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new");
        assert_eq!(aside.len(), 1, "the running image waits aside: {aside:?}");

        replace_binary(&src, &dest).unwrap();
        assert_eq!(names(&bin), ["atlas.exe"], "the next install swept it");
    }

    #[test]
    fn two_installs_stage_under_different_names() {
        let t = tempfile::tempdir().unwrap();
        let a = staging_path(t.path(), 1);
        let b = staging_path(t.path(), 2);
        assert_ne!(a, b, "concurrent installers must not share a staging file");
        assert_eq!(
            a.parent(),
            Some(t.path()),
            "same folder as the target (atomic rename)"
        );
    }

    /// The installer scripts run `atlas install` with the bootstrap binary, which can be older than
    /// what the machine already runs. It must keep the newer install (the update step that follows
    /// may be offline), copy an equal or older one, and never replace a dev link.
    #[test]
    fn install_never_downgrades_and_never_replaces_a_link() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("bootstrap");
        std::fs::write(&src, "bootstrap").unwrap();
        let put = |body: &str| {
            let d = t.path().join("bin").join("atlas");
            std::fs::create_dir_all(d.parent().unwrap()).unwrap();
            let _ = std::fs::remove_file(&d);
            std::fs::write(&d, body).unwrap();
            d
        };
        let run = |dest: &Path, own: &str, have: Option<&str>| {
            install_binary(&src, dest, own, |_| have.map(str::to_string)).unwrap()
        };

        // Newer on disk (alpha.10 vs the bootstrap's alpha.9: numeric, not text, order): kept.
        let dest = put("on-disk");
        assert_eq!(
            run(&dest, "1.5.0-alpha.9", Some("1.5.0-alpha.10")),
            Installed::KeptNewer("1.5.0-alpha.10".into())
        );
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "on-disk");
        // A stable outranks its own prerelease.
        assert_eq!(
            run(&dest, "1.5.0-rc.1", Some("1.5.0")),
            Installed::KeptNewer("1.5.0".into())
        );
        // Older on disk: replaced. Equal: re-copied (a reinstall repairs a damaged file).
        assert_eq!(
            run(&dest, "1.5.0-alpha.10", Some("1.5.0-alpha.9")),
            Installed::Copied
        );
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "bootstrap");
        std::fs::write(&dest, "damaged").unwrap();
        assert_eq!(run(&dest, "1.5.0", Some("1.5.0")), Installed::Copied);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "bootstrap");
        // Unreadable on disk: replaced.
        std::fs::write(&dest, "garbage").unwrap();
        assert_eq!(run(&dest, "1.5.0", None), Installed::Copied);
        // Absent: installed.
        let fresh = t.path().join("fresh").join("atlas");
        assert_eq!(run(&fresh, "1.5.0", None), Installed::Copied);
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "bootstrap");

        #[cfg(unix)]
        {
            let real = t.path().join("target-debug-atlas");
            std::fs::write(&real, "dev build").unwrap();
            let link = t.path().join("linked").join("atlas");
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert_eq!(run(&link, "1.5.0", None), Installed::KeptLink);
            assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
            assert_eq!(std::fs::read_to_string(&real).unwrap(), "dev build");
        }
    }

    #[test]
    fn is_newer_is_semver_precedence() {
        assert!(is_newer("1.5.0-alpha.10", "1.5.0-alpha.9"));
        assert!(is_newer("1.5.0", "1.5.0-rc.1"));
        assert!(is_newer("v1.6.0", "1.5.9"));
        assert!(!is_newer("1.5.0-alpha.9", "1.5.0-alpha.10"));
        assert!(
            !is_newer("1.5.0", "1.5.0+build.7"),
            "build metadata is not precedence"
        );
        assert!(!is_newer("garbage", "1.5.0"));
        assert!(!is_newer("1.5.0", "garbage"));
    }
}
