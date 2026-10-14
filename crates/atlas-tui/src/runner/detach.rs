//! Process control around a runner: `--detach` (start a daemon and wait for
//! its socket) and `atlas down` (ask it to stop and wait for it to go).

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use super::RUNNER_LOG;
use crate::config;
use crate::control;
use crate::runtime::SingletonLock;

/// Result of `atlas dev --detach`.
pub enum Detached {
    Started {
        pid: i32,
    },
    /// A runner already owns the project (selection arguments are not applied to it).
    AlreadyRunning {
        pid: i64,
    },
}

/// Last 64 KiB of a log file (lossy UTF-8); empty when it cannot be read. A runner
/// that crashed at startup may leave a huge log; the error is at the end.
fn read_log_tail(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL_BYTES: u64 = 64 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    if f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
        .is_err()
    {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// `atlas dev --detach`: validate, start the runner as a detached daemon and wait
/// until it is reachable.
pub async fn detach(root_dir: &Path, select: &config::SelectionRequest) -> Result<Detached> {
    use std::process::{Command, Stdio};

    // Fail on a bad atlas.yaml / selection here, on the user's terminal, not in a log file.
    let cfg = config::load(root_dir)?;
    config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;

    if let Some(info) = control::info(root_dir).await {
        if info.mode == "runner" {
            return Ok(Detached::AlreadyRunning { pid: info.pid });
        }
        anyhow::bail!(
            "atlas dev is already running in a terminal (pid {}); quit it first, or stop it with atlas down",
            info.pid
        );
    }

    let log_dir = crate::runtime::log_dir(root_dir);
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(RUNNER_LOG);
    let out = std::fs::File::create(&log_path)?;
    let err = out.try_clone()?;

    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("dev")
        .arg("--runner")
        .arg("--dir")
        .arg(root_dir)
        .args(&select.services)
        .args(
            select
                .profiles
                .iter()
                .flat_map(|p| ["--profile", p.as_str()]),
        )
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    if select.no_deps {
        cmd.arg("--no-deps");
    }
    // New session: no controlling terminal, so closing this shell never signals
    // the runner (SIGHUP) and a tty read/write can never stop it (SIGTTIN/OU).
    // Windows: out of this console (closing the window never ends it) and its job.
    let mut child = atlas_core::sys::spawn_detached(&mut cmd)?;
    let pid = child.id() as i32;

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait()? {
            let tail = read_log_tail(&log_path)
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::bail!(
                "runner exited during startup ({status}); see {}\n{tail}",
                log_path.display()
            );
        }
        if let Some(info) = control::info(root_dir).await
            && info.pid == i64::from(pid)
        {
            return Ok(Detached::Started { pid });
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "runner (pid {pid}) did not open its control socket within 60s; see {}",
                log_path.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `atlas down`: ask the running atlas to stop everything and wait for it to go.
pub enum DownOutcome {
    NotRunning,
    Stopped {
        pid: i64,
    },
    /// Still alive after the wait.
    Timeout {
        pid: i64,
    },
}

pub async fn down(root_dir: &Path, wait: Duration) -> Result<DownOutcome, String> {
    let Some(info) = control::info(root_dir).await else {
        return Ok(DownOutcome::NotRunning);
    };
    let resp = control::call(root_dir, "shutdown", json!({})).await?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "shutdown refused".into()));
    }
    Ok(wait_gone(root_dir, info.pid, wait).await)
}

/// Wait until the atlas with `pid` has exited (process dead, or its lock free, so a
/// recycled pid cannot fool us).
pub async fn wait_gone(root_dir: &Path, pid: i64, wait: Duration) -> DownOutcome {
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        let alive = atlas_core::sys::pid_alive(pid as u32);
        if !alive || SingletonLock::existing_pid(&root_dir.join(".logs/.tui.lock")).is_none() {
            return DownOutcome::Stopped { pid };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    DownOutcome::Timeout { pid }
}
