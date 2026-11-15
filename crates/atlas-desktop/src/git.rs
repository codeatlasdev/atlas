//! Review data for a worktree: parsed diff, status, log. Everything runs
//! through hostd `exec` (same path for SSH hosts).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use tauri::State;

use crate::host::HostHub;
use crate::worktree::git;

use atlas_core::git_review::{FileDiff, Hunk, Line, UNTRACKED_MAX, base_for, parse, status_json};

/// An untracked file as an all-added diff; read through hostd so SSH checkouts work.
async fn untracked(hub: &HostHub, dir: &Path, rel: &str) -> FileDiff {
    let p = dir.join(rel).to_string_lossy().into_owned();
    let small = hub
        .call("fs_stat", json!({"path": p}))
        .await
        .ok()
        .and_then(|v| v["size"].as_u64())
        .is_some_and(|n| n <= UNTRACKED_MAX);
    let content = if small {
        hub.read_text(&crate::host::host_for_path(&p), &p).await
    } else {
        None
    };
    let lines: Vec<Line> = content
        .as_deref()
        .unwrap_or("")
        .lines()
        .enumerate()
        .map(|(i, t)| Line {
            kind: "add",
            text: t.to_string(),
            old_no: None,
            new_no: Some(i as u32 + 1),
        })
        .collect();
    let n = lines.len() as u32;
    FileDiff {
        path: rel.to_string(),
        old_path: None,
        status: "untracked",
        staged: false,
        binary: content.is_none(),
        hunks: if content.is_some() && n > 0 {
            vec![Hunk {
                header: format!("@@ -0,0 +1,{n} @@"),
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: n,
                lines,
            }]
        } else {
            Vec::new()
        },
    }
}

fn root(id: &str) -> Result<PathBuf, String> {
    crate::registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| PathBuf::from(e.root))
        .ok_or_else(|| format!("unknown worktree {id}"))
}

/// `mode`: `working` (unstaged + untracked), `staged`, or `base` (vs `base`, incl.
/// uncommitted work). `base` defaults to `branch.<b>.base`, else HEAD.
#[tauri::command]
pub async fn git_diff(
    hub: State<'_, Arc<HostHub>>,
    id: String,
    mode: String,
    base: Option<String>,
) -> Result<Value, String> {
    let dir = root(&id)?;
    let args = ["diff", "--no-color", "-U3", "--no-ext-diff", "-M"];
    let mut files = Vec::new();
    let mut base_used = None;
    match mode.as_str() {
        "staged" => {
            files = parse(
                &git(&hub, &dir, &[&args[..], &["--cached"]].concat()).await?,
                true,
            )
        }
        "working" => {
            files.extend(
                parse(&git(&hub, &dir, &args).await?, true)
                    .into_iter()
                    .map(|mut f| {
                        f.staged = false;
                        f
                    }),
            );
        }
        _ => {
            let b = match base.filter(|b| !b.is_empty()) {
                Some(b) => b,
                None => default_base(&hub, &dir).await,
            };
            let mb = git(&hub, &dir, &["merge-base", "HEAD", &b])
                .await
                .map(|s| s.trim().to_string())
                .unwrap_or(b.clone());
            files = parse(
                &git(&hub, &dir, &[&args[..], &[mb.as_str()]].concat()).await?,
                false,
            );
            base_used = Some(b);
        }
    }
    if mode != "staged" {
        let list = git(
            &hub,
            &dir,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )
        .await?;
        for rel in list.split('\0').filter(|s| !s.is_empty()) {
            files.push(untracked(&hub, &dir, rel).await);
        }
    }
    Ok(json!({"files": files, "base": base_used}))
}

type GitFuture<'a> = std::pin::Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// `git` through hostd as the plain closure `status_json` takes (the args are copied so the future borrows no slice).
fn hub_git<'a>(hub: &'a HostHub, dir: &'a Path) -> impl Fn(&[&str]) -> GitFuture<'a> + 'a {
    move |args| {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        Box::pin(async move {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            git(hub, dir, &args).await
        })
    }
}

async fn default_base(hub: &HostHub, dir: &Path) -> String {
    let branch = git(hub, dir, &["branch", "--show-current"])
        .await
        .unwrap_or_default();
    base_for(Some(branch.trim()), &hub_git(hub, dir)).await
}

/// Porcelain v2 status: branch, upstream, ahead/behind vs base, entries (ignored ones included, `x`/`y` = `!`).
/// One `status --branch` plus the base lookup (see `status_json`); the frontend debounces how often this runs.
#[tauri::command]
pub async fn git_status(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Value, String> {
    let dir = root(&id)?;
    status_json(&dir, &hub_git(&hub, &dir)).await
}

/// `git <args>` in the worktree (stage/unstage/commit/push…); argv is restricted to git.
#[tauri::command]
pub async fn git_run(
    hub: State<'_, Arc<HostHub>>,
    id: String,
    args: Vec<String>,
    stdin: Option<String>,
) -> Result<Value, String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let dir = root(&id)?;
    let mut argv: Vec<String> = vec![
        "git".into(),
        "-C".into(),
        dir.to_string_lossy().into_owned(),
    ];
    argv.extend(args);
    let env = crate::shell_env::vars_json_async().await;
    let r = hub
        .call(
            "exec",
            json!({"cwd": dir, "argv": argv, "env": env,
                   "stdin": stdin.map(|s| b64.encode(s)), "timeout_ms": 120_000}),
        )
        .await?;
    let dec = |k: &str| {
        String::from_utf8_lossy(&b64.decode(r[k].as_str().unwrap_or("")).unwrap_or_default())
            .into_owned()
    };
    Ok(json!({"code": r["code"], "stdout": dec("stdout"), "stderr": dec("stderr")}))
}

/// Tracked + untracked (not ignored) files, for Quick Open.
#[tauri::command]
pub async fn git_files(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Vec<String>, String> {
    let dir = root(&id)?;
    let out = git(
        &hub,
        &dir,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )
    .await?;
    let mut v: Vec<String> = out
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    v.dedup();
    Ok(v)
}
