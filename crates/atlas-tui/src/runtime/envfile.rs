//! Dotenv-style `env_files` loading and per-service `lock_file` cleanup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Parse dotenv content: `KEY=VALUE`, optional `export `, single/double
/// quotes, `#` comments (full-line, or trailing on unquoted values), blank
/// lines. Only the first `=` splits, so values may contain `=`.
pub fn parse_env(content: &str) -> Vec<(String, String)> {
    content.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (key, raw) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key.to_string(), parse_value(raw.trim())))
}

fn parse_value(raw: &str) -> String {
    let mut chars = raw.chars();
    if let Some(q @ ('"' | '\'')) = chars.next() {
        // Quoted: take up to the closing quote; anything after is ignored.
        if let Some(end) = raw[1..].find(q) {
            return raw[1..1 + end].to_string();
        }
        return raw[1..].to_string();
    }
    // Unquoted: a ` #` starts a trailing comment.
    match raw.find(" #") {
        Some(i) => raw[..i].trim_end().to_string(),
        None => raw.to_string(),
    }
}

/// Load `files` (relative to `root`) in order; later files win. Missing or
/// unreadable files are skipped silently.
pub fn load_env_files(root: &Path, files: &[String]) -> HashMap<String, String> {
    let mut env = HashMap::new();
    for f in files {
        if let Ok(content) = std::fs::read_to_string(root.join(f)) {
            env.extend(parse_env(&content));
        }
    }
    env
}

/// Merge env layers, lowest → highest precedence: `env_files` < per-service
/// `env:`. (Inherited parent env is the base the child already has; atlas-
/// injected vars are applied by the caller afterwards.)
pub fn layered_env(
    env_files: &HashMap<String, String>,
    service_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = env_files.clone();
    out.extend(service_env.iter().map(|(k, v)| (k.clone(), v.clone())));
    out
}

/// Parse the pid out of a `{"pid": N}` lock file body.
pub fn parse_lock_pid(content: &str) -> Option<i32> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let pid = v.get("pid")?.as_i64()?;
    i32::try_from(pid).ok().filter(|&p| p > 1)
}

/// Decision for a stale-lock cleanup.
#[derive(Debug, PartialEq, Eq)]
pub enum LockAction {
    /// Process is gone (or lock is unparsable): just delete the file.
    RemoveFileOnly,
    /// Live process owned by this project: terminate then delete the file.
    KillThenRemove(i32),
    /// Live but must not be touched (ours / parent / outside root): leave
    /// everything as-is.
    Leave,
}

/// True when a process started no later than the lock file was last written —
/// a process born after the lock cannot be the one that wrote it (pid reuse).
/// Either time unknown → false (cannot prove ownership).
pub fn started_before_lock(start_secs: Option<u64>, lock_mtime_secs: Option<u64>) -> bool {
    matches!((start_secs, lock_mtime_secs), (Some(s), Some(m)) if s <= m)
}

/// Pure decision: given the lock pid and facts about the process, decide.
/// `plausible_owner` = name passes the service-runtime blocklist AND the
/// process started no later than the lock file's mtime. A live pid inside the
/// project that fails it is a recycled pid: the lock is stale, the process is
/// not ours to kill.
pub fn decide_lock_action(
    pid: Option<i32>,
    alive: bool,
    cwd_inside_root: bool,
    plausible_owner: bool,
    our_pid: i32,
    our_ppid: i32,
) -> LockAction {
    let Some(pid) = pid else {
        return LockAction::RemoveFileOnly;
    };
    if !alive {
        return LockAction::RemoveFileOnly;
    }
    if pid == our_pid || pid == our_ppid || !cwd_inside_root {
        return LockAction::Leave;
    }
    if !plausible_owner {
        return LockAction::RemoveFileOnly;
    }
    LockAction::KillThenRemove(pid)
}

/// True when `cwd` is `root` or below it.
pub fn path_inside(cwd: &Path, root: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    cwd.starts_with(root)
}

/// Current working directory of `pid` via a per-PID query (never port-based).
pub fn process_cwd(pid: i32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }
    // Windows does not expose another process's directory: the lock is then never "inside".
    #[cfg(windows)]
    {
        let _ = pid;
        None
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix('n').map(PathBuf::from))
    }
}

/// Clean a stale service lock: kill a live same-project owner (SIGTERM, wait
/// up to 2s, SIGKILL) and remove the file; remove it if the owner is dead.
pub async fn cleanup_lock(root: &Path, lock_file: &str) {
    let path = root.join(lock_file);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let pid = parse_lock_pid(&content);
    let alive = match pid {
        Some(p) => crate::runtime::health::check_process_alive(p as u32).await,
        None => false,
    };
    // Ownership facts are only gathered for a live pid. The cwd query shells
    // out (lsof on macOS), so run it off the async workers.
    let (inside, plausible) = match (alive, pid) {
        (true, Some(p)) => {
            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let root = root.to_path_buf();
            tokio::task::spawn_blocking(move || {
                let inside = process_cwd(p).is_some_and(|c| path_inside(&c, &root));
                let plausible = crate::runtime::process::pid_is_service_runtime(p)
                    && started_before_lock(crate::runtime::process::proc_start_secs(p), mtime);
                (inside, plausible)
            })
            .await
            .unwrap_or((false, false))
        }
        _ => (false, false),
    };
    let our_pid = std::process::id() as i32;
    let our_ppid = atlas_core::sys::parent_pid() as i32;
    match decide_lock_action(pid, alive, inside, plausible, our_pid, our_ppid) {
        LockAction::RemoveFileOnly => {
            let _ = std::fs::remove_file(&path);
        }
        LockAction::KillThenRemove(p) => {
            crate::runtime::process::kill_exact_gracefully(p, Duration::from_secs(2)).await;
            let _ = std::fs::remove_file(&path);
        }
        LockAction::Leave => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_export_comments_blank_and_equals() {
        let src = "\
# comment

FOO=bar
export BAZ=qux
Q1=\"hello world\"
Q2='single # not comment'
URL=postgres://u:p@h/db?a=b&c=d
TRAIL=val # trailing
HASH=a#b
EMPTY=
  SPACED  =  v  
not a pair
=novalue
";
        let m: HashMap<_, _> = parse_env(src).into_iter().collect();
        assert_eq!(m["FOO"], "bar");
        assert_eq!(m["BAZ"], "qux");
        assert_eq!(m["Q1"], "hello world");
        assert_eq!(m["Q2"], "single # not comment");
        assert_eq!(m["URL"], "postgres://u:p@h/db?a=b&c=d");
        assert_eq!(m["TRAIL"], "val");
        assert_eq!(m["HASH"], "a#b");
        assert_eq!(m["EMPTY"], "");
        assert_eq!(m["SPACED"], "v");
        assert_eq!(m.len(), 9);
    }

    #[test]
    fn later_file_wins_and_missing_skipped() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.env"), "X=1\nY=1\n").unwrap();
        std::fs::write(dir.path().join("b.env"), "Y=2\n").unwrap();
        let files = vec!["a.env".into(), "missing.env".into(), "b.env".into()];
        let env = load_env_files(dir.path(), &files);
        assert_eq!(env["X"], "1");
        assert_eq!(env["Y"], "2");
    }

    #[test]
    fn service_env_overrides_env_files() {
        let files: HashMap<_, _> = [("A", "file"), ("B", "file")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into();
        let svc: HashMap<_, _> = [("B", "svc")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into();
        let out = layered_env(&files, &svc);
        assert_eq!(out["A"], "file");
        assert_eq!(out["B"], "svc");
    }

    #[test]
    fn lock_pid_parsing() {
        assert_eq!(parse_lock_pid(r#"{"pid": 1234}"#), Some(1234));
        assert_eq!(parse_lock_pid(r#"{"pid": 1}"#), None);
        assert_eq!(parse_lock_pid(r#"{"x": 1}"#), None);
        assert_eq!(parse_lock_pid("garbage"), None);
    }

    #[test]
    fn lock_decision() {
        use LockAction::*;
        let d = decide_lock_action;
        assert_eq!(d(None, false, true, true, 10, 9), RemoveFileOnly);
        assert_eq!(d(Some(50), false, true, true, 10, 9), RemoveFileOnly);
        assert_eq!(d(Some(50), true, true, true, 10, 9), KillThenRemove(50));
        assert_eq!(d(Some(50), true, false, true, 10, 9), Leave);
        assert_eq!(d(Some(10), true, true, true, 10, 9), Leave);
        assert_eq!(d(Some(9), true, true, true, 10, 9), Leave);
        // Recycled into a shell/editor (blocklisted) or born after the lock
        // was written: never kill; the lock file itself is stale.
        assert_eq!(d(Some(50), true, true, false, 10, 9), RemoveFileOnly);
    }

    #[test]
    fn started_before_lock_rule() {
        assert!(started_before_lock(Some(100), Some(100)));
        assert!(started_before_lock(Some(99), Some(100)));
        assert!(!started_before_lock(Some(101), Some(100)));
        // Unknown start or mtime: cannot prove ownership, so do not kill.
        assert!(!started_before_lock(None, Some(100)));
        assert!(!started_before_lock(Some(100), None));
    }

    #[test]
    fn path_inside_checks_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(path_inside(&sub, dir.path()));
        assert!(path_inside(dir.path(), dir.path()));
        assert!(!path_inside(Path::new("/"), dir.path()));
    }

    #[tokio::test]
    async fn cleanup_removes_lock_for_dead_pid() {
        let dir = tempfile::TempDir::new().unwrap();
        let lock = dir.path().join("x.lock");
        // pid far above any real pid_max on macOS/Linux defaults
        std::fs::write(&lock, r#"{"pid": 2147480000}"#).unwrap();
        cleanup_lock(dir.path(), "x.lock").await;
        assert!(!lock.exists());
    }
}
