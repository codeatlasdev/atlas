//! Managing existing worktrees: remove (with the archive hook), the base-ref list,
//! importing worktrees made outside Atlas, and the cleanup-list stats.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use super::create::hook_env;
use super::exec::{exec, exec_on, git};
use super::naming::free_offset;
use crate::host::HostHub;
use crate::live::LiveHub;
use crate::registry::{self, Entry};

#[tauri::command]
pub async fn worktree_remove(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    live: State<'_, LiveHub>,
    id: String,
    force: bool,
) -> Result<Value, String> {
    let res = remove(&hub, &id, force, false).await?;
    live.remove(&id);
    let _ = app.emit("projects://changed", ());
    Ok(res)
}

/// Auto clean up (#58): remove a worktree only when nothing would be lost, never with `--force`, and keep its
/// branch so the frontend can restore it. With `merged`, Rust asks `gh` itself: the worktree is removed only when
/// its branch's PR is MERGED, and only commits up to that PR's head count as pushed.
/// Returns `{kept: "not_merged"|"dirty"|"secrets"|"unpushed"}` or `{removed: true, branch}`.
#[tauri::command]
pub async fn worktree_auto_remove(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    live: State<'_, LiveHub>,
    id: String,
    merged: bool,
) -> Result<Value, String> {
    use crate::git_branch::{gh_runner, hostd_runner, keep_reason, merged_head};
    let e = registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("unknown worktree")?;
    let dir = PathBuf::from(&e.root);
    let head = match (merged, &e.branch) {
        (false, _) => None,
        (true, Some(b)) => match merged_head(&gh_runner(hub.inner().clone(), dir.clone()), b).await
        {
            Some(h) => Some(h),
            None => return Ok(json!({"kept": "not_merged"})),
        },
        (true, None) => return Ok(json!({"kept": "not_merged"})),
    };
    if let Some(why) = keep_reason(&hostd_runner(hub.inner().clone(), dir), head.as_deref()).await?
    {
        return Ok(json!({"kept": why}));
    }
    remove(&hub, &id, false, true).await?;
    live.remove(&id);
    let _ = app.emit("projects://changed", ());
    Ok(json!({"removed": true, "branch": e.branch}))
}

async fn remove(hub: &HostHub, id: &str, force: bool, keep_branch: bool) -> Result<Value, String> {
    let entries = registry::load()?;
    let e = entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or("unknown worktree")?;
    let parent = e
        .parent
        .as_ref()
        .ok_or("the main checkout cannot be deleted")?;
    let root = PathBuf::from(
        &entries
            .iter()
            .find(|x| &x.id == parent)
            .ok_or("parent missing")?
            .root,
    );
    let wt = PathBuf::from(&e.root);
    let host = e.host().to_string();
    let wt_s = wt.to_string_lossy().into_owned();
    let exists = hub.exists(&host, &wt_s).await;
    if e.is_remote() {
        let _ = exec_on(hub, &host, &wt, &["atlas", "down", "--dir", &wt_s], 30_000).await;
    } else {
        let _ = atlas_tui::runner::down(&wt, std::time::Duration::from_secs(10)).await;
    }
    let root_s = root.to_string_lossy().into_owned();
    if let Some(cmd) = hub
        .load_config(&host, &root_s, 0)
        .await
        .ok()
        .and_then(|c| c.worktree)
        .and_then(|w| w.archive)
        && exists
    {
        let name = wt
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let env = hook_env(&root, &wt, &name, e.port_offset);
        let vars = env.as_object().into_iter().flatten();
        let argv: Vec<String> = if cfg!(windows) && host == crate::host::LOCAL {
            // cmd has no `env`: `set "K=V"` before the script (quoted, so `&` in a path is data).
            let sets: String = vars
                .map(|(k, v)| format!("set \"{k}={}\"&& ", v.as_str().unwrap_or_default()))
                .collect();
            super::exec::shell_argv(&host, &format!("{sets}{cmd}"))
        } else {
            let mut argv: Vec<String> = vec!["env".into()];
            for (k, v) in vars {
                argv.push(format!("{k}={}", v.as_str().unwrap_or_default()));
            }
            argv.extend(["sh".into(), "-c".into(), cmd]);
            argv
        };
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let _ = exec(hub, &wt, &refs, 120_000).await;
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&wt_s);
    if exists {
        git(hub, &root, &args).await?;
    } else {
        let _ = git(hub, &root, &["worktree", "prune"]).await;
    }
    let mut preserved = None;
    if let Some(b) = e.branch.as_ref().filter(|_| !keep_branch) {
        let flag = if force { "-D" } else { "-d" };
        if git(hub, &root, &["branch", flag, b]).await.is_err() {
            preserved = Some(b.clone());
        }
    }
    let mut all = registry::load()?;
    all.retain(|x| x.id != e.id);
    registry::save(&all)?;
    Ok(json!({"preserved_branch": preserved}))
}

/// Local and remote branch names (base-ref picker).
#[tauri::command]
pub async fn git_refs(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Vec<String>, String> {
    let root = PathBuf::from(
        &registry::load()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or("unknown project")?
            .root,
    );
    let out = git(
        &hub,
        &root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .await?;
    Ok(out
        .lines()
        .filter(|l| !l.ends_with("/HEAD") && *l != "origin")
        .map(str::to_string)
        .collect())
}

/// Register worktrees of `repo_id` created outside Atlas (`git worktree add`).
#[tauri::command]
pub async fn worktrees_import(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    repo_id: String,
) -> Result<u32, String> {
    let mut entries = registry::load()?;
    let repo = entries
        .iter()
        .find(|e| e.id == repo_id && e.parent.is_none())
        .cloned()
        .ok_or("unknown project")?;
    let out = git(
        &hub,
        Path::new(&repo.root),
        &["worktree", "list", "--porcelain"],
    )
    .await?;
    let mut added = 0;
    for block in out.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        for l in block.lines() {
            if let Some(p) = l.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = l.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            }
        }
        let Some(p) = path else { continue };
        let p_s = p.to_string_lossy().into_owned();
        let id = registry::id_for_host(repo.host.as_deref(), &p);
        if entries.iter().any(|e| e.id == id)
            || !hub.exists(repo.host(), &format!("{p_s}/atlas.yaml")).await
        {
            continue;
        }
        let offset = free_offset(&entries, &repo.id);
        entries.push(Entry {
            id,
            root: p_s,
            pinned: false,
            added: chrono::Utc::now().to_rfc3339(),
            parent: Some(repo.id.clone()),
            port_offset: offset,
            branch,
            host: repo.host.clone(),
            identity: None,
        });
        added += 1;
    }
    if added > 0 {
        registry::save(&entries)?;
        let _ = app.emit("projects://changed", ());
    }
    Ok(added)
}

/// Size, last commit and git state for the cleanup list.
#[tauri::command]
pub async fn worktree_stats(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Value, String> {
    let e = registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("unknown worktree")?;
    let dir = PathBuf::from(&e.root);
    let du = exec(&hub, &dir, &["du", "-sk", "."], 60_000).await.ok();
    let kb = du.and_then(|o| {
        o.stdout
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<u64>().ok())
    });
    let dirty = git(&hub, &dir, &["status", "--porcelain"])
        .await
        .map(|s| !s.trim().is_empty())
        .ok();
    let base = match &e.branch {
        Some(b) => git(&hub, &dir, &["config", &format!("branch.{b}.base")])
            .await
            .ok()
            .map(|s| s.trim().to_string()),
        None => None,
    };
    let (ahead, behind) = match &base {
        Some(b) => git(
            &hub,
            &dir,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{b}"),
            ],
        )
        .await
        .ok()
        .and_then(|s| {
            let mut it = s.split_whitespace().filter_map(|n| n.parse::<u64>().ok());
            Some((it.next()?, it.next()?))
        })
        .map_or((None, None), |(a, b)| (Some(a), Some(b))),
        None => (None, None),
    };
    let last = git(&hub, &dir, &["log", "-1", "--format=%ct"])
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    Ok(
        json!({"kb": kb, "dirty": dirty, "ahead": ahead, "behind": behind, "base": base, "last_commit": last}),
    )
}
