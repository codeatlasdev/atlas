//! Running commands for worktree/git work: every call goes through hostd `exec`, so a
//! local checkout and an SSH one take the same path.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::json;

use crate::host::HostHub;

pub(crate) struct Out {
    pub code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

/// Run `argv` in `cwd` on the host that owns `cwd` (see `HostHub::call`).
pub(crate) async fn exec(
    hub: &HostHub,
    cwd: &Path,
    argv: &[&str],
    timeout_ms: u64,
) -> Result<Out, String> {
    exec_on(
        hub,
        &crate::host::host_for_path(&cwd.to_string_lossy()),
        cwd,
        argv,
        timeout_ms,
    )
    .await
}

/// [`exec`] on an explicit host (paths not registered yet, e.g. a new worktree).
pub(super) async fn exec_on(
    hub: &HostHub,
    host: &str,
    cwd: &Path,
    argv: &[&str],
    timeout_ms: u64,
) -> Result<Out, String> {
    let r = hub
        .call(
            "exec",
            json!({"host": host, "cwd": cwd, "argv": argv, "timeout_ms": timeout_ms}),
        )
        .await?;
    let dec = |k: &str| {
        String::from_utf8_lossy(&B64.decode(r[k].as_str().unwrap_or("")).unwrap_or_default())
            .into_owned()
    };
    if r["timed_out"] == true {
        return Err(format!("{} timed out", argv.join(" ")));
    }
    Ok(Out {
        code: r["code"].as_i64(),
        stdout: dec("stdout"),
        stderr: dec("stderr"),
    })
}

/// `git -C dir args…`; non-zero exit → stderr as the error.
pub(crate) async fn git(hub: &HostHub, dir: &Path, args: &[&str]) -> Result<String, String> {
    git_on(
        hub,
        &crate::host::host_for_path(&dir.to_string_lossy()),
        dir,
        args,
    )
    .await
}

pub(super) async fn git_on(
    hub: &HostHub,
    host: &str,
    dir: &Path,
    args: &[&str],
) -> Result<String, String> {
    let mut argv = vec!["git", "-C"];
    let d = dir.to_string_lossy();
    argv.push(&d);
    argv.extend_from_slice(args);
    let o = exec_on(hub, host, dir, &argv, 60_000).await?;
    if o.code == Some(0) {
        Ok(o.stdout)
    } else {
        Err(format!("git {}: {}", args.join(" "), o.stderr.trim()))
    }
}

/// `origin/<default>` when the remote has a HEAD, else `HEAD`.
pub(super) async fn default_base(hub: &HostHub, repo: &Path) -> String {
    git(
        hub,
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    .map(|s| s.trim().to_string())
    .ok()
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "HEAD".into())
}

pub(super) async fn ref_exists(hub: &HostHub, repo: &Path, r: &str) -> bool {
    git(hub, repo, &["rev-parse", "--verify", "--quiet", r])
        .await
        .is_ok()
}
