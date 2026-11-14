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

    #[cfg(windows)]
    {
        let already_in_path = std::env::var_os("PATH")
            .is_some_and(|path| std::env::split_paths(&path).any(|p| p == install_dir));
        if already_in_path {
            println!("\x1b[32m✓\x1b[0m Already in PATH");
        } else {
            match add_user_path(&install_dir) {
                Ok(_) => println!("\x1b[32m✓\x1b[0m Added to PATH  [user environment]"),
                Err(e) => println!(
                    "\x1b[33m◆\x1b[0m Add {} to your PATH manually ({e:#})",
                    install_dir.display()
                ),
            }
        }
        println!("\x1b[1;32m✓\x1b[0m Done! Open a new terminal to use `atlas`.");
    }
    #[cfg(not(windows))]
    setup_path_unix(&install_dir, &dest);
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

/// Unix: write every shell's startup file (always, even when this process already has the dir on
/// `PATH`: an Atlas terminal adds it, other terminals do not), link `~/.local/bin/atlas` when that
/// dir is on `PATH`, report each outcome, and end with what makes `atlas` work in this shell.
#[cfg(not(windows))]
fn setup_path_unix(install_dir: &Path, dest: &Path) {
    if std::env::var_os("SUDO_USER").is_some_and(|u| !u.is_empty() && u != "root") {
        println!(
            "\x1b[33m◆\x1b[0m Running under sudo: this installs for root, not for {}. Run the installer without sudo.",
            std::env::var("SUDO_USER").unwrap_or_default()
        );
    }
    let env = ShellEnv::current();
    for (shell, outcome) in setup_path_all_shells(&env, install_dir) {
        match outcome {
            Ok(PathSetup::NoFile) => println!(
                "\x1b[33m◆\x1b[0m {shell}: no startup file to add to; create ~/.bash_profile with:\n  {}",
                shellrc::path_line(install_dir)
            ),
            Ok(s) => {
                if let Some(label) = setup_label(shell, &s) {
                    println!("\x1b[32m✓\x1b[0m Added to PATH  [{label}]");
                }
            }
            Err(e) => println!("\x1b[33m◆\x1b[0m {shell}: {e:#}"),
        }
    }
    let path = std::env::var_os("PATH");
    let ready_now = match link_local_bin(&env, dest, path.as_deref()) {
        Ok(LocalLink::Linked(link)) => {
            println!(
                "\x1b[32m✓\x1b[0m Linked {} → {}",
                link.display(),
                dest.display()
            );
            true
        }
        Ok(LocalLink::Taken(link)) => {
            println!(
                "\x1b[33m◆\x1b[0m {} exists and is not Atlas's link; left alone",
                link.display()
            );
            false
        }
        Ok(LocalLink::NotOnPath) => false,
        Err(e) => {
            println!("\x1b[33m◆\x1b[0m {e:#}");
            false
        }
    } || path.is_some_and(|p| std::env::split_paths(&p).any(|e| e == install_dir));

    let (shell, _) = atlas_core::login_env::resolve_shell();
    let name = Path::new(&shell)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let line = shellrc::line_for_shell(&name, install_dir);
    if line.is_none() {
        println!(
            "\x1b[33m◆\x1b[0m Atlas does not write {name}'s startup file: add {} to PATH there",
            install_dir.display()
        );
    }
    if ready_now {
        println!("\x1b[1;32m✓\x1b[0m Done! `atlas` works in this shell now.");
    } else {
        println!("\x1b[1;32m✓\x1b[0m Done! Open a new terminal, or run this in the current one:");
        match line {
            Some(l) => println!("  {l}"),
            None => println!("  (add {} to PATH)", install_dir.display()),
        }
    }
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

/// Put `install_dir` on `PATH` for every shell on this machine: each shell's outcome, an error
/// included (a read-only rc, a file of the user's under Atlas's name), so none goes unreported.
#[cfg_attr(windows, allow(dead_code))]
fn setup_path_all_shells(
    env: &ShellEnv,
    install_dir: &Path,
) -> Vec<(&'static str, Result<PathSetup>)> {
    type Setup = fn(&ShellEnv, &Path) -> Result<PathSetup>;
    let shells: [(&str, Setup); 4] = [
        ("fish", setup_fish),
        ("nu", setup_nu),
        ("zsh", setup_zsh),
        ("bash", setup_bash),
    ];
    shells
        .into_iter()
        .filter(|(name, _)| shell_exists(name))
        .map(|(name, setup)| (name, setup(env, install_dir)))
        .collect()
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

/// Write a startup file that is all Atlas's own (fish `conf.d`, nushell `autoload`). One that
/// `ours` does not recognize is the user's (their own `atlas` wrapper, say): left alone, an error
/// naming `line`. One of ours with older contents is rewritten.
#[cfg_attr(windows, allow(dead_code))]
fn write_owned_file(
    file: PathBuf,
    contents: &str,
    ours: impl Fn(&str) -> bool,
    line: &str,
) -> Result<PathSetup> {
    if let Ok(existing) = std::fs::read_to_string(&file) {
        if existing == contents {
            return Ok(PathSetup::AlreadySet(file));
        }
        if !ours(&existing) {
            bail!(
                "{} exists and is not the file Atlas writes; add `{line}` to it yourself",
                file.display()
            );
        }
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Could not create {}", dir.display()))?;
    }
    std::fs::write(&file, contents)
        .with_context(|| format!("Could not write {}", file.display()))?;
    Ok(PathSetup::Written(file))
}

/// fish: `conf.d/atlas.fish`. Replacing the file an older install wrote also takes the dir out of
/// the universal `fish_user_paths` that file's line had stored.
#[cfg_attr(windows, allow(dead_code))]
fn setup_fish(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    let file = env.fish_file();
    let had_legacy = std::fs::read_to_string(&file).is_ok_and(|c| {
        shellrc::is_fish_file_ours(&c, install_dir) && c != shellrc::fish_contents(install_dir)
    });
    let s = write_owned_file(
        file,
        &shellrc::fish_contents(install_dir),
        |c| shellrc::is_fish_file_ours(c, install_dir),
        &shellrc::fish_line(install_dir),
    )?;
    if had_legacy {
        // Best effort: the new file works either way; a stale universal entry only outlives uninstall.
        let _ = shellrc::fish_forget_universal(env, install_dir);
    }
    Ok(s)
}

/// nushell: `autoload/atlas.nu` (nushell 0.102+ loads it on every start).
#[cfg_attr(windows, allow(dead_code))]
fn setup_nu(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    write_owned_file(
        env.nu_file(),
        &shellrc::nu_contents(install_dir),
        |c| shellrc::is_nu_file_ours(c, install_dir),
        &shellrc::nu_line(install_dir),
    )
}

/// What linking `atlas` into `~/.local/bin` did.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
enum LocalLink {
    /// Made the link (or it already pointed at `dest`).
    Linked(PathBuf),
    /// `~/.local/bin` is not on `PATH`: a link there would not help.
    NotOnPath,
    /// Something else is already at `~/.local/bin/atlas`: the user's, left alone.
    Taken(PathBuf),
}

/// Link `~/.local/bin/atlas` -> `dest` when `~/.local/bin` is on `path` (the installer's own
/// `PATH`, so the session's). `atlas` then resolves in this very shell and in non-interactive
/// ones that never read an rc file (`bash -c`, agents, GUI-launched tools).
#[cfg_attr(windows, allow(dead_code))]
fn link_local_bin(
    env: &ShellEnv,
    dest: &Path,
    path: Option<&std::ffi::OsStr>,
) -> Result<LocalLink> {
    let link = env.local_bin_link();
    let Some(dir) = link.parent() else {
        return Ok(LocalLink::NotOnPath);
    };
    if !path.is_some_and(|p| std::env::split_paths(p).any(|e| e == dir)) {
        return Ok(LocalLink::NotOnPath);
    }
    match std::fs::read_link(&link) {
        Ok(to) if to == dest => return Ok(LocalLink::Linked(link)),
        Ok(_) => return Ok(LocalLink::Taken(link)),
        Err(_) if link.symlink_metadata().is_ok() => return Ok(LocalLink::Taken(link)),
        Err(_) => {}
    }
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(dest, &link)
        .with_context(|| format!("Could not link {}", link.display()))?;
    Ok(LocalLink::Linked(link))
}

/// zsh: `$ZDOTDIR/.zshenv` (else `~/.zshenv`), which every zsh reads.
#[cfg_attr(windows, allow(dead_code))]
fn setup_zsh(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    append_block(env.zsh_file(), install_dir)
}

/// bash: [`ShellEnv::bash_file`]. On macOS with none of the login files that is not an error, but
/// not a success either. On Linux, a login file (ssh, `bash -l`) that does not source `~/.bashrc`
/// gets the block too, so login shells find `atlas` as well.
#[cfg_attr(windows, allow(dead_code))]
fn setup_bash(env: &ShellEnv, install_dir: &Path) -> Result<PathSetup> {
    let Some(rc) = env.bash_file() else {
        return Ok(PathSetup::NoFile);
    };
    let s = append_block(rc, install_dir)?;
    if !cfg!(target_os = "macos")
        && let Some(login) = [".bash_profile", ".bash_login", ".profile"]
            .iter()
            .map(|f| env.home.join(f))
            .find(|p| p.is_file())
        && !std::fs::read_to_string(&login).is_ok_and(|c| c.contains(".bashrc"))
    {
        append_block(login, install_dir)?;
    }
    Ok(s)
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
        for setup in [setup_zsh, setup_bash, setup_fish, setup_nu] {
            let first = setup(&env, &bin).unwrap();
            assert!(matches!(first, PathSetup::Written(_)), "{first:?}");
        }
        let files = [
            env.zsh_file(),
            env.bash_file().unwrap(),
            env.fish_file(),
            env.nu_file(),
        ];
        let before: Vec<_> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();
        for setup in [setup_zsh, setup_bash, setup_fish, setup_nu] {
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
    #[cfg(target_os = "macos")]
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

    /// Incident (#265, Omarchy): `~/.bash_profile` existed, the line went there, and the terminal's
    /// non-login bash never read it. A real interactive non-login bash must see `atlas` on PATH.
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_interactive_bash_sees_the_path_even_with_a_bash_profile() {
        let (_t, env, bin) = shell_home();
        std::fs::write(env.home.join(".bash_profile"), "export FROM_PROFILE=1\n").unwrap();
        let s = setup_bash(&env, &bin).unwrap();
        assert_eq!(s, PathSetup::Written(env.home.join(".bashrc")));
        let out = std::process::Command::new("bash")
            .args(["-i", "-c", "echo \"$PATH\""])
            .env("HOME", &env.home)
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let path = String::from_utf8_lossy(&out.stdout);
        assert!(
            path.split(':').any(|p| p.trim() == bin.to_str().unwrap()),
            "{path}"
        );
        // ssh / `bash -l` read only the login file, which here does not source ~/.bashrc.
        let login = std::process::Command::new("bash")
            .args(["-l", "-c", "echo \"$PATH\""])
            .env("HOME", &env.home)
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let lp = String::from_utf8_lossy(&login.stdout);
        assert!(
            lp.split(':').any(|p| p.trim() == bin.to_str().unwrap()),
            "{lp}"
        );
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

    /// The nushell file follows the same rule: a user's own `atlas.nu` is never replaced.
    #[test]
    fn nu_never_overwrites_a_file_it_did_not_write() {
        let (_t, env, bin) = shell_home();
        let f = env.nu_file();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "alias a = atlas\n").unwrap();
        assert!(setup_nu(&env, &bin).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "alias a = atlas\n");
    }

    /// Up to 1.5.0-alpha.25 the fish file used `fish_add_path <dir>` (universal: fish stored the dir
    /// on disk, so it outlived uninstall). Re-installing replaces it with the global, quoted line.
    #[test]
    fn an_older_fish_file_is_rewritten_with_the_global_line() {
        let (_t, env, bin) = shell_home();
        let f = env.fish_file();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(
            &f,
            format!("# Atlas CLI\nfish_add_path {}\n", bin.display()),
        )
        .unwrap();
        assert_eq!(
            setup_fish(&env, &bin).unwrap(),
            PathSetup::Written(f.clone())
        );
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains("fish_add_path -g \""), "{text}");
    }

    /// The `~/.local/bin` link: only when that dir is on PATH, never over something else, and a
    /// second run is a no-op.
    #[test]
    #[cfg(unix)]
    fn local_bin_link_only_when_on_path_and_never_over_another_file() {
        let (_t, env, bin) = shell_home();
        let dest = bin.join("atlas");
        let link = env.local_bin_link();
        let local = link.parent().unwrap().to_path_buf();
        let off = std::ffi::OsString::from("/usr/bin:/bin");
        assert_eq!(
            link_local_bin(&env, &dest, Some(&off)).unwrap(),
            LocalLink::NotOnPath
        );
        assert!(
            link.symlink_metadata().is_err(),
            "nothing made when it would not help"
        );

        let on = std::env::join_paths([local.as_path(), Path::new("/usr/bin")]).unwrap();
        assert_eq!(
            link_local_bin(&env, &dest, Some(&on)).unwrap(),
            LocalLink::Linked(link.clone())
        );
        assert_eq!(std::fs::read_link(&link).unwrap(), dest);
        assert_eq!(
            link_local_bin(&env, &dest, Some(&on)).unwrap(),
            LocalLink::Linked(link.clone())
        );

        std::fs::remove_file(&link).unwrap();
        std::fs::write(&link, "#!/bin/sh\necho mine\n").unwrap();
        assert_eq!(
            link_local_bin(&env, &dest, Some(&on)).unwrap(),
            LocalLink::Taken(link.clone())
        );
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "#!/bin/sh\necho mine\n"
        );
    }

    /// Incident class (found by hand while fixing #265): the cleanup ran `fish --no-config`, which
    /// does not load universal variables, then `fish -c`, where the new `-g` line in `conf.d`
    /// shadowed them; either way the stale universal entry survived. Real fish, temp home;
    /// skipped where fish is not installed.
    #[test]
    #[cfg(unix)]
    fn reinstall_takes_the_old_universal_fish_path_away() {
        if !which_exists("fish") {
            eprintln!("fish not installed: skipped");
            return;
        }
        let (_t, env, bin) = shell_home();
        // `fish_add_path` skips a dir that does not exist: without it there is no global shadow.
        std::fs::create_dir_all(&bin).unwrap();
        let st = std::process::Command::new("fish")
            .env("HOME", &env.home)
            .env_remove("XDG_CONFIG_HOME")
            .args(["-c", "set -U fish_user_paths $argv[1]"])
            .arg(&bin)
            .status()
            .unwrap();
        assert!(st.success());
        assert!(shellrc::fish_universal_has(&env, &bin), "seeded");
        let f = env.fish_file();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(
            &f,
            format!("# Atlas CLI\nfish_add_path {}\n", bin.display()),
        )
        .unwrap();
        setup_fish(&env, &bin).unwrap();
        assert!(!shellrc::fish_universal_has(&env, &bin));
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
