//! The environment every child Atlas spawns runs with: the user's *login shell* environment.
//!
//! A desktop/menu launch (Finder/Dock on macOS, a `.desktop` entry on Linux) hands the app the
//! session's bare environment (`PATH=/usr/bin:/bin`, no `~/.bun/bin`, nvm, mise, cargo…), because the
//! user's shell rc files were never read. Every service, `pre_command`, action and pty then dies with
//! `command not found`. We ask the user's own shell for its environment once and give it to every child.
//!
//! Shell choice (see [`resolve_shell`]): an explicit override (Atlas Desktop's Settings), else `$SHELL`,
//! else the account's login shell from the user database (`getpwuid`), else `/bin/sh`. The probe runs the
//! shell as an interactive login shell (`-ilc`, because bash/zsh/fish read different rc files for login
//! and interactive shells, and many installers only touch one), degrading to `-lc` and `-c` for shells
//! that reject the combined flags (csh/tcsh). The only things the shell must run are `echo` and the
//! external `env -0`; rc output around the markers is ignored. Commands themselves stay `sh -c`.
//!
//! When the probe fails (timeout, restricted shell, broken rc) [`fallback_path`] prepends the well-known
//! user bin directories so `bun`/`cargo`/`node` managers still resolve, and [`Probe::error`] says so.
//!
//! Windows has nothing to probe: every app gets the user's environment (the registry's user and system
//! `Path`) however it was started, so the probe is our own environment.

#[cfg(unix)]
use std::ffi::CStr;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

const MARK: &str = "__ATLAS_ENV__";
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(6);
const FLAG_SETS: [&[&str]; 3] = [&["-l", "-i", "-c"], &["-l", "-c"], &["-c"]];
/// Per-session/terminal vars that must not leak into children.
const SKIP: [&str; 6] = [
    "PWD",
    "OLDPWD",
    "SHLVL",
    "_",
    "TERM_SESSION_ID",
    "TERM_PROGRAM",
];

/// Outcome of one probe of the login shell.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Probe {
    pub shell: String,
    /// `settings` | `$SHELL` | `account` | `default`
    pub source: &'static str,
    /// Flags that worked, e.g. `-l -i -c`.
    pub flags: Option<String>,
    /// Why the probe failed; the env is then the inherited one plus [`fallback_path`].
    pub error: Option<String>,
    pub took_ms: u64,
    #[serde(skip)]
    pub vars: Vec<(String, String)>,
}

impl Probe {
    /// The `PATH` children run with. Windows names are case-insensitive and its own spelling is `Path`.
    pub fn path(&self) -> Option<&str> {
        self.vars
            .iter()
            .find(|(k, _)| is_path_key(k))
            .map(|(_, v)| v.as_str())
    }
}

fn is_path_key(k: &str) -> bool {
    if cfg!(windows) {
        k.eq_ignore_ascii_case("PATH")
    } else {
        k == "PATH"
    }
}

/// The cached probe; the first access runs it (a few seconds at most), so async callers use
/// `spawn_blocking`.
static CURRENT: LazyLock<Mutex<Probe>> = LazyLock::new(|| Mutex::new(run()));
static OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

/// Set the shell override (Atlas Desktop's Settings; `None` = automatic). Takes effect on the next
/// [`reprobe`].
pub fn set_override(shell: Option<String>) {
    *OVERRIDE.lock() = shell;
}

/// The cached probe, running it on first use.
pub fn current() -> Probe {
    CURRENT.lock().clone()
}

/// Probe again now (after the override changed) and cache the result.
pub fn reprobe() -> Probe {
    let p = run();
    *CURRENT.lock() = p.clone();
    p
}

/// Replace the cached probe (tests of spawn paths seed a probe of a fake HOME; the real app never calls
/// this).
pub fn install(p: Probe) {
    *CURRENT.lock() = p;
}

/// The env to hand to every child via `.envs(...)`: the login shell's, or (probe failed) ours with the
/// well-known user bins prepended to `PATH`.
pub fn vars() -> Vec<(String, String)> {
    current().vars
}

/// The `PATH` children run with.
pub fn path_var() -> OsString {
    current().path().map(OsString::from).unwrap_or_default()
}

/// First executable `name` on the children's `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    crate::paths::which_in(name, &path_var())
}

/// Apply [`vars`] to a child. The one call every spawn site makes (std and tokio `Command` both).
pub fn apply(cmd: &mut Command) -> &mut Command {
    cmd.envs(vars())
}

/// Well-known user bin directories that exist under `home`, in precedence order.
pub fn user_bins(home: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        ".bun/bin",
        ".cargo/bin",
        ".local/bin",
        ".local/share/mise/shims",
        ".asdf/shims",
        ".volta/bin",
        ".deno/bin",
        "go/bin",
    ]
    .iter()
    .map(|d| home.join(d))
    .collect();
    // nvm keeps no stable shim dir: the `current` symlink or the newest installed version.
    let nvm = home.join(".nvm");
    v.push(nvm.join("current/bin"));
    if let Ok(rd) = std::fs::read_dir(nvm.join("versions/node")) {
        let mut vs: Vec<PathBuf> = rd.flatten().map(|e| e.path().join("bin")).collect();
        vs.sort();
        v.extend(vs.into_iter().rev().take(1));
    }
    v.retain(|p| p.is_dir());
    v
}

/// `base_path` with the existing [`user_bins`] of `home` prepended (deduplicated).
pub fn fallback_path(home: &Path, base_path: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for p in user_bins(home)
        .into_iter()
        .map(|p| p.display().to_string())
        .chain(base_path.split(':').map(String::from))
    {
        if !p.is_empty() && !parts.contains(&p) {
            parts.push(p);
        }
    }
    parts.join(":")
}

/// Shell to probe and where the choice came from.
pub fn resolve_shell() -> (String, &'static str) {
    let over = OVERRIDE.lock().clone();
    pick_shell(
        over,
        std::env::var("SHELL").ok(),
        account_shell(),
        crate::paths::is_executable,
    )
}

fn pick_shell(
    over: Option<String>,
    env_shell: Option<String>,
    account: Option<String>,
    ok: impl Fn(&Path) -> bool,
) -> (String, &'static str) {
    let good = |s: &Option<String>| s.clone().filter(|s| ok(Path::new(s)));
    if let Some(s) = good(&over) {
        return (s, "settings");
    }
    if let Some(s) = good(&env_shell) {
        return (s, "$SHELL");
    }
    if let Some(s) = good(&account) {
        return (s, "account");
    }
    // zsh is macOS's default; a Linux box may not have it.
    let default = if ok(Path::new("/bin/zsh")) {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    (default.into(), "default")
}

/// Login shell from the user database (set by `chsh`). Windows has none.
#[cfg(windows)]
pub fn account_shell() -> Option<String> {
    None
}

/// Login shell from the user database (set by `chsh`).
#[cfg(unix)]
pub fn account_shell() -> Option<String> {
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: buffers live for the call; `out` points into `pw` on success.
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut pw,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        )
    };
    if rc != 0 || out.is_null() || pw.pw_shell.is_null() {
        return None;
    }
    // SAFETY: `pw_shell` is a NUL-terminated string inside `buf`.
    let s = unsafe { CStr::from_ptr(pw.pw_shell) }
        .to_string_lossy()
        .into_owned();
    (!s.is_empty()).then_some(s)
}

/// Windows: our own environment is the user's (see the module doc); the shell is the override from
/// Settings, else `%ComSpec%`.
#[cfg(windows)]
fn run() -> Probe {
    let shell = OVERRIDE
        .lock()
        .clone()
        .filter(|s| crate::paths::is_executable(Path::new(s)))
        .or_else(|| std::env::var("ComSpec").ok())
        .unwrap_or_else(|| "cmd.exe".into());
    Probe {
        shell,
        source: "windows",
        flags: None,
        error: None,
        took_ms: 0,
        vars: std::env::vars()
            .filter(|(k, _)| !SKIP.contains(&k.as_str()))
            .collect(),
    }
}

#[cfg(unix)]
fn run() -> Probe {
    let (shell, source) = resolve_shell();
    let home = dirs::home_dir().unwrap_or_else(|| "/".into());
    let base: Vec<(String, String)> = std::env::vars().collect();
    probe_with_source(&shell, source, &home, &base)
}

/// Probe `shell` (any shell, any env): the testable core of [`run`]. `base` is the environment the app
/// itself was launched with; it seeds the probe and is the fallback.
pub fn probe_with(shell: &str, home: &Path, base: &[(String, String)]) -> Probe {
    probe_with_source(shell, "settings", home, base)
}

fn probe_with_source(
    shell: &str,
    source: &'static str,
    home: &Path,
    base: &[(String, String)],
) -> Probe {
    let t0 = Instant::now();
    let mut errors = Vec::new();
    for flags in FLAG_SETS {
        match attempt(shell, flags, home, base) {
            Ok(vars) => {
                return Probe {
                    shell: shell.into(),
                    source,
                    flags: Some(flags.join(" ")),
                    error: None,
                    took_ms: t0.elapsed().as_millis() as u64,
                    vars,
                };
            }
            Err(e) => errors.push(format!("{}: {e}", flags.join(" "))),
        }
    }
    let base_path = base
        .iter()
        .find(|(k, _)| k == "PATH")
        .map_or("/usr/bin:/bin", |(_, v)| v.as_str());
    let mut vars: Vec<(String, String)> =
        base.iter().filter(|(k, _)| k != "PATH").cloned().collect();
    vars.push(("PATH".into(), fallback_path(home, base_path)));
    Probe {
        shell: shell.into(),
        source,
        flags: None,
        error: Some(format!(
            "could not read the environment of {shell} ({}); using the app's PATH plus the usual user bin directories (~/.bun/bin, ~/.cargo/bin, ~/.local/bin, mise/asdf shims, nvm)",
            errors.join("; ")
        )),
        took_ms: t0.elapsed().as_millis() as u64,
        vars,
    }
}

fn attempt(
    shell: &str,
    flags: &[&str],
    home: &Path,
    base: &[(String, String)],
) -> Result<Vec<(String, String)>, String> {
    let script = format!("echo {MARK}; /usr/bin/env -0; echo {MARK}");
    let mut child = Command::new(shell)
        .args(flags)
        .arg(&script)
        .env_clear()
        .envs(base.iter().map(|(k, v)| (k, v)))
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Read concurrently so a chatty rc file can't fill the pipe and stall.
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + ATTEMPT_TIMEOUT;
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out after {}s", ATTEMPT_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let out = reader.join().unwrap_or_default();
    parse(&String::from_utf8_lossy(&out)).ok_or_else(|| "no environment in output".into())
}

/// `KEY=VALUE` pairs between the markers; rc files may print around them.
pub fn parse(out: &str) -> Option<Vec<(String, String)>> {
    let mut parts = out.split(MARK);
    parts.next()?;
    let body = parts.next()?;
    parts.next()?; // closing marker must be present
    let vars: Vec<_> = body
        .trim_start_matches(['\r', '\n'])
        .split('\0')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| !k.is_empty() && !SKIP.contains(k))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    vars.iter().any(|(k, _)| k == "PATH").then_some(vars)
}

#[cfg(all(unix, any(test, feature = "test-fixtures")))]
/// Fixtures for spawn-path tests in other crates: a fake HOME with a `bun` on `~/.bun/bin` added by every
/// shell's rc file, and the bare environment of a desktop-entry launch.
pub mod fixtures {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// A fake HOME whose rc files of every shell put `~/.bun/bin` (with a fake `bun` that prints
    /// `bun-ran <args>`) on PATH, the way the bun installer does. rc files also print noise, like a real
    /// `.bashrc` with a banner.
    pub fn fake_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".bun/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let bun = bin.join("bun");
        std::fs::write(&bun, "#!/bin/sh\necho bun-ran \"$@\"\n").unwrap();
        std::fs::set_permissions(&bun, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sh_rc = "echo 'welcome banner'\nexport BUN_INSTALL=\"$HOME/.bun\"\nexport PATH=\"$BUN_INSTALL/bin:$PATH\"\n";
        for f in [".bashrc", ".bash_profile", ".zshrc", ".zshenv"] {
            std::fs::write(home.path().join(f), sh_rc).unwrap();
        }
        let fish = home.path().join(".config/fish");
        std::fs::create_dir_all(&fish).unwrap();
        std::fs::write(
            fish.join("config.fish"),
            "echo 'welcome banner'\nset -gx PATH $HOME/.bun/bin $PATH\n",
        )
        .unwrap();
        home
    }

    /// What a desktop/menu launch hands the app: no shell rc was ever read.
    pub fn gui_env(home: &Path) -> Vec<(String, String)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), home.display().to_string()),
        ]
    }

    /// The first of bash/zsh/fish installed here.
    pub fn some_shell() -> Option<&'static str> {
        all_shells().into_iter().next()
    }

    pub fn all_shells() -> Vec<&'static str> {
        [
            "/bin/bash",
            "/usr/bin/bash",
            "/bin/zsh",
            "/usr/bin/zsh",
            "/opt/homebrew/bin/fish",
            "/usr/bin/fish",
        ]
        .into_iter()
        .filter(|s| Path::new(s).exists())
        .collect()
    }
}

/// Windows: the probe is this process's environment, `Path` included, and children find tools on it.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn the_probe_is_our_environment_and_its_path_resolves_tools() {
        let p = run();
        assert_eq!(p.source, "windows");
        assert!(p.error.is_none());
        let path = p.path().expect("Path is found whatever its case");
        assert_eq!(Some(path.to_string()), std::env::var("PATH").ok());
        let ours = OsString::from(path);
        assert!(crate::paths::which_in("cmd", &ours).is_some());
        assert!(!p.vars.iter().any(|(k, _)| k == "PWD"));
    }
}

// Probes POSIX login shells (bash, zsh, fish).
#[cfg(all(test, unix))]
mod tests {
    use super::fixtures::{all_shells as shells, fake_home, gui_env};
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn path_of(p: &Probe) -> String {
        p.path().unwrap_or_default().to_string()
    }

    #[test]
    fn a_gui_launch_finds_bun_through_every_installed_shell() {
        let home = fake_home();
        let found = shells();
        assert!(!found.is_empty(), "no bash/zsh/fish to test with");
        for sh in found {
            let p = probe_with(sh, home.path(), &gui_env(home.path()));
            let want = home.path().join(".bun/bin");
            assert!(
                path_of(&p).split(':').any(|d| Path::new(d) == want),
                "{sh}: PATH {:?} lacks {} (error: {:?})",
                path_of(&p),
                want.display(),
                p.error
            );
            assert!(p.error.is_none(), "{sh}: {:?}", p.error);
            assert!(
                crate::paths::which_in("bun", std::ffi::OsStr::new(&path_of(&p))).is_some(),
                "{sh}: bun not resolvable"
            );
        }
    }

    #[test]
    fn rc_banner_output_does_not_corrupt_the_environment() {
        let home = fake_home();
        for sh in shells() {
            let p = probe_with(sh, home.path(), &gui_env(home.path()));
            assert!(
                p.vars
                    .iter()
                    .all(|(k, v)| !k.contains("banner") && !v.contains("welcome")),
                "{sh}: noise leaked into env: {:?}",
                p.vars
            );
        }
    }

    #[test]
    fn a_broken_shell_falls_back_to_user_bins_and_says_so() {
        let home = fake_home();
        let p = probe_with("/nonexistent/shell", home.path(), &gui_env(home.path()));
        let err = p.error.as_deref().expect("probe must report failure");
        assert!(
            err.contains("/nonexistent/shell") && err.contains("~/.bun/bin"),
            "{err}"
        );
        let want = home.path().join(".bun/bin");
        assert!(
            path_of(&p).split(':').any(|d| Path::new(d) == want),
            "{}",
            path_of(&p)
        );
        assert!(path_of(&p).ends_with("/usr/bin:/bin"), "{}", path_of(&p));
    }

    #[test]
    fn fallback_path_keeps_only_existing_dirs_and_dedupes() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(fallback_path(home.path(), "/usr/bin:/bin"), "/usr/bin:/bin");
        let cargo = home.path().join(".cargo/bin");
        std::fs::create_dir_all(&cargo).unwrap();
        let p = fallback_path(home.path(), &format!("{}:/usr/bin", cargo.display()));
        assert_eq!(p, format!("{}:/usr/bin", cargo.display()));
        let nvm = home.path().join(".nvm/versions/node");
        for v in ["v18.0.0", "v20.1.0"] {
            std::fs::create_dir_all(nvm.join(v).join("bin")).unwrap();
        }
        let p = fallback_path(home.path(), "/usr/bin");
        assert!(p.contains("v20.1.0/bin") && !p.contains("v18.0.0"), "{p}");
    }

    #[test]
    fn shell_choice_prefers_override_then_env_then_account() {
        let ok = |p: &Path| p.starts_with("/good");
        let s = |x: &str| Some(x.to_string());
        assert_eq!(
            pick_shell(s("/good/o"), s("/good/e"), s("/good/a"), ok),
            ("/good/o".into(), "settings")
        );
        assert_eq!(
            pick_shell(s("/bad/o"), s("/good/e"), s("/good/a"), ok),
            ("/good/e".into(), "$SHELL")
        );
        assert_eq!(
            pick_shell(None, s("/bad/e"), s("/good/a"), ok),
            ("/good/a".into(), "account")
        );
        let (d, src) = pick_shell(None, None, None, ok);
        assert_eq!(src, "default");
        assert!(d.ends_with("sh"));
    }

    #[test]
    fn parse_keeps_env_drops_noise_and_session_vars() {
        let out = format!("Welcome!\n{MARK}\nPATH=/a:/b\0FOO=x=y\0PWD=/tmp\0\n{MARK}\nbye");
        let v = parse(&out).unwrap();
        assert_eq!(
            v,
            vec![
                ("PATH".into(), "/a:/b".into()),
                ("FOO".into(), "x=y".into())
            ]
        );
    }

    #[test]
    fn parse_rejects_truncated_or_pathless_output() {
        assert!(parse(&format!("{MARK}\nPATH=/a\0")).is_none());
        assert!(parse(&format!("{MARK}\nFOO=1\0{MARK}")).is_none());
        assert!(parse("nothing").is_none());
    }

    /// An interactive rc that blocks forever must not hang a spawn: the probe times out and falls back.
    #[test]
    fn a_hanging_rc_times_out_instead_of_blocking() {
        let home = tempfile::tempdir().unwrap();
        let sh = home.path().join("hang.sh");
        std::fs::write(&sh, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let t = Instant::now();
        let p = probe_with(sh.to_str().unwrap(), home.path(), &gui_env(home.path()));
        assert!(
            p.error.as_deref().unwrap().contains("timed out"),
            "{:?}",
            p.error
        );
        // Three flag sets, each bounded.
        assert!(t.elapsed() < ATTEMPT_TIMEOUT * 3 + Duration::from_secs(5));
    }
}
