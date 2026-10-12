use std::path::{Path, PathBuf};

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
pub mod opener;
pub mod process;
pub mod service;
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

/// Child pids of the current run, used to reap orphans after a crash.
pub fn pid_file(root: &Path) -> PathBuf {
    log_dir(root).join(".pids")
}
