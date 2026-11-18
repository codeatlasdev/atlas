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

use atlas_core::pkgowner::{self, Package};
use atlas_i18n::t;

use super::AtlasCommand;
use super::shellrc::{self, ShellEnv};
use super::update_pkg;

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
    /// Windows: the install dir is on the user's `Path` (registry). Always false elsewhere, where
    /// the shell files carry the `PATH` lines. Only a dir that is there is planned for removal.
    pub user_path: bool,
    /// fish is installed: its universal variables are worth asking about (tests leave it off).
    pub fish: bool,
    /// `$XDG_DATA_HOME` (`~/.local/share`): where the WebKit store and crash reports live.
    pub data_home: PathBuf,
    /// Linux: the distro packages that own this `atlas` and `atlas-desktop`. Their package
    /// manager owns the files, so uninstall removes the package, never the files.
    pub packages: Vec<Package>,
}

impl Env {
    pub(crate) fn current() -> Result<Self> {
        let atlas_home = atlas_core::paths::atlas_home();
        #[cfg(windows)]
        let user_path = super::install::has_user_path(&atlas_home.join("bin")).unwrap_or(false);
        #[cfg(not(windows))]
        let user_path = false;
        let shell = ShellEnv::current();
        Ok(Self {
            atlas_home,
            data_home: dirs::data_local_dir().unwrap_or_else(|| shell.home.join(".local/share")),
            packages: installed_packages(&shell.home),
            shell,
            app: atlas_core::paths::installed_app(),
            user_path,
            fish: atlas_core::paths::which("fish").is_some(),
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
    Path { path: PathBuf, note: Note },
    /// A symlink (`atlas link`): only the link goes, never what it points at.
    Link { path: PathBuf, note: Note },
    /// The `# Atlas CLI` block install appended to a shell startup file.
    RcBlock { file: PathBuf, dir: PathBuf },
    /// A startup file all of Atlas's own (fish `conf.d/atlas.fish`, nushell `autoload/atlas.nu`).
    ShellFile { file: PathBuf },
    /// `dir` in fish's universal `fish_user_paths` (the fish line before 1.5.0-alpha.26 put it there).
    FishUniversal { dir: PathBuf, shell: ShellEnv },
    /// Windows: the install dir on the user's `Path`.
    UserPath { dir: PathBuf },
    /// Linux: a distro package, removed by its package manager (root needed).
    Package(Package),
}

impl Item {
    fn describe(&self) -> String {
        match self {
            Item::Path { path, note } | Item::Link { path, note } => {
                format!("{}  ({})", path.display(), note.text())
            }
            Item::RcBlock { file, .. } => {
                t!(
                    "cmd_install_item_rc_block",
                    path = file.display().to_string()
                )
            }
            Item::ShellFile { file } => {
                t!(
                    "cmd_install_item_shell_file",
                    path = file.display().to_string()
                )
            }
            Item::FishUniversal { dir, .. } => {
                t!(
                    "cmd_install_item_fish_universal",
                    path = dir.display().to_string()
                )
            }
            Item::UserPath { dir } => {
                t!(
                    "cmd_install_item_user_path",
                    path = dir.display().to_string()
                )
            }
            Item::Package(p) => t!(
                "cmd_install_item_package",
                name = p.name.clone(),
                command = p.manual_remove_command()
            ),
        }
    }
}

/// What a path in the plan is, in words: the `--list` output and the confirmation show it after the path. One
/// catalog message each (`cmd_install_note_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Note {
    AppLinked,
    App,
    LinkOnPath,
    Everything,
    Cli,
    HostSocket,
    HostToken,
    HostPid,
    HostLog,
    DesktopSocket,
    DesktopToken,
    DesktopDebugSocket,
    UpdateCheck,
    UpdateLock,
    RemoteCache,
    AppBackup,
    AppVersion,
    MenuEntry,
    MenuIcon,
    PrevApp,
    StagedApp,
    WebkitStore,
    WebkitStoreLegacy,
    CrashReports,
    WindowState,
}

impl Note {
    fn text(self) -> String {
        match self {
            Note::AppLinked => t!("cmd_install_note_app_linked"),
            Note::App => t!("cmd_install_note_app"),
            Note::LinkOnPath => t!("cmd_install_note_link_on_path"),
            Note::Everything => t!("cmd_install_note_everything"),
            Note::Cli => t!("cmd_install_note_cli"),
            Note::HostSocket => t!("cmd_install_note_host_socket"),
            Note::HostToken => t!("cmd_install_note_host_token"),
            Note::HostPid => t!("cmd_install_note_host_pid"),
            Note::HostLog => t!("cmd_install_note_host_log"),
            Note::DesktopSocket => t!("cmd_install_note_desktop_socket"),
            Note::DesktopToken => t!("cmd_install_note_desktop_token"),
            Note::DesktopDebugSocket => t!("cmd_install_note_desktop_debug_socket"),
            Note::UpdateCheck => t!("cmd_install_note_update_check"),
            Note::UpdateLock => t!("cmd_install_note_update_lock"),
            Note::RemoteCache => t!("cmd_install_note_remote_cache"),
            Note::AppBackup => t!("cmd_install_note_app_backup"),
            Note::AppVersion => t!("cmd_install_note_app_version"),
            Note::MenuEntry => t!("cmd_install_note_menu_entry"),
            Note::MenuIcon => t!("cmd_install_note_menu_icon"),
            Note::PrevApp => t!("cmd_install_note_prev_app"),
            Note::StagedApp => t!("cmd_install_note_staged_app"),
            Note::WebkitStore => t!("cmd_install_note_webkit_store"),
            Note::WebkitStoreLegacy => t!("cmd_install_note_webkit_store_legacy"),
            Note::CrashReports => t!("cmd_install_note_crash_reports"),
            Note::WindowState => t!("cmd_install_note_window_state"),
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
const OWNED: &[(&str, Note)] = &[
    ("bin", Note::Cli),
    ("hostd.sock", Note::HostSocket),
    ("hostd.token", Note::HostToken),
    ("hostd.pid", Note::HostPid),
    ("hostd.log", Note::HostLog),
    ("desktop.sock", Note::DesktopSocket),
    ("desktop.token", Note::DesktopToken),
    ("desktop-debug.sock", Note::DesktopDebugSocket),
    (".update_check", Note::UpdateCheck),
    (".update.lock", Note::UpdateLock),
    ("remote", Note::RemoteCache),
    ("desktop/Atlas.app.backup", Note::AppBackup),
    ("desktop/appimage-version", Note::AppVersion),
    ("desktop/app-version", Note::AppVersion),
];

/// The program a desktop entry's `Exec=` starts: the quoted path `atlas desktop` writes (`"`,
/// `` ` ``, `$` and `\` escaped with a backslash), or else the first word.
fn entry_exec(text: &str) -> Option<PathBuf> {
    let rest = text
        .lines()
        .find_map(|l| l.strip_prefix("Exec="))?
        .trim_start();
    let Some(quoted) = rest.strip_prefix('"') else {
        return rest.split_whitespace().next().map(PathBuf::from);
    };
    let mut path = String::new();
    let mut chars = quoted.chars();
    loop {
        match chars.next()? {
            '"' => return Some(path.into()),
            '\\' => path.push(chars.next()?),
            c => path.push(c),
        }
    }
}

/// Linux files `atlas desktop` and the app leave outside `~/.atlas`, planned whether or not the
/// AppImage is still there (a hand-deleted one used to hide all of them).
fn plan_linux(env: &Env, purge: bool, p: &mut Plan) {
    use atlas_core::paths::{
        LINUX_APP_PREV, LINUX_APP_STAGING, LINUX_LEGACY_MENU_ID, LINUX_MENU_ID, linux_app,
        linux_menu_files,
    };
    if !cfg!(target_os = "linux") {
        return;
    }
    let exists = |path: &Path| path.symlink_metadata().is_ok();
    let home = &env.shell.home;
    let app = linux_app(home);

    // The launcher entry and icon, when the entry starts our app (one that starts something else
    // is not ours, nor is the icon beside it).
    for id in [LINUX_MENU_ID, LINUX_LEGACY_MENU_ID] {
        let (entry, icon) = linux_menu_files(home, id);
        let ours = match std::fs::read_to_string(&entry) {
            Ok(text) => entry_exec(&text).is_some_and(|exec| exec == app),
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        };
        if !ours {
            continue;
        }
        for (path, note) in [(entry, Note::MenuEntry), (icon, Note::MenuIcon)] {
            if exists(&path) {
                p.remove.push(Item::Path { path, note });
            }
        }
    }

    // What an update killed half way leaves beside the AppImage.
    for (name, note) in [
        (LINUX_APP_PREV, Note::PrevApp),
        (LINUX_APP_STAGING, Note::StagedApp),
    ] {
        let path = app.with_file_name(name);
        if exists(&path) {
            p.remove.push(Item::Path { path, note });
        }
    }

    // What the app keeps for itself: the WebKit store (its UI settings; the pre-1.6 id's copy
    // too), crash reports and window state. The user's, so a plain uninstall keeps and lists them.
    let config = env
        .shell
        .xdg_config
        .clone()
        .unwrap_or_else(|| home.join(".config"));
    for (path, note) in [
        (env.data_home.join(LINUX_MENU_ID), Note::WebkitStore),
        (
            env.data_home.join(LINUX_LEGACY_MENU_ID),
            Note::WebkitStoreLegacy,
        ),
        (env.data_home.join("atlas"), Note::CrashReports),
        (config.join(LINUX_MENU_ID), Note::WindowState),
    ] {
        if !exists(&path) {
            continue;
        }
        if purge {
            p.remove.push(Item::Path { path, note });
        } else {
            p.keep.push(path);
        }
    }
}

/// Plan the uninstall for `env`.
pub(crate) fn plan(env: &Env, purge: bool) -> Plan {
    let mut p = Plan::default();
    let exists = |path: &Path| path.symlink_metadata().is_ok();

    // The desktop app: a linked one is only unlinked. A distro package's app is never `env.app`
    // (its package manager owns it: see `env.packages`), and on Windows the app has its own
    // uninstaller (Settings › Apps, `Atlas-Setup.exe --uninstall`) that also clears the registry;
    // deleting its exe here would leave that entry pointing at nothing.
    if let Some(app) = env.app.as_ref().filter(|a| !cfg!(windows) && exists(a)) {
        if super::desktop::is_symlink(app) {
            p.remove.push(Item::Link {
                path: app.clone(),
                note: Note::AppLinked,
            });
        } else {
            p.remove.push(Item::Path {
                path: app.clone(),
                note: Note::App,
            });
        }
    }

    // Shell startup lines: only the exact block install wrote, in whichever candidate has it.
    let bin = env.bin_dir();
    if env.user_path {
        p.remove.push(Item::UserPath { dir: bin.clone() });
    }
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
    let owned = [
        (
            env.shell.fish_file(),
            shellrc::is_fish_file_ours as fn(&str, &Path) -> bool,
        ),
        (env.shell.nu_file(), shellrc::is_nu_file_ours),
    ];
    for (file, ours) in owned {
        if std::fs::read_to_string(&file).is_ok_and(|c| ours(&c, &bin)) {
            p.remove.push(Item::ShellFile { file });
        }
    }
    if env.fish && shellrc::fish_universal_has(&env.shell, &bin) {
        p.remove.push(Item::FishUniversal {
            dir: bin.clone(),
            shell: env.shell.clone(),
        });
    }
    // `~/.local/bin/atlas`, only when it is the link install made to this install's binary.
    let link = env.shell.local_bin_link();
    if std::fs::read_link(&link).is_ok_and(|to| to == bin.join(atlas_core::paths::exe("atlas"))) {
        p.remove.push(Item::Link {
            path: link,
            note: Note::LinkOnPath,
        });
    }

    // `~/.atlas`: all of it, or only what Atlas itself put there.
    if purge {
        if exists(&env.atlas_home) {
            p.remove.push(Item::Path {
                path: env.atlas_home.clone(),
                note: Note::Everything,
            });
        }
    } else {
        for &(rel, note) in OWNED {
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
    plan_linux(env, purge, &mut p);
    // Last, so no file removal waits behind the package manager's password prompt.
    p.remove
        .extend(env.packages.iter().cloned().map(Item::Package));
    p
}

impl Plan {
    pub(crate) fn is_empty(&self) -> bool {
        self.remove.is_empty()
    }

    /// Print the plan the way `--list` and the confirmation show it.
    pub(crate) fn print(&self, out: &mut impl Write) -> std::io::Result<()> {
        writeln!(out, "  {}", t!("cmd_install_plan_remove"))?;
        for item in &self.remove {
            writeln!(out, "    {}", item.describe())?;
        }
        if !self.keep.is_empty() {
            writeln!(out, "  {}", t!("cmd_install_plan_keep"))?;
            for k in &self.keep {
                writeln!(out, "    {}", k.display())?;
            }
        }
        Ok(())
    }

    /// Carry the plan out. Keeps going after a step fails and returns every failure, so one
    /// stubborn file never leaves the rest of Atlas half removed without saying so.
    pub(crate) fn apply(&self, out: &mut impl Write) -> Vec<String> {
        self.apply_with(out, &mut remove_package)
    }

    /// [`Plan::apply`] with the package removal given, so a test never runs a package manager.
    pub(crate) fn apply_with(
        &self,
        out: &mut impl Write,
        remove_pkg: &mut dyn FnMut(&Package) -> Result<()>,
    ) -> Vec<String> {
        let mut failures = Vec::new();
        for item in &self.remove {
            let done = apply_item(item, remove_pkg);
            match done {
                Ok(()) => {
                    let msg = t!("cmd_install_removed", item = item.describe());
                    let _ = writeln!(out, "\x1b[32m✓\x1b[0m {msg}");
                }
                Err(e) => {
                    let msg = t!(
                        "cmd_install_failed",
                        item = item.describe(),
                        reason = format!("{e:#}")
                    );
                    let _ = writeln!(out, "\x1b[31m✗\x1b[0m {msg}");
                    failures.push(format!("{}: {e:#}", item.describe()));
                }
            }
        }
        failures
    }
}

fn apply_item(item: &Item, remove_pkg: &mut dyn FnMut(&Package) -> Result<()>) -> Result<()> {
    match item {
        Item::Path { path, .. } => remove_path(path),
        Item::Link { path, .. } => remove_file_if_present(path),
        Item::Package(p) => remove_pkg(p),
        Item::RcBlock { file, dir } => {
            let text = std::fs::read_to_string(file).with_context(|| {
                t!("cmd_install_read_failed", path = file.display().to_string())
            })?;
            // Re-checked at apply time: the file may have changed since the plan was shown.
            let Some(stripped) = shellrc::strip_block(&text, dir) else {
                return Ok(());
            };
            std::fs::write(file, stripped).with_context(|| {
                t!(
                    "cmd_install_write_failed",
                    path = file.display().to_string()
                )
            })
        }
        Item::ShellFile { file } => remove_file_if_present(file),
        Item::FishUniversal { dir, shell } => {
            shellrc::fish_forget_universal(shell, dir).map(|_| ())
        }
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

/// Run the package manager's removal as root: `sudo` with a terminal, `pkexec` without one. The
/// plan was shown and confirmed, so nothing asks again; where nothing can become root the
/// failure says the command to run by hand.
fn remove_package(pkg: &Package) -> Result<()> {
    let escalation = match update_pkg::plan(
        update_pkg::is_root(),
        std::io::stdin().is_terminal(),
        true,
        |n| atlas_core::paths::which(n).is_some(),
    ) {
        Ok(update_pkg::Plan::Install(e)) => e,
        _ => bail!(
            "{}",
            atlas_i18n::t!(
                "uninstall_cannot_become_root",
                command = pkg.manual_remove_command()
            )
        ),
    };
    update_pkg::run_install(&update_pkg::escalated(escalation, pkg.remove_argv()))
}

/// Linux: the packages that own the running `atlas` and `/usr/bin/atlas-desktop`. Asked only for
/// files outside `home` (an install under `~/.atlas` or `~/.local/bin` has no package), and only
/// for files that are ours: the running binary, and the app by its own name.
fn installed_packages(home: &Path) -> Vec<Package> {
    let mut found: Vec<Package> = Vec::new();
    if !cfg!(target_os = "linux") {
        return found;
    }
    let files = [
        Some(super::update_core::judged_exe()),
        atlas_core::paths::packaged_app(),
    ];
    for file in files.into_iter().flatten().filter(|f| !f.starts_with(home)) {
        if let Some(p) = pkgowner::package(&file)
            && !found.contains(&p)
        {
            found.push(p);
        }
    }
    found
}

/// Remove one file; one that vanished between the plan and the apply is already done.
fn remove_file_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| {
            t!(
                "cmd_install_remove_failed",
                path = path.display().to_string()
            )
        }),
        _ => Ok(()),
    }
}

/// Remove a file, a symlink (never what it points at) or a directory tree; absent is fine.
fn remove_path(path: &Path) -> Result<()> {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| {
                t!(
                    "cmd_install_inspect_failed",
                    path = path.display().to_string()
                )
            });
        }
    };
    let r = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.with_context(|| t!("cmd_install_remove_busy", path = path.display().to_string()))
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

/// `[y/N]` (`[s/N]` in pt-BR): only an explicit yes in the language of the prompt or in English; EOF is no.
pub(crate) fn parse_answer(line: Option<&str>) -> Answer {
    match line.map(|l| l.trim().to_lowercase()).as_deref() {
        Some("y" | "yes" | "s" | "sim") => Answer::Yes,
        _ => Answer::No,
    }
}

pub async fn handle(yes: bool, purge: bool, list: bool) -> Result<()> {
    let env = Env::current()?;
    let plan = plan(&env, purge);

    println!("\x1b[1;31m◆\x1b[0m {}", t!("cmd_install_uninstall_title"));
    println!();
    if plan.is_empty() {
        println!("  {}", t!("cmd_install_uninstall_nothing"));
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
            bail!("{}", t!("cmd_install_uninstall_needs_confirmation"));
        }
        eprint!("  {} ", t!("cmd_install_uninstall_confirm"));
        let mut line = String::new();
        let n = std::io::stdin().lock().read_line(&mut line)?;
        if parse_answer((n > 0).then_some(line.as_str())) == Answer::No {
            println!("  {}", t!("cmd_install_uninstall_aborted"));
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
        println!("  {}", t!("cmd_install_uninstall_done"));
        Ok(())
    } else {
        bail!(
            "{}",
            t!("cmd_install_uninstall_steps_failed", count = failures.len())
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
        let shell = ShellEnv {
            home: home.clone(),
            zdotdir: None,
            xdg_config: None,
        };
        write(&shell.nu_file(), &shellrc::nu_contents(&bin));
        #[cfg(unix)]
        {
            std::fs::create_dir_all(home.join(".local/bin")).unwrap();
            std::os::unix::fs::symlink(bin.join("atlas"), home.join(".local/bin/atlas")).unwrap();
        }
        let env = Env {
            atlas_home: atlas,
            shell,
            app: None,
            user_path: false,
            fish: false,
            data_home: home.join(".local/share"),
            packages: Vec::new(),
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
        assert!(!m.env.shell.nu_file().exists());
        // `machine()` makes the `~/.local/bin/atlas` link on Unix only.
        #[cfg(unix)]
        {
            assert!(
                home.join(".local/bin/atlas").symlink_metadata().is_err(),
                "the link install made is gone"
            );
            assert!(home.join(".local/bin").is_dir(), "the user's dir stays");
        }
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
        assert!(!p.remove.contains(&Item::ShellFile { file: fish.clone() }));
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(fish.exists());
    }

    /// Up to 1.5.0-alpha.25 the fish file held `fish_add_path <dir>` (universal, unquoted): still
    /// Atlas's file, so uninstall deletes it instead of leaving `atlas` on PATH.
    #[test]
    fn the_fish_file_an_older_install_wrote_is_removed() {
        let m = machine();
        let bin = m.env.atlas_home.join("bin");
        let fish = m.env.shell.fish_file();
        write(
            &fish,
            &format!("# Atlas CLI\nfish_add_path {}\n", bin.display()),
        );
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert!(!fish.exists());
    }

    /// `~/.local/bin/atlas` that is not the link install made (the user's own script, or a link to
    /// another binary) is never removed.
    #[test]
    #[cfg(unix)]
    fn a_local_bin_atlas_that_is_not_our_link_stays() {
        let m = machine();
        let link = m.env.shell.local_bin_link();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("/opt/other/atlas", &link).unwrap();
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert!(link.symlink_metadata().is_ok());
        std::fs::remove_file(&link).unwrap();
        write(&link, "#!/bin/sh\necho mine\n");
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "#!/bin/sh\necho mine\n"
        );
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
            text.contains(&t!("cmd_install_plan_remove")) && text.contains(".atlas"),
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
            user_path: false,
            fish: false,
            data_home: tmp.path().join(".local/share"),
            packages: Vec::new(),
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

    // Windows plans no app (see `on_windows_the_app_is_left_to_its_own_uninstaller`).
    #[cfg(not(windows))]
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
        assert_eq!(parse_answer(Some("s\n")), Answer::Yes);
        assert_eq!(parse_answer(Some("Sim")), Answer::Yes);
        for no in [Some(""), Some("\n"), Some("n"), Some("maybe"), None] {
            assert_eq!(parse_answer(no), Answer::No, "{no:?}");
        }
    }

    /// The prompt shows `[y/N]` in English and `[s/N]` in pt-BR: the letter it offers as "yes" must be one
    /// `parse_answer` accepts, in every language, or a translated prompt would refuse its own answer.
    #[test]
    fn the_yes_a_prompt_offers_is_the_yes_it_accepts() {
        // The pseudo-locale (en-XA, dev builds) accents every letter: it is a layout aid, not a language to answer in.
        for tag in atlas_i18n::locales().filter(|t| *t != "en-XA") {
            let prompt = atlas_i18n::t_in!(tag, "cmd_install_uninstall_confirm");
            let yes = prompt
                .split_once('[')
                .and_then(|(_, rest)| rest.split_once('/'))
                .map(|(yes, _)| yes.to_owned())
                .unwrap_or_else(|| panic!("{tag}: no [y/N] in {prompt:?}"));
            assert_eq!(parse_answer(Some(&yes)), Answer::Yes, "{tag}: {prompt}");
            assert_eq!(
                parse_answer(Some(&yes.to_lowercase())),
                Answer::Yes,
                "{tag}"
            );
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
        let (entry, icon) = write_launcher(&m, atlas_core::paths::LINUX_MENU_ID, &app);
        let other = home.join(".local/share/applications/firefox.desktop");
        write(&other, "x");
        m.env.app = Some(app.clone());
        assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
        assert!(!app.exists() && !entry.exists() && !icon.exists());
        assert!(other.exists(), "another app's launcher is never touched");
    }

    /// The reader of `Exec=` is the inverse of the writer: spaces, quotes, `$`, backticks and
    /// backslashes in the app's path come back as the same path (else a user with such a home
    /// keeps a dead launcher entry).
    #[cfg(target_os = "linux")]
    #[test]
    fn the_exec_of_a_launcher_entry_reads_back_as_the_path_it_was_written_with() {
        for path in [
            "/home/u/.local/bin/atlas-desktop",
            "/home/a b/.local/bin/atlas-desktop",
            "/h/$x\"`y\\z/atlas-desktop",
        ] {
            let text = super::super::desktop_linux::desktop_entry(Path::new(path));
            assert_eq!(entry_exec(&text), Some(PathBuf::from(path)), "{text}");
        }
        assert_eq!(entry_exec("[Desktop Entry]\nName=x\n"), None);
        assert_eq!(
            entry_exec("Exec=/usr/bin/atlas-desktop %U\n"),
            Some(PathBuf::from("/usr/bin/atlas-desktop"))
        );
    }

    #[cfg(target_os = "linux")]
    fn our_app(m: &Machine) -> PathBuf {
        atlas_core::paths::linux_app(&m.env.shell.home)
    }

    /// What `atlas desktop` wrote: the entry (from the real writer) and the icon, for `id`.
    #[cfg(target_os = "linux")]
    fn write_launcher(m: &Machine, id: &str, exec: &Path) -> (PathBuf, PathBuf) {
        let (entry, icon) = atlas_core::paths::linux_menu_files(&m.env.shell.home, id);
        write(&entry, &super::super::desktop_linux::desktop_entry(exec));
        write(&icon, "png");
        (entry, icon)
    }

    /// #277: the launcher files were planned only inside `if let Some(app) = env.app`, so an
    /// AppImage deleted by hand left the menu entry (a dead "Atlas" in the launcher) and its icon.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_launcher_entry_and_icon_go_even_when_the_appimage_is_already_gone() {
        let m = machine();
        let app = our_app(&m);
        assert!(!app.exists() && m.env.app.is_none());
        let current = write_launcher(&m, atlas_core::paths::LINUX_MENU_ID, &app);
        let legacy = write_launcher(&m, atlas_core::paths::LINUX_LEGACY_MENU_ID, &app);
        let p = plan(&m.env, false);
        assert!(p.apply(&mut Vec::new()).is_empty());
        for f in [&current.0, &current.1, &legacy.0, &legacy.1] {
            assert!(!f.exists(), "{} survived", f.display());
        }
        assert!(plan(&m.env, false).is_empty(), "nothing is left to plan");
    }

    /// The entry is Atlas's only when it starts Atlas's app: one that starts another program (a
    /// dev build, a user's own launcher under that id) is not ours to delete, nor is its icon.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_launcher_entry_that_starts_another_program_is_left_alone() {
        let m = machine();
        let (entry, icon) = write_launcher(
            &m,
            atlas_core::paths::LINUX_MENU_ID,
            Path::new("/opt/mine/atlas-desktop"),
        );
        let p = plan(&m.env, true);
        assert!(p.apply(&mut Vec::new()).is_empty());
        assert!(entry.exists() && icon.exists());
    }

    /// #277: an update killed half way leaves `.atlas-desktop.prev` (the old app) or
    /// `.atlas-desktop.new` (the staged one) beside the AppImage, whether or not the AppImage
    /// itself is still there.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_previous_and_staged_appimage_files_are_removed() {
        for app_present in [true, false] {
            let mut m = machine();
            let app = our_app(&m);
            let dir = app.parent().unwrap().to_path_buf();
            let (prev, staged) = (
                dir.join(atlas_core::paths::LINUX_APP_PREV),
                dir.join(atlas_core::paths::LINUX_APP_STAGING),
            );
            let neighbour = dir.join("atlas-desktop-notes.txt");
            for f in [&prev, &staged, &neighbour] {
                write(f, "x");
            }
            if app_present {
                write(&app, "appimage");
                m.env.app = Some(app.clone());
            }
            assert!(plan(&m.env, false).apply(&mut Vec::new()).is_empty());
            assert!(
                !prev.exists() && !staged.exists(),
                "app_present={app_present}"
            );
            assert!(neighbour.exists(), "files that are not Atlas's stay");
        }
    }

    /// #277: the WebKitGTK store (`<data>/br.com.codeatlas.atlas`, the pre-1.6 id's copy), the
    /// crash reports (`<data>/atlas/crashes`) and the window state (`<config>/<id>`) were in no
    /// plan. A plain uninstall keeps and lists them (they are the user's UI settings and reports);
    /// `--purge` removes them from `$XDG_DATA_HOME` / `$XDG_CONFIG_HOME` wherever those point.
    #[cfg(target_os = "linux")]
    #[test]
    fn purge_takes_the_webkit_store_crash_reports_and_window_state() {
        let mut m = machine();
        let home = m.env.shell.home.clone();
        m.env.data_home = home.join("xdg-data");
        m.env.shell.xdg_config = Some(home.join("xdg-config"));
        let data = m.env.data_home.clone();
        let ours = [
            data.join("br.com.codeatlas.atlas/localstorage/x"),
            data.join("dev.codeatlas.atlas/localstorage/x"),
            data.join("atlas/crashes/crash-1.toml"),
            home.join("xdg-config/br.com.codeatlas.atlas/.window-state.json"),
        ];
        let theirs = [
            data.join("br.com.codeatlas.atlas.dev/x"),
            data.join("other-app/x"),
            home.join(".local/share/br.com.codeatlas.atlas/x"),
        ];
        for f in ours.iter().chain(&theirs) {
            write(f, "x");
        }

        let plain = plan(&m.env, false);
        let mut listing = Vec::new();
        plain.print(&mut listing).unwrap();
        let listing = String::from_utf8(listing).unwrap();
        assert!(plain.apply(&mut Vec::new()).is_empty());
        for f in &ours {
            assert!(f.exists(), "{} must survive a plain uninstall", f.display());
        }
        for dir in ["br.com.codeatlas.atlas", "atlas"] {
            assert!(
                listing.contains(&data.join(dir).display().to_string()),
                "the plan says it keeps {dir}: {listing}"
            );
        }

        let p = plan(&m.env, true);
        assert!(p.keep.is_empty());
        assert!(p.apply(&mut Vec::new()).is_empty());
        for f in &ours {
            assert!(!f.exists(), "{} survived --purge", f.display());
        }
        for f in &theirs {
            assert!(f.exists(), "{} is not Atlas's", f.display());
        }
    }

    /// #277: a package-owned `/usr/bin/atlas` was silently skipped. It is planned as the
    /// package's own removal (`pacman -R codeatlas-atlas-bin`, shown by `--list`), run by the
    /// package manager, last so no file removal waits behind a password prompt; a removal that
    /// cannot run is reported with the command to run by hand and everything else still goes.
    #[test]
    fn a_package_install_is_removed_by_its_package_manager() {
        use atlas_core::pkgowner::PackageManager;
        let mut m = machine();
        let pkg = Package {
            manager: PackageManager::Pacman,
            name: "codeatlas-atlas-bin".into(),
        };
        m.env.packages = vec![pkg.clone()];
        let p = plan(&m.env, false);
        assert_eq!(
            p.remove.last(),
            Some(&Item::Package(pkg.clone())),
            "{:?}",
            p.remove
        );
        let mut listing = Vec::new();
        p.print(&mut listing).unwrap();
        assert!(
            String::from_utf8(listing)
                .unwrap()
                .contains("sudo pacman -R codeatlas-atlas-bin")
        );

        let mut ran = Vec::new();
        let failures = p.apply_with(&mut Vec::new(), &mut |p: &Package| {
            ran.push(p.remove_argv());
            Ok(())
        });
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(ran, [pkg.remove_argv()]);
        assert!(!m.env.atlas_home.join("bin").exists(), "user files go too");

        let m = {
            let mut m = machine();
            m.env.packages = vec![pkg];
            m
        };
        let failures = plan(&m.env, false).apply_with(&mut Vec::new(), &mut |p: &Package| {
            anyhow::bail!("run: {}", p.manual_remove_command())
        });
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(
            failures[0].contains("sudo pacman -R codeatlas-atlas-bin"),
            "{failures:?}"
        );
        assert!(!m.env.atlas_home.join("bin").exists(), "the rest still ran");
    }

    /// A machine with only the package (nothing under `~`) still has something to remove.
    #[test]
    fn a_package_alone_makes_the_plan_non_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mut env = Env {
            atlas_home: tmp.path().join(".atlas"),
            shell: ShellEnv {
                home: tmp.path().to_path_buf(),
                zdotdir: None,
                xdg_config: None,
            },
            app: None,
            user_path: false,
            fish: false,
            data_home: tmp.path().join(".local/share"),
            packages: Vec::new(),
        };
        assert!(plan(&env, true).is_empty());
        env.packages = vec![Package {
            manager: atlas_core::pkgowner::PackageManager::Apt,
            name: "atlas".into(),
        }];
        assert_eq!(plan(&env, true).remove.len(), 1);
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

    /// Windows: the app belongs to its own uninstaller (Settings › Apps), which also clears the
    /// registry; `atlas uninstall` must not delete its exe and leave that entry dangling.
    #[cfg(windows)]
    #[test]
    fn on_windows_the_app_is_left_to_its_own_uninstaller() {
        let mut m = machine();
        let app = m
            .env
            .shell
            .home
            .join("AppData/Local/Atlas/atlas-desktop.exe");
        write(&app, "exe");
        m.env.app = Some(app.clone());
        let p = plan(&m.env, false);
        assert!(!removed_paths(&p).contains(&app), "{:?}", p.remove);
    }

    /// Incident class (Windows CI): the user-`Path` removal was planned unconditionally, so an
    /// empty machine "had something to remove" and a second run never finished empty. It is planned
    /// only when the install dir really is on the user's `Path`, and the plan is empty again after.
    #[test]
    fn the_user_path_entry_is_planned_only_when_it_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        let mut env = Env {
            atlas_home: tmp.path().join(".atlas"),
            shell: ShellEnv {
                home: tmp.path().to_path_buf(),
                zdotdir: None,
                xdg_config: None,
            },
            app: None,
            user_path: false,
            fish: false,
            data_home: tmp.path().join(".local/share"),
            packages: Vec::new(),
        };
        assert!(plan(&env, false).is_empty());
        env.user_path = true;
        let p = plan(&env, false);
        assert_eq!(
            p.remove,
            [Item::UserPath {
                dir: env.atlas_home.join("bin")
            }]
        );
    }
}
