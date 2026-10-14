//! The report core behind "Report" and "Copy diagnostics": one place that knows what a bug report
//! about the app or its terminal host needs (versions of app, CLI and hostd, OS, the tail of
//! `hostd.log` and of the app log, the recent hostd incidents and Rust panics) and that redacts
//! secrets before anything leaves this module.
//!
//! The frontend never assembles this by hand: it calls `diag_snapshot` and renders the
//! [`Diagnostics`] it gets (camelCase, serialized from these structs; the contract test below
//! serializes the real value). Reuses the existing error screen (`lib/errors/ErrorPanel.svelte`):
//! its "Copy as Markdown" and "Report issue" already consume an `ErrorReport`, to which the
//! frontend adds `diagnostics`.
//!
//! Sources, all best effort (a report is being built because something is already broken, so a
//! missing file or a dead daemon is an empty field, never an error):
//! - versions: this crate's `CARGO_PKG_VERSION`, `atlas version` of the installed CLI, `info` of
//!   the running hostd;
//! - `hostd.log` tail: `atlas_host::paths::log(base)`;
//! - app log: `<base>/desktop/app.log`, written by [`log`] and by the panic hook [`install_panic_hook`];
//! - incidents: an in-memory ring of host events (unreachable, restarted, handoff failed).

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use parking_lot::Mutex;
use serde::Serialize;

/// Bytes of a log read from its end before collapsing repeats. The report is bounded by
/// [`TAIL_LINES`], so this only has to reach past a burst of one repeated error to its cause.
const TAIL_BYTES: u64 = 1024 * 1024;
/// Lines of a tail that go into a report.
const TAIL_LINES: usize = 40;
/// Incidents kept in memory.
const MAX_INCIDENTS: usize = 20;
/// The app log is rotated past this size (one `.1` generation), so it cannot grow unbounded.
const MAX_LOG_BYTES: u64 = 512 * 1024;

pub const REDACTED: &str = "[redacted]";

/// What a report says about the machine and the three binaries that must agree.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    pub app: String,
    /// `atlas version` of the installed CLI; `None` when it is missing or did not answer.
    pub cli: Option<String>,
    /// `info.version` of the running terminal host; `None` when it did not answer.
    pub hostd: Option<String>,
    pub hostd_pid: Option<u64>,
    pub hostd_caps: Vec<String>,
}

/// Where Atlas is installed and how macOS treats it: the facts that make one machine differ from
/// another with the same version (`/Applications` vs `~/Applications`, quarantine, translocation,
/// a dev symlink). Paths have the home directory collapsed to `~`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    /// The `.app` this process runs from; `None` when it is not a bundle (`desktop:dev`).
    pub running_bundle: Option<String>,
    /// The `Atlas.app` in `/Applications` or `~/Applications`.
    pub installed_app: Option<String>,
    pub cli: Option<String>,
    /// Where the CLI symlink points (`atlas link`); `None` for a regular file.
    pub cli_linked_to: Option<String>,
    /// The running bundle carries `com.apple.quarantine` (first launch not yet approved, or a
    /// download that was never cleared).
    pub quarantined: bool,
    /// macOS runs the app from a randomized read-only copy (`AppTranslocation`): a quarantined app
    /// started from outside `/Applications`. It cannot see its siblings, update itself or reach
    /// its own CLI by relative path.
    pub translocated: bool,
    /// The followed update channel.
    pub channel: String,
}

fn tilde(p: &Path) -> String {
    redact(&p.to_string_lossy())
}

impl Install {
    /// Pure assembly (unit-tested); `xattrs` is the output of `xattr` for the running bundle.
    pub fn from_parts(
        running: Option<PathBuf>,
        installed: Option<PathBuf>,
        cli: Option<PathBuf>,
        cli_linked_to: Option<String>,
        xattrs: Option<&str>,
        channel: &str,
    ) -> Self {
        Self {
            translocated: running
                .as_deref()
                .is_some_and(|p| p.to_string_lossy().contains("/AppTranslocation/")),
            quarantined: xattrs.is_some_and(|x| x.contains("com.apple.quarantine")),
            running_bundle: running.as_deref().map(tilde),
            installed_app: installed.as_deref().map(tilde),
            cli: cli.as_deref().map(tilde),
            cli_linked_to: cli_linked_to.map(|l| redact(&l)),
            channel: channel.to_string(),
        }
    }

    /// Read the facts of this machine. Best effort: every missing piece is `None`/`false`.
    pub fn gather() -> Self {
        let running = std::env::current_exe().ok().and_then(|exe| {
            let app = exe.parent()?.parent()?.parent()?;
            (app.extension()? == "app").then(|| app.to_path_buf())
        });
        let xattrs = running.as_ref().and_then(|b| {
            let out = std::process::Command::new("xattr").arg(b).output().ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        });
        let cli = atlas_host::client::atlas_bin().ok();
        let linked = cli
            .as_ref()
            .and_then(|c| std::fs::read_link(c).ok())
            .map(|t| t.to_string_lossy().into_owned());
        Self::from_parts(
            running,
            atlas_core::paths::installed_app(),
            cli,
            linked,
            xattrs.as_deref(),
            &atlas_core::channel::current().to_string(),
        )
    }
}

/// One thing that went wrong with the terminal host, newest last.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    /// Unix seconds.
    pub at: u64,
    /// `unreachable | restarted | upgrade-failed | panic | start-failed`.
    pub kind: String,
    pub detail: String,
}

/// Everything "Report" and "Copy diagnostics" carry. Secrets are already redacted.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub versions: Versions,
    /// `macOS 15.1 (aarch64)`.
    pub os: String,
    pub install: Install,
    /// Last lines of `hostd.log`, home directory collapsed to `~`.
    pub hostd_log: String,
    /// Last lines of the app log (panics, host incidents).
    pub app_log: String,
    pub incidents: Vec<Incident>,
    /// The newest `atlas` crash report (CLI, hostd or app panic): version, message and location,
    /// never the backtrace or the arguments. `None` when there is none.
    pub last_crash: Option<String>,
}

static INCIDENTS: LazyLock<Mutex<VecDeque<Incident>>> = LazyLock::new(Mutex::default);

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Record a host incident: kept in memory for the next report and appended to the app log.
pub fn incident(kind: &str, detail: &str) {
    let detail = redact(detail);
    {
        let mut q = INCIDENTS.lock();
        if q.len() >= MAX_INCIDENTS {
            q.pop_front();
        }
        q.push_back(Incident {
            at: now(),
            kind: kind.to_string(),
            detail: detail.clone(),
        });
    }
    log(&format!("{kind}: {detail}"));
}

pub fn incidents() -> Vec<Incident> {
    INCIDENTS.lock().iter().cloned().collect()
}

/// `<base>/desktop/app.log`.
pub fn app_log_path(base: &Path) -> PathBuf {
    base.join("desktop").join("app.log")
}

/// Append one timestamped line to the app log of the default base dir.
pub fn log(line: &str) {
    log_in(&atlas_host::paths::base(), line);
}

/// [`log`] for the log under `base`; rotates once past [`MAX_LOG_BYTES`].
pub fn log_in(base: &Path, line: &str) {
    let path = app_log_path(base);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{} {}", now(), redact(line).replace('\n', "\\n"));
    }
}

/// A Rust panic of the app: the message and location go to the app log and the incident ring.
/// Chains to the previous hook, so the default stderr output and debug-build behavior stay.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic payload");
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        incident("panic", &format!("{msg} ({at})"));
        prev(info);
    }));
}

/// Last [`TAIL_LINES`] lines of `path` (reading at most [`TAIL_BYTES`] from its end), ANSI escapes
/// stripped, runs of identical lines collapsed into one `(repeated N×)` line so a burst of the same
/// error cannot push its cause out of the tail, and redacted. Empty when missing or unreadable.
pub fn tail(path: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = atlas_host::strip_ansi(&String::from_utf8_lossy(&buf));
    let mut lines = text.lines();
    // A read that began mid-file starts mid-line: drop that fragment.
    if start > 0 {
        lines.next();
    }
    let mut runs: Vec<(&str, usize)> = Vec::new();
    for l in lines {
        match runs.last_mut() {
            Some((prev, n)) if *prev == l => *n += 1,
            _ => runs.push((l, 1)),
        }
    }
    let skip = runs.len().saturating_sub(TAIL_LINES);
    let out: Vec<String> = runs[skip..]
        .iter()
        .map(|&(l, n)| {
            if n > 1 {
                format!("{l}  (repeated {n}×)")
            } else {
                l.to_string()
            }
        })
        .collect();
    redact(&out.join("\n"))
}

/// Replace what must never leave the machine: tokens and keys by shape and by name, `Authorization`
/// headers, URL credentials, and the home directory (collapsed to `~`).
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    if let Some(home) = dirs::home_dir().and_then(|h| h.to_str().map(str::to_string))
        && home.len() > 1
    {
        out = out.replace(&home, "~");
    }
    out = redact_users(&out);
    out = redact_url_credentials(&out);
    out = redact_named(&out);
    redact_tokens(&out)
}

/// `/Users/<name>/` and `/home/<name>/` of any user, not only the current one.
fn redact_users(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = ["/Users/", "/home/"]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
    {
        let prefix_len = if rest[i..].starts_with("/Users/") {
            7
        } else {
            6
        };
        out.push_str(&rest[..i]);
        let after = &rest[i + prefix_len..];
        let name_len = after
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        out.push('~');
        rest = &after[name_len..];
    }
    out.push_str(rest);
    out
}

/// `scheme://user:password@host` → `scheme://[redacted]@host`.
fn redact_url_credentials(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out.push_str(REDACTED);
                rest = &tail[at..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

/// Names whose value is a secret: `token=…`, `"password": "…"`, `Authorization: Bearer …`, `API_KEY=…`.
fn redact_named(s: &str) -> String {
    const NAMES: &[&str] = &[
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "api-key",
        "access_key",
        "private_key",
        "authorization",
        "auth",
        "cookie",
        "credential",
    ];
    s.lines()
        .map(|line| redact_line(line, NAMES))
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str, names: &[&str]) -> String {
    let lower = line.to_ascii_lowercase();
    let bytes = line.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        // Nearest secret-looking identifier at or after `i` that starts an identifier.
        let hit = names
            .iter()
            .filter_map(|n| {
                let mut from = i;
                while let Some(p) = lower[from..].find(n) {
                    let at = from + p;
                    if at == 0
                        || !ident(bytes[at - 1])
                        || bytes[at - 1] == b'_'
                        || bytes[at - 1] == b'-'
                    {
                        return Some((at, n.len()));
                    }
                    from = at + n.len();
                }
                None
            })
            .min_by_key(|(p, _)| *p);
        let Some((at, nlen)) = hit else { break };
        // The rest of the identifier (`GITHUB_TOKEN`, `token_id`, `x-api-key`).
        let mut j = at + nlen;
        while j < line.len() && ident(bytes[j]) {
            j += 1;
        }
        let mut k = j;
        if line[k..].starts_with(['"', '\'']) {
            k += 1;
        }
        while line[k..].starts_with(' ') {
            k += 1;
        }
        // Only `name=value` / `name: value` is an assignment; prose that mentions a name is not.
        if !line[k..].starts_with(['=', ':']) {
            out.push_str(&line[i..j]);
            i = j;
            continue;
        }
        k += 1;
        while line[k..].starts_with(' ') {
            k += 1;
        }
        let quote = line[k..].chars().next().filter(|c| matches!(c, '"' | '\''));
        if quote.is_some() {
            k += 1;
        }
        // `Authorization: Bearer abc` keeps the scheme word out of the secret.
        if let Some(p) = ["Bearer ", "Basic ", "bearer ", "basic "]
            .iter()
            .find(|p| line[k..].starts_with(**p))
        {
            k += p.len();
        }
        let end = match quote {
            Some(q) => line[k..].find(q).map_or(line.len(), |e| k + e),
            None => line[k..]
                .find(|c: char| c.is_whitespace() || c == ',' || c == '&' || c == '"' || c == '\'')
                .map_or(line.len(), |e| k + e),
        };
        out.push_str(&line[i..k]);
        out.push_str(REDACTED);
        i = end;
    }
    out.push_str(&line[i..]);
    out
}

/// Well-known token shapes and any long hex/base64-looking run, wherever they appear.
fn redact_tokens(s: &str) -> String {
    const PREFIXES: &[&str] = &[
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "sk-",
        "xoxb-",
        "xoxp-",
        "AKIA",
        "glpat-",
        "npm_",
        "pk_live_",
        "sk_live_",
    ];
    let mut out = String::with_capacity(s.len());
    for (n, word) in s
        .split_inclusive(|c: char| c.is_whitespace() || "\"'(),;<>[]{}=:".contains(c))
        .enumerate()
    {
        let _ = n;
        let trimmed =
            word.trim_end_matches(|c: char| c.is_whitespace() || "\"'(),;<>[]{}=:".contains(c));
        let tail = &word[trimmed.len()..];
        let secret = PREFIXES
            .iter()
            .any(|p| trimmed.starts_with(p) && trimmed.len() >= p.len() + 8)
            || is_long_secretish(trimmed);
        if secret {
            out.push_str(REDACTED);
            out.push_str(tail);
        } else {
            out.push_str(word);
        }
    }
    out
}

/// 32+ chars of hex or base64 alphabet with at least one digit and one letter: a hash or a token,
/// never prose or a path (paths contain `/` only between short segments, which fails the length).
fn is_long_secretish(w: &str) -> bool {
    w.len() >= 32
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
        && w.chars().any(|c| c.is_ascii_digit())
        && w.chars().any(|c| c.is_ascii_alphabetic())
        && !w.starts_with('/')
}

/// `macOS 15.1 (aarch64)`; the version needs `sw_vers`, so a failure leaves just the family.
pub fn os_label() -> String {
    let arch = std::env::consts::ARCH;
    let family = match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        o => o,
    };
    let version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty());
    match version {
        Some(v) => format!("{family} {v} ({arch})"),
        None => format!("{family} ({arch})"),
    }
}

/// Summary of the newest `crash-*.toml` in `dir` (`atlas_core::crash::crash_dir`): the fields a
/// maintainer needs, redacted. The backtrace and the command line stay in the file on disk.
pub fn last_crash(dir: &Path) -> Option<String> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".toml"))
        })
        // `crash-<unix seconds>.toml`: the number orders them (a name sort would put 99 after 100).
        .max_by_key(|p| {
            p.file_stem()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix("crash-"))
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        })?;
    let text = std::fs::read_to_string(newest).ok()?;
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k)?.trim_start().strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_string())
    };
    let msg = field("msg")?;
    let line = match (field("version"), field("location")) {
        (Some(v), Some(l)) => format!("atlas {v}: {msg} ({l})"),
        (_, Some(l)) => format!("{msg} ({l})"),
        _ => msg,
    };
    Some(redact(&line))
}

/// Assemble a report from already-gathered versions; reads the two logs under `base` and the
/// crash reports in the default crash directory.
pub fn build(base: &Path, versions: Versions) -> Diagnostics {
    build_with(base, &atlas_core::crash::crash_dir(), versions)
}

/// [`build`] with an explicit crash directory.
pub fn build_with(base: &Path, crashes: &Path, versions: Versions) -> Diagnostics {
    Diagnostics {
        versions,
        os: os_label(),
        install: Install::gather(),
        hostd_log: tail(&atlas_host::paths::log(base)),
        app_log: tail(&app_log_path(base)),
        incidents: incidents(),
        last_crash: last_crash(crashes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A realistic `hostd.log` with every kind of secret a daemon or a child process could print.
    const LOG: &str = "\
atlas hostd 1.5.0-alpha.5 listening on /Users/ana/.atlas/hostd.sock (pid 57750)
spawn failed: GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789 cwd=/Users/ana/work/app
curl -H \"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig\" https://api.example.com
clone https://ana:hunter2@github.com/acme/private.git
{\"password\": \"correct horse battery staple\", \"user\": \"ana\"}
API_KEY=sk-live-0123456789abcdef OPENAI_API_KEY=sk-proj-abcdefghijklmnop
digest 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 done
plain line, nothing to hide: terminal 3 exited with code 0";

    #[test]
    fn redaction_removes_every_secret_shape_and_keeps_the_rest() {
        let out = redact(LOG);
        for secret in [
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "hunter2",
            "correct horse battery staple",
            "sk-live-0123456789abcdef",
            "sk-proj-abcdefghijklmnop",
            "9f86d081884c7d659a2feaa0c55ad015",
            "/Users/ana",
        ] {
            assert!(!out.contains(secret), "{secret} leaked:\n{out}");
        }
        // What a maintainer needs to read stays readable.
        for keep in [
            "atlas hostd 1.5.0-alpha.5 listening on ~/.atlas/hostd.sock (pid 57750)",
            "plain line, nothing to hide: terminal 3 exited with code 0",
            "\"user\": \"ana\"",
            "github.com/acme/private.git",
        ] {
            assert!(out.contains(keep), "{keep} was lost:\n{out}");
        }
        assert!(out.contains(REDACTED));
    }

    /// The bias is deliberate: a secret-named key loses its value even when the value is prose
    /// (`secret: terminal`); leaking a token costs more than losing a word.
    #[test]
    fn a_secret_named_key_is_redacted_even_when_the_value_looks_harmless() {
        assert_eq!(redact("secret: terminal"), format!("secret: {REDACTED}"));
        // ...but prose that merely mentions the word is left alone.
        assert_eq!(redact("the token was refreshed"), "the token was refreshed");
        assert_eq!(redact("authenticated as ana"), "authenticated as ana");
    }

    #[test]
    fn redaction_is_idempotent_and_leaves_clean_text_alone() {
        let once = redact(LOG);
        assert_eq!(redact(&once), once);
        let clean = "terminal host 1.5.0-alpha.4 is older than Atlas 1.5.0-alpha.5; restarting";
        assert_eq!(redact(clean), clean);
    }

    #[test]
    fn the_current_users_home_collapses_to_tilde() {
        let home = dirs::home_dir().unwrap();
        let line = format!("log at {}/.atlas/hostd.log", home.display());
        assert_eq!(redact(&line), "log at ~/.atlas/hostd.log");
    }

    #[test]
    fn tail_keeps_the_last_lines_in_order_and_redacts_them() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hostd.log");
        let mut body = String::new();
        for i in 0..200 {
            body.push_str(&format!("line {i}\n"));
        }
        body.push_str("TOKEN=supersecretvalue123\n");
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[TAIL_LINES - 2], "line 199");
        assert_eq!(lines[TAIL_LINES - 1], format!("TOKEN={REDACTED}"));
    }

    #[test]
    fn tail_of_a_huge_log_starts_on_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.log");
        // Distinct lines (identical ones would collapse) and more than TAIL_BYTES of them, so the
        // read starts mid-file.
        let body: String = (0..15_000).map(|i| format!("{i:0>99}\n")).collect();
        assert!(body.len() as u64 > TAIL_BYTES);
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        assert!(t.lines().all(|l| l.len() == 99), "a fragment leaked in");
    }

    /// Regression (1.5.0-alpha.8 report): hostd.log ended in 1300+ copies of "another atlas hostd
    /// is running" in red ANSI, so the report's 40-line tail was that one line 40 times with raw
    /// escape codes, and the cause ("Too many open files", the takeovers before it) never reached
    /// the issue. Repeats collapse into one line with a count, escapes are stripped, and the lines
    /// before the burst stay in the tail.
    #[test]
    fn tail_collapses_a_repeated_burst_and_keeps_what_came_before() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hostd.log");
        let mut body =
            String::from("atlas hostd 1.5.0-alpha.5 listening (pid 73595, took over 7 pty(s))\n");
        body.push_str("\x1b[1;31merror:\x1b[0m Too many open files (os error 24)\n");
        for _ in 0..1331 {
            body.push_str("\x1b[1;31merror:\x1b[0m another atlas hostd is running (~/.atlas)\n");
        }
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        assert!(!t.contains('\x1b'), "ANSI escapes leaked: {t}");
        assert!(t.contains("Too many open files"), "{t}");
        assert!(t.contains("took over 7 pty(s)"), "{t}");
        assert_eq!(
            t.matches("another atlas hostd is running").count(),
            1,
            "{t}"
        );
        assert!(t.contains("(repeated 1331×)"), "{t}");
    }

    #[test]
    fn a_missing_log_is_an_empty_tail_not_an_error() {
        // 1.5.0-alpha.5 scenario "hostd.log missing": the report is still built.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(tail(&dir.path().join("nope.log")), "");
        let d = build(dir.path(), Versions::default());
        assert!(d.hostd_log.is_empty() && d.app_log.is_empty());
    }

    #[test]
    fn the_app_log_rotates_instead_of_growing_forever() {
        let dir = tempfile::tempdir().unwrap();
        let p = app_log_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![b'a'; MAX_LOG_BYTES as usize + 1]).unwrap();
        log_in(dir.path(), "after rotation");
        assert!(p.with_extension("log.1").exists());
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("after rotation")
        );
    }

    #[test]
    fn log_lines_are_redacted_and_single_line() {
        let dir = tempfile::tempdir().unwrap();
        log_in(dir.path(), "boom\nTOKEN=abc12345secret");
        let s = std::fs::read_to_string(app_log_path(dir.path())).unwrap();
        assert_eq!(s.lines().count(), 1, "{s}");
        assert!(!s.contains("abc12345secret"), "{s}");
    }

    /// Cross-boundary contract: the frontend (`lib/errors/diagnostics.ts`) reads these exact keys.
    /// The value is the real serialized struct, never a hand-written mock.
    #[test]
    fn diagnostics_serialize_with_the_keys_the_frontend_reads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(atlas_host::paths::log(dir.path()), "atlas hostd up\n").unwrap();
        let d = build(
            dir.path(),
            Versions {
                app: "1.5.0-alpha.5".into(),
                cli: Some("1.5.0-alpha.5".into()),
                hostd: Some("1.5.0-alpha.4".into()),
                hostd_pid: Some(7),
                hostd_caps: vec!["pty_attach_raw".into()],
            },
        );
        let v = serde_json::to_value(&d).unwrap();
        for k in [
            "versions",
            "os",
            "install",
            "hostdLog",
            "appLog",
            "incidents",
            "lastCrash",
        ] {
            assert!(v.get(k).is_some(), "missing {k} in {v}");
        }
        for k in ["app", "cli", "hostd", "hostdPid", "hostdCaps"] {
            assert!(v["versions"].get(k).is_some(), "missing versions.{k}");
        }
        assert_eq!(v["hostdLog"], json!("atlas hostd up"));
        assert_eq!(v["versions"]["hostd"], "1.5.0-alpha.4");
    }

    /// A hostd panic: the CLI's panic hook (`atlas_core::crash`) prints the banner into hostd's
    /// stderr, which is `hostd.log`, and writes `crash-<ts>.toml`. The report must carry both the
    /// log banner and the crash file's message/location (redacted), newest crash only.
    #[test]
    fn a_hostd_panic_reaches_the_report_from_the_log_and_the_crash_file() {
        let base = tempfile::tempdir().unwrap();
        let crashes = tempfile::tempdir().unwrap();
        std::fs::write(
            atlas_host::paths::log(base.path()),
            "atlas crashed.\n  index out of bounds at crates/atlas-host/src/pty/mod.rs:10:5\n",
        )
        .unwrap();
        std::fs::write(
            crashes.path().join("crash-100.toml"),
            "version = \"1.5.0-alpha.4\"\nmsg = \"old one\"\nlocation = \"a.rs:1:1\"\n",
        )
        .unwrap();
        std::fs::write(
            crashes.path().join("crash-200.toml"),
            "version = \"1.5.0-alpha.5\"\nargs = \"atlas hostd\"\nmsg = \"boom TOKEN=abc12345secret\"\nlocation = \"crates/atlas-host/src/server.rs:9:1\"\n\nbacktrace = \'\'\'\nframe\n\'\'\'\n",
        )
        .unwrap();
        let d = build_with(base.path(), crashes.path(), Versions::default());
        assert!(d.hostd_log.contains("index out of bounds"));
        let c = d.last_crash.expect("the newest crash report is summarized");
        assert!(c.contains("boom") && c.contains("server.rs:9:1"), "{c}");
        assert!(
            !c.contains("abc12345secret") && !c.contains("old one"),
            "{c}"
        );
        assert!(
            !c.contains("frame"),
            "the backtrace stays out of the report: {c}"
        );
    }

    #[test]
    fn no_crash_directory_means_no_crash_line() {
        let base = tempfile::tempdir().unwrap();
        let d = build_with(base.path(), &base.path().join("nope"), Versions::default());
        assert_eq!(d.last_crash, None);
    }

    /// The machine facts a maintainer needs to diagnose a user's install remotely, with the home
    /// directory collapsed: which bundle runs, where the app and CLI are installed, whether macOS
    /// quarantined or translocated the app, and whether the CLI is a dev symlink.
    #[test]
    fn install_facts_name_the_paths_and_flag_translocation_and_quarantine() {
        let home = dirs::home_dir().unwrap();
        let bundle = home.join("Applications/Atlas.app");
        let i = Install::from_parts(
            Some(bundle.clone()),
            Some(bundle.clone()),
            Some(home.join(".atlas/bin/atlas")),
            None,
            Some("com.apple.quarantine"),
            "alpha",
        );
        assert_eq!(
            i.running_bundle.as_deref(),
            Some("~/Applications/Atlas.app")
        );
        assert_eq!(i.installed_app.as_deref(), Some("~/Applications/Atlas.app"));
        assert_eq!(i.cli.as_deref(), Some("~/.atlas/bin/atlas"));
        assert!(i.quarantined);
        assert!(!i.translocated);
        assert_eq!(i.channel, "alpha");
        // Gatekeeper's randomized read-only copy: the running bundle is not the installed one.
        let t = Install::from_parts(
            Some("/private/var/folders/xx/AppTranslocation/ABC/d/Atlas.app".into()),
            Some("/Applications/Atlas.app".into()),
            None,
            None,
            None,
            "stable",
        );
        assert!(t.translocated);
        assert_eq!(t.cli, None);
    }

    #[test]
    fn a_linked_cli_reports_where_it_points() {
        let i = Install::from_parts(
            None,
            None,
            Some("/h/.atlas/bin/atlas".into()),
            Some("/repo/target/debug/atlas".into()),
            None,
            "alpha",
        );
        assert_eq!(i.cli_linked_to.as_deref(), Some("/repo/target/debug/atlas"));
        assert!(i.running_bundle.is_none());
    }

    #[test]
    fn incidents_are_bounded_redacted_and_ordered() {
        for i in 0..(MAX_INCIDENTS + 5) {
            incident("unreachable", &format!("n{i} TOKEN=hunter2hunter2"));
        }
        let all = incidents();
        assert!(all.len() <= MAX_INCIDENTS);
        assert!(all.iter().all(|i| !i.detail.contains("hunter2")));
        assert!(
            all.last()
                .unwrap()
                .detail
                .starts_with(&format!("n{}", MAX_INCIDENTS + 4))
        );
    }

    /// A deterministic report: real structs, real redaction, real serializer.
    fn fixture() -> serde_json::Value {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            atlas_host::paths::log(dir.path()),
            "atlas hostd 1.5.0-alpha.4 listening on /Users/ana/.atlas/hostd.sock (pid 8664, took over 2 pty(s))\n\
             spawn failed: GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("desktop")).unwrap();
        std::fs::write(
            app_log_path(dir.path()),
            "1791943000 unreachable: socket in use\n",
        )
        .unwrap();
        let mut d = build(
            dir.path(),
            Versions {
                app: "1.5.0-alpha.5".into(),
                cli: Some("1.5.0-alpha.5".into()),
                hostd: Some("1.5.0-alpha.4".into()),
                hostd_pid: Some(8664),
                hostd_caps: vec!["pty_attach_raw".into(), "upgrade".into(), "lsp".into()],
            },
        );
        // Machine-dependent and process-global fields are pinned: the fixture is a contract, not a log.
        d.os = "macOS 15.1 (aarch64)".into();
        d.install = Install::from_parts(
            Some("/Applications/Atlas.app".into()),
            Some("/Applications/Atlas.app".into()),
            Some("/Users/ana/.atlas/bin/atlas".into()),
            None,
            Some("com.apple.quarantine"),
            "alpha",
        );
        d.last_crash = Some(
            "atlas 1.5.0-alpha.4: index out of bounds (crates/atlas-host/src/server.rs:9:1)".into(),
        );
        d.incidents = vec![Incident {
            at: 1791943000,
            kind: "unreachable".into(),
            detail: "socket in use".into(),
        }];
        serde_json::to_value(&d).unwrap()
    }

    /// Contract with the frontend: `apps/desktop/src/lib/__fixtures__/diagnostics.json` is the REAL
    /// serialization of [`Diagnostics`] (secrets already redacted). The TS tests build reports from
    /// that file, so a rename on either side fails a test. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn diagnostics_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/diagnostics.json");
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&fixture(), &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it");
        let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
        let fresh: serde_json::Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
        let log = fresh["hostdLog"].as_str().unwrap();
        assert!(
            !log.contains("ghp_") && !log.contains("/Users/ana"),
            "{log}"
        );
    }
}
