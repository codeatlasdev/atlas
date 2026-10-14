//! System-ssh transport. Every script runs as `bash -c <script>` on the host
//! (or locally for `host: local`), so `~/.ssh/config` aliases and pinned
//! known_hosts apply. Every interpolated value MUST go through [`shq`].

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Single-quote for POSIX shells: `a'b` → `'a'\''b'`.
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub struct Remote {
    pub host: String,
}

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Remote {
    pub fn new(host: &str) -> Self {
        Self { host: host.into() }
    }

    fn is_local(&self) -> bool {
        self.host == "local"
    }

    fn command(&self, script: &str) -> Command {
        if self.is_local() {
            let mut c = Command::new("bash");
            c.arg("-c").arg(script);
            return c;
        }
        let mut c = Command::new("ssh");
        c.args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=4",
            "-o",
            "ControlMaster=auto",
            "-o",
            "ControlPath=~/.ssh/atlas-%C",
            "-o",
            "ControlPersist=120",
        ])
        .arg(&self.host)
        .arg("--")
        .arg(format!("bash -c {}", shq(script)));
        c
    }

    async fn run(&self, script: &str, stdin: Option<&[u8]>, file: Option<&Path>) -> Result<Output> {
        let mut cmd = self.command(script);
        cmd.stdin(if stdin.is_some() || file.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
        let mut child = cmd.spawn().context("spawning ssh")?;
        if let Some(mut pipe) = child.stdin.take() {
            if let Some(data) = stdin {
                pipe.write_all(data).await?;
            }
            if let Some(f) = file {
                let mut src = tokio::fs::File::open(f).await?;
                tokio::io::copy(&mut src, &mut pipe).await?;
            }
            pipe.shutdown().await.ok();
        }
        let out = child.wait_with_output().await?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    /// Run a read-only or idempotent script; retried (1s, 3s) only when ssh
    /// itself fails (exit 255).
    pub async fn exec(&self, script: &str) -> Result<Output> {
        self.exec_stdin(script, None).await
    }

    /// Like [`exec`](Self::exec) with `data` piped to the script's stdin.
    pub async fn exec_stdin(&self, script: &str, data: Option<&[u8]>) -> Result<Output> {
        let mut last = None;
        for delay in [0u64, 1, 3] {
            if delay > 0 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            let out = self.run(script, data, None).await?;
            if out.code != 255 || self.is_local() {
                return Ok(out);
            }
            last = Some(out);
        }
        Ok(last.expect("at least one attempt"))
    }

    /// Never retried: for the one non-idempotent call (launching the driver).
    pub async fn exec_once(&self, script: &str) -> Result<Output> {
        self.run(script, None, None).await
    }

    /// Stream `file` into `<dir>/artifact.tar.gz`; a retry wipes `dir` first.
    pub async fn upload(&self, file: &Path, dir: &str) -> Result<()> {
        let script = format!(
            "rm -rf {d} && mkdir -p {d} && cat > {d}/artifact.tar.gz",
            d = shq(dir)
        );
        let mut last = String::new();
        for delay in [0u64, 1, 3] {
            if delay > 0 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            let out = self.run(&script, None, Some(file)).await?;
            if out.code == 0 {
                return Ok(());
            }
            last = out.stderr;
            if out.code != 255 {
                break;
            }
        }
        anyhow::bail!("upload to {} failed: {}", self.host, last.trim())
    }

    /// Run with inherited stdout/stderr (for `logs -f`).
    pub async fn stream(&self, script: &str) -> Result<i32> {
        let status = self.command(script).stdin(Stdio::null()).status().await?;
        Ok(status.code().unwrap_or(-1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shq_escapes_single_quote() {
        assert_eq!(shq("a'b"), "'a'\\''b'");
        assert_eq!(shq("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    // The `local` deploy host is this machine's bash.
    #[cfg(unix)]
    #[tokio::test]
    async fn local_exec_round_trips_quotes() {
        let r = Remote::new("local");
        let out = r
            .exec(&format!("printf %s {}", shq("it's $HOME")))
            .await
            .unwrap();
        assert_eq!(out.stdout, "it's $HOME");
    }
}
