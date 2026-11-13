//! `atlas uninstall`: take Atlas off this machine, listing everything first.
//!
//! Default: remove what `atlas install`, `atlas desktop` and `atlas update` put there (the CLI, the
//! desktop app, the shell `PATH` lines, update state and the terminal host's sockets) and keep what
//! is the user's: settings, the project list, worktrees (which can hold uncommitted work),
//! themes and the agent integrations. `--purge` removes all of `~/.atlas` as well.
//!
//! The work is a [`Plan`] built from an [`Env`] (pure reads of the file system), then
//! [`Plan::apply`]. Tests run both against a temp home, never the process environment.

use anyhow::{Context, Result, bail};
use clap::Args;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::AtlasCommand;
use super::shellrc::{self, ShellEnv};

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Skip the confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Also delete `~/.atlas`: settings, the project list, worktrees (with any uncommitted work),
    /// themes and the omp plugin
    #[arg(long)]
    pub purge: bool,
    /// List what would be removed and exit; nothing is changed
    #[arg(long, alias = "dry-run")]
    pub list: bool,
}

impl AtlasCommand for UninstallArgs {
    async fn run(self) -> Result<()> {
        handle(self.yes, self.purge, self.list).await
    }
}

/// Everything the plan depends on, so a test can stand a whole machine in a temp dir.
#[derive(Debug, Clone)]
pub(crate) struct Env {
    /// `~/.atlas`.
    pub atlas_home: PathBuf,
    pub shell: ShellEnv,
    /// The desktop app `atlas desktop` manages (`atlas_core::paths::installed_app`), if any.
    pub app: Option<PathBuf>,
}

impl Env {
    pub(crate) fn current() -> Result<Self> {
        let atlas_home = atlas_core::paths::atlas_home();
        Ok(Self {
            atlas_home,
            shell: ShellEnv::current(),
            app: atlas_core::paths::installed_app(),
        })
    }

    fn bin_dir(&self) -> PathBuf {
        self.atlas_home.join("bin")
    }
}

/// One thing the uninstall removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    /// A file or directory, removed whole.
    Path { path: PathBuf, note: &'static str },
    /// A symlink (`atlas link`): only the link goes, never what it points at.
    Link { path: PathBuf, note: &'static str },
    /// The `# Atlas CLI` block install appended to a shell startup file.
    RcBlock { file: PathBuf, dir: PathBuf },
    /// The fish `conf.d/atlas.fish` install wrote.
    FishFile { file: PathBuf },
    /// Windows: the install dir on the user's `Path`.
    #[cfg_attr(not(windows), allow(dead_code))]
    UserPath { dir: PathBuf },
}

impl Item {
    fn describe(&self) -> String {
        match self {
            Item::Path { path, note } | Item::Link { path, note } => {
                format!("{}  ({note})", path.display())
            }
            Item::RcBlock { file, .. } => {
                format!("{}  (the PATH lines Atlas added)", file.display())
            }
            Item::FishFile { file } => format!("{}  (the PATH file Atlas wrote)", file.display()),
            Item::UserPath { dir } => format!("{}  (user PATH entry)", dir.display()),
        }
    }
}

/// What the uninstall will do, and what it deliberately leaves.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    pub remove: Vec<Item>,
    /// User data kept (only when not purging): shown so nobody wonders where it went.
    pub keep: Vec<PathBuf>,
}

/// Runtime and install state under `~/.atlas` that means nothing without the binary: removed on a
/// plain uninstall. Everything else under `~/.atlas` is the user's.
const OWNED: &[(&str, &str)] = &[
    ("bin", "the atlas command and its update backup"),
    ("hostd.sock", "terminal host socket"),
    ("hostd.token", "terminal host token"),
    ("hostd.pid", "terminal host pid"),
    ("hostd.log", "terminal host log"),
    ("desktop.sock", "desktop control socket"),
    ("desktop.token", "desktop control token"),
    ("desktop-debug.sock", "desktop debug socket"),
    (".update_check", "update check cache"),
    (".update.lock", "update lock"),
    ("remote", "cached remote binaries"),
    (
        "desktop/Atlas.app.backup",
        "app backup from `atlas link --desktop`",
    ),
    ("desktop/appimage-version", "recorded app version"),
    ("desktop/app-version", "recorded app version"),
];

/// What `atlas desktop` writes under `$HOME` on Linux next to the AppImage (and, before 1.6, under
/// the old app id): the launcher entry and its icon.
const LINUX_MENU_FILES: &[&str] = &[
    ".local/share/applications/br.com.codeatlas.atlas.desktop",
    ".local/share/icons/hicolor/128x128/apps/br.com.codeatlas.atlas.png",
    ".local/share/applications/dev.codeatlas.atlas.desktop",
    ".local/share/icons/hicolor/128x128/apps/dev.codeatlas.atlas.png",
];

/// Plan the uninstall for `env`.
pub(crate) fn plan(env: &Env, purge: bool) -> Plan {
    let mut p = Plan::default();
    let exists = |path: &Path| path.symlink_metadata().is_ok();

    // The desktop app: a linked one is only unlinked. A distro package's app is never `env.app`
    // (its package manager owns it), and on Windows the app has its own uninstaller (Settings ›
    // Apps, `Atlas-Setup.exe --uninstall`) that also clears the registry; deleting its exe here
    // would leave that entry pointing at nothing.
    if let Some(app) = env.app.as_ref().filter(|a| !cfg!(windows) && exists(a)) {
        if super::desktop::is_symlink(app) {
            p.remove.push(Item::Link {
                path: app.clone(),
                note: "desktop app, linked to a dev build",
            });
        } else {
            p.remove.push(Item::Path {
                path: app.clone(),
                note: "Atlas Desktop",
            });
            // Linux: the menu entry and icon `atlas desktop` wrote for the AppImage.
            for rel in LINUX_MENU_FILES {
                let path = env.shell.home.join(rel);
                if cfg!(target_os = "linux") && exists(&path) {
                    p.remove.push(Item::Path {
                        path,
                        note: "desktop menu entry",
                    });
                }
            }
        }
    }

    // Shell startup lines: only the exact block install wrote, in whichever candidate has it.
    let bin = env.bin_dir();
    #[cfg(windows)]
    p.remove.push(Item::UserPath { dir: bin.clone() });
    for file in env.shell.candidates() {
        let has = std::fs::read_to_string(&file)
            .ok()
            .is_some_and(|c| shellrc::strip_block(&c, &bin).is_some());
        if has {
            p.remove.push(Item::RcBlock {
                file,
                dir: bin.clone(),
            });
        }
    }
    let fish = env.shell.fish_file();
    if std::fs::read_to_string(&fish).is_ok_and(|c| shellrc::is_fish_file_ours(&c, &bin)) {
        p.remove.push(Item::FishFile { file: fish });
    }

    // `~/.atlas`: all of it, or only what Atlas itself put there.
    if purge {
        if exists(&env.atlas_home) {
            p.remove.push(Item::Path {
                path: env.atlas_home.clone(),
                note: "settings, projects, worktrees, terminal host state, everything",
            });
        }
    } else {
        for (rel, note) in OWNED {
            let path = env.atlas_home.join(rel);
            if exists(&path) {
                p.remove.push(Item::Path { path, note });
            }
        }
        // Kept: every top-level entry that is not removed whole above (`desktop/` loses only its
        // recorded-version files).
        let whole: Vec<&str> = OWNED
            .iter()
            .map(|(r, _)| *r)
            .filter(|r| !r.contains('/'))
            .collect();
        if let Ok(rd) = std::fs::read_dir(&env.atlas_home) {
            p.keep = rd
                .flatten()
                .filter(|e| !whole.contains(&e.file_name().to_string_lossy().as_ref()))
                .map(|e| e.path())
                .collect();
            p.keep.sort();
        }
    }
    p
}

impl Plan {
    pub(crate) fn is_empty(&self) -> bool {
        self.remove.is_empty()
    }

    /// Print the plan the way `--list` and the confirmation show it.
    pub(crate) fn print(&self, out: &mut impl Write) -> std::io::Result<()> {
        writeln!(out, "  This will remove:")?;
        for item in &self.remove {
            writeln!(out, "    {}", item.describe())?;
        }
        if !self.keep.is_empty() {
            writeln!(out, "  and keep (run with --purge to delete it too):")?;
            for k in &self.keep {
                writeln!(out, "    {}", k.display())?;
            }
        }
        Ok(())
    }

    /// Carry the plan out. Keeps going after a step fails and returns every failure, so one
    /// stubborn file never leaves the rest of Atlas half removed without saying so.
    pub(crate) fn apply(&self, out: &mut impl Write) -> Vec<String> {
        let mut failures = Vec::new();
        for item in &self.remove {
            let done = apply_item(item);
            match done {
                Ok(()) => {
                    let _ = writeln!(out, "\x1b[32m✓\x1b[0m Removed  {}", item.describe());
                }
                Err(e) => {
                    let _ = writeln!(out, "\x1b[31m✗\x1b[0m Failed   {}: {e:#}", item.describe());
                    failures.push(format!("{}: {e:#}", item.describe()));
                }
            }
        }
        failures
    }
}

fn apply_item(item: &Item) -> Result<()> {
    match item {
        Item::Path { path, .. } => remove_path(path),
        Item::Link { path, .. } => remove_file_if_present(path),
        Item::RcBlock { file, dir } => {
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            // Re-checked at apply time: the file may have changed since the plan was shown.
            let Some(stripped) = shellrc::strip_block(&text, dir) else {
                return Ok(());
            };
            std::fs::write(file, stripped)
                .with_context(|| format!("cannot write {}", file.display()))
        }
        Item::FishFile { file } => remove_file_if_present(file),
        Item::UserPath { dir } => {
            #[cfg(windows)]
            {
                super::install::remove_user_path(dir).map(|_| ())
            }
            #[cfg(not(windows))]
            {
                let _ = dir;
                Ok(())
            }
        }
    }
}

/// Remove one file; one that vanished between the plan and the apply is already done.
fn remove_file_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("cannot remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Remove a file, a symlink (never what it points at) or a directory tree; absent is fine.
fn remove_path(path: &Path) -> Result<()> {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("cannot stat {}", path.display())),
    };
    let r = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.with_context(|| {
        format!(
            "cannot remove {} (close Atlas Desktop and running `atlas` commands, then retry)",
            path.display()
        )
    })
}

/// Windows cannot delete the running executable (this one, or `atlas hostd`), but it can move it
/// out of the way to the temp dir, where nothing refers to it.
#[cfg(windows)]
fn move_running_exe_aside(bin_dir: &Path) {
    let exe = bin_dir.join(atlas_core::paths::exe("atlas"));
    if exe.exists() {
        let aside =
            std::env::temp_dir().join(format!("atlas-uninstalled-{}.exe", std::process::id()));
        let _ = std::fs::rename(&exe, aside);
    }
}

/// Ask the running terminal host to stop before its files go. Best effort: no host, or one that
/// does not answer, is fine; its sockets are removed with the rest.
async fn stop_hostd() {
    if let Ok(c) = atlas_host::HostClient::connect_dir(&atlas_host::paths::base(), "cli").await {
        let _ = c.call("shutdown", serde_json::json!({})).await;
    }
}

/// What the confirmation said.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    No,
}

/// `[y/N]`: only an explicit yes; EOF is no.
pub(crate) fn parse_answer(line: Option<&str>) -> Answer {
    match line.map(|l| l.trim().to_ascii_lowercase()).as_deref() {
        Some("y" | "yes") => Answer::Yes,
        _ => Answer::No,
    }
}

pub async fn handle(yes: bool, purge: bool, list: bool) -> Result<()> {
    let env = Env::current()?;
    let plan = plan(&env, purge);

    println!("\x1b[1;31m◆\x1b[0m Atlas uninstaller");
    println!();
    if plan.is_empty() {
        println!("  Nothing to remove — atlas is not installed.");
        return Ok(());
    }
    plan.print(&mut std::io::stdout())?;
    println!();
    if list {
        return Ok(());
    }

    if !yes {
        // Nobody to ask: refuse instead of treating a closed stdin as consent or as a silent no.
        if !std::io::stdin().is_terminal() {
            bail!("uninstall needs confirmation: run it in a terminal, or pass --yes");
        }
        eprint!("  Continue? [y/N] ");
        let mut line = String::new();
        let n = std::io::stdin().lock().read_line(&mut line)?;
        if parse_answer((n > 0).then_some(line.as_str())) == Answer::No {
            println!("  Aborted.");
            return Ok(());
        }
        println!();
    }

    stop_hostd().await;
    #[cfg(windows)]
    move_running_exe_aside(&env.bin_dir());

    let failures = plan.apply(&mut std::io::stdout());
    println!();
    if failures.is_empty() {
        println!("  Atlas has been removed. Restart your shell.");
        Ok(())
    } else {
        bail!(
            "{} step(s) failed; the rest was removed. Close Atlas Desktop and running `atlas` commands, then run `atlas uninstall` again",
            failures.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine in a temp dir: `~/.atlas` with the files install/update/desktop leave, user data,
    /// and shell startup files holding the block install appended next to the user's own lines.
    struct Machine {
        _tmp: tempfile::TempDir,
        env: Env,
    }

    const USER_ZSHENV: &str =
        "# mine\nsource ~/.atlas/env.sh\nexport ATLAS_HOME=\"$HOME/.atlas-projects\"\n";

    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn machine() -> Machine {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let atlas = home.join(".atlas");
        let bin = atlas.join("bin");
        // What the installer made.
        write(&bin.join("atlas"), "cli");
        write(&bin.join(".atlas.prev"), "prev");
        write(&atlas.join("hostd.sock"), "");
        write(&atlas.join(".update_check"), "{}");
        write(&atlas.join("desktop/app-version"), "1.0.0");
        // What is the user's.
        write(&atlas.join("desktop/settings.json"), "{\"theme\":\"dark\"}");
        write(&atlas.join("desktop/projects.json"), "[\"/work/app\"]");
        write(&atlas.join("worktrees/app/feat/wip.txt"), "uncommitted");
        write(&atlas.join("update-channel"), "alpha\n");
        write(&atlas.join("omp-plugin/package.json"), "{}");
        // Shell files: install's block between the user's own lines.
        let zsh = format!("{USER_ZSHENV}{}", shellrc::block(&bin));
        write(&home.join(".zshenv"), &zsh);
        write(
            &home.join(".bashrc"),
            &format!("alias ll='ls -l'\n{}", shellrc::block(&bin)),
        );
        write(
            &home.join(".config/fish/conf.d/atlas.fish"),
            &shellrc::fish_contents(&bin),
        );
        let env = Env {
            atlas_home: atlas,
            shell: ShellEnv {
                home,
                zdotdir: None,
                xdg_config: None,
            },
            app: None,
        };
        Machine { _tmp: tmp, env }
    }

    fn removed_paths(p: &Plan) -> Vec<PathBuf> {
        p.remove
            .iter()
            .filter_map(|i| match i {
                Item::Path { path, .. } | Item::Link { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    /// Incident class (1.x): `atlas uninstall` deleted all of `~/.atlas` (settings, the project list and
    /// worktrees with uncommitted work) even though the docs only promised the binary and PATH lines.
    #[test]
    fn a_plain_uninstall_keeps_settings_projects_and_worktrees() {
        let m = machine();
        let p = plan(&m.env, false);
        let out = &mut Vec::new();
        assert!(p.apply(out).is_empty());
        let a = &m.env.atlas_home;
        assert!(!a.join("bin").exists(), "the CLI is gone");
        assert!(!a.join("hostd.sock").exists() && !a.join(".update_check").exists());
        assert!(!a.join("desktop/app-version").exists());
        for kept in [
            "desktop/settings.json",
            "desktop/projects.json",
            "worktrees/app/feat/wip.txt",
            "update-channel",
            "omp-plugin/package.json",
        ] {
            assert!(
                a.join(kept).exists(),
                "{kept} must survive a plain uninstall"
            );
        }
        assert!(
            p.keep.contains(&a.join("worktrees")) && p.keep.contains(&a.join("desktop")),
            "the plan says what it keeps: {:?}",
            p.keep
        );
    }

    #[test]
    fn purge_removes_the_whole_atlas_home() {
        let m = machine();
        let p = plan(&m.env, true);
        assert!(p.keep.is_empty());
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(!m.env.atlas_home.exists());
    }

    /// Incident class: the old uninstaller deleted every line containing `.atlas` from the rc files,
    /// which also removed `source ~/.atlas/env.sh` and similar lines the user wrote.
    #[test]
    fn only_the_path_block_atlas_added_leaves_the_rc_files() {
        let m = machine();
        let home = &m.env.shell.home;
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert_eq!(
            std::fs::read_to_string(home.join(".zshenv")).unwrap(),
            USER_ZSHENV
        );
        assert_eq!(
            std::fs::read_to_string(home.join(".bashrc")).unwrap(),
            "alias ll='ls -l'\n"
        );
        assert!(!home.join(".config/fish/conf.d/atlas.fish").exists());
    }

    /// Incident class: install writes `$ZDOTDIR/.zshenv`; uninstall only looked at `~/.zshenv`.
    #[test]
    fn the_path_block_in_zdotdir_is_removed_too() {
        let mut m = machine();
        let bin = m.env.atlas_home.join("bin");
        let zdot = m.env.shell.home.join("zdot");
        write(
            &zdot.join(".zshenv"),
            &format!("export Z=1\n{}", shellrc::block(&bin)),
        );
        m.env.shell.zdotdir = Some(zdot.clone());
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert_eq!(
            std::fs::read_to_string(zdot.join(".zshenv")).unwrap(),
            "export Z=1\n"
        );
    }

    /// A fish file the user wrote under the same name is not Atlas's to delete.
    #[test]
    fn a_fish_file_that_is_not_what_install_wrote_stays() {
        let m = machine();
        let fish = m.env.shell.fish_file();
        write(&fish, "function atlas\n  echo my own wrapper\nend\n");
        let p = plan(&m.env, false);
        assert!(!p.remove.iter().any(|i| matches!(i, Item::FishFile { .. })));
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(fish.exists());
    }

    /// Dry run: building the plan and printing it changes nothing.
    #[test]
    fn listing_the_plan_touches_nothing() {
        let m = machine();
        let before = snapshot(&m.env.shell.home);
        let p = plan(&m.env, true);
        let mut out = Vec::new();
        p.print(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("This will remove:") && text.contains(".atlas"),
            "{text}"
        );
        assert_eq!(snapshot(&m.env.shell.home), before);
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut v = Vec::new();
        fn walk(d: &Path, v: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, v);
                } else {
                    v.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        walk(root, &mut v);
        v.sort();
        v
    }

    #[test]
    fn an_empty_machine_has_nothing_to_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env {
            atlas_home: tmp.path().join(".atlas"),
            shell: ShellEnv {
                home: tmp.path().to_path_buf(),
                zdotdir: None,
                xdg_config: None,
            },
            app: None,
        };
        assert!(plan(&env, false).is_empty());
        assert!(plan(&env, true).is_empty());
    }

    /// Running it twice is fine: the second run finds nothing, and a path that vanished between
    /// the plan and the apply is not a failure.
    #[test]
    fn uninstall_is_idempotent() {
        let m = machine();
        let p = plan(&m.env, false);
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(p.apply(&mut Vec::new()).is_empty(), "same plan again");
        let again = plan(&m.env, false);
        assert!(again.remove.is_empty(), "{:?}", again.remove);
    }

    /// The desktop app: a real install is removed, a link (`atlas link --desktop`) only loses the
    /// link and never the dev build it points at.
    #[cfg(unix)]
    #[test]
    fn a_linked_app_loses_only_the_link() {
        let mut m = machine();
        let dev = m
            .env
            .shell
            .home
            .join("repo/target/debug/bundle/macos/Atlas.app");
        write(&dev.join("Contents/Info.plist"), "dev");
        let link = m.env.shell.home.join("Applications/Atlas.app");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&dev, &link).unwrap();
        m.env.app = Some(link.clone());
        let p = plan(&m.env, false);
        assert!(
            p.remove
                .iter()
                .any(|i| matches!(i, Item::Link { path, .. } if *path == link)),
            "{:?}",
            p.remove
        );
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(link.symlink_metadata().is_err(), "link removed");
        assert!(
            dev.join("Contents/Info.plist").exists(),
            "the dev build is untouched"
        );
    }

    #[test]
    fn an_installed_app_bundle_is_removed_whole() {
        let mut m = machine();
        let app = m.env.shell.home.join("Applications/Atlas.app");
        write(&app.join("Contents/Info.plist"), "release");
        m.env.app = Some(app.clone());
        let p = plan(&m.env, false);
        assert!(removed_paths(&p).contains(&app));
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(!app.exists());
    }

    /// One path that cannot be removed does not stop the others, and the failure is reported.
    #[cfg(unix)]
    #[test]
    fn a_failing_step_is_reported_and_the_rest_still_runs() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root ignores permissions: nothing to demonstrate
        }
        let m = machine();
        let bin = m.env.atlas_home.join("bin");
        // A read-only folder keeps its entries: `bin/atlas` cannot be unlinked.
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o555)).unwrap();
        let failures = plan(&m.env, false).apply(&mut Vec::new());
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("bin"), "{failures:?}");
        assert!(
            !m.env.atlas_home.join(".update_check").exists(),
            "the others were removed"
        );
        assert_eq!(
            std::fs::read_to_string(m.env.shell.home.join(".bashrc")).unwrap(),
            "alias ll='ls -l'\n"
        );
    }

    #[test]
    fn the_confirmation_is_an_explicit_yes_and_eof_is_no() {
        assert_eq!(parse_answer(Some("y\n")), Answer::Yes);
        assert_eq!(parse_answer(Some("YES")), Answer::Yes);
        for no in [Some(""), Some("\n"), Some("n"), Some("maybe"), None] {
            assert_eq!(parse_answer(no), Answer::No, "{no:?}");
        }
    }

    #[test]
    fn flags_parse_and_the_old_yes_still_means_skip_the_prompt() {
        use clap::Parser;
        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            u: UninstallArgs,
        }
        let a = Cli::try_parse_from(["atlas"]).unwrap().u;
        assert!(!a.yes && !a.purge && !a.list);
        let a = Cli::try_parse_from(["atlas", "-y", "--purge", "--dry-run"])
            .unwrap()
            .u;
        assert!(a.yes && a.purge && a.list);
    }

    /// Linux: the AppImage's launcher entry and icon go with it (the docs used to tell people to
    /// delete them by hand).
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_app_takes_its_menu_entry_and_icon_with_it() {
        let mut m = machine();
        let home = m.env.shell.home.clone();
        let app = home.join(".local/bin/atlas-desktop");
        write(&app, "appimage");
        let entry = home.join(".local/share/applications/br.com.codeatlas.atlas.desktop");
        let icon = home.join(".local/share/icons/hicolor/128x128/apps/br.com.codeatlas.atlas.png");
        let other = home.join(".local/share/applications/firefox.desktop");
        for f in [&entry, &icon, &other] {
            write(f, "x");
        }
        m.env.app = Some(app.clone());
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert!(!app.exists() && !entry.exists() && !icon.exists());
        assert!(other.exists(), "another app's launcher is never touched");
    }

    /// A distro package owns `/usr/bin/atlas-desktop`: it is never `env.app`, so nothing of it is
    /// planned; and an app that is not installed plans nothing for it.
    #[test]
    fn no_app_means_nothing_planned_for_it() {
        let m = machine();
        let p = plan(&m.env, true);
        assert!(
            !p.remove
                .iter()
                .any(|i| i.describe().contains("Atlas Desktop")),
            "{:?}",
            p.remove
        );
    }
}
