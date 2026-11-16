use std::path::{Path, PathBuf};

pub mod clock;
pub mod deps;
pub mod dev_host;
pub mod docker;
pub mod envfile;
pub mod health;
pub mod linebuf;
pub mod lock;
pub mod logindex;
pub mod loglinks;
pub mod logs;
pub mod manager;
pub mod memory;
pub mod opener;
pub mod port_conflict;
pub mod probe;
pub mod process;
pub mod service;
pub mod text;
pub mod timerange;
pub mod tunnel;
pub mod usage;

pub use lock::SingletonLock;
pub use logs::{LogBuffer, LogEntry, LogLevel, LogStream};
pub use manager::{ManagerEvent, ServiceManager};
pub use service::{ServiceState, ServiceStatus};

/// Where atlas keeps per-project logs, sockets, pid and status files.
pub fn log_dir(root: &Path) -> PathBuf {
    root.join(".logs")
}

/// Create the log directory (`.logs/`) and keep it out of git: a `.gitignore` of `*` inside it ignores every runtime
/// file (and itself) without touching the project's own `.gitignore` or its tracked files, in a checkout or a
/// worktree alike. An `.gitignore` already there (the user's own) is left as it is. Every place that creates the
/// directory goes through here, so the first file written is never a candidate for "Stage all".
pub fn ensure_log_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "# Atlas runtime files: never committed.\n*\n")?;
    }
    Ok(())
}

/// Child pids of the current run, used to reap orphans after a crash.
pub fn pid_file(root: &Path) -> PathBuf {
    log_dir(root).join(".pids")
}
