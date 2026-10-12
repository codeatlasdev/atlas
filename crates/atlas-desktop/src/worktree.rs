//! Git worktrees as registry entries (`parent` = main checkout's id).
//!
//! Every git/gh/sh call goes through hostd `exec` (same path a remote host
//! will use). Creation is async: `worktree_create` returns a `creation_id` and
//! reports `worktree://progress {creation_id, step, message, worktree?, setup?}`
//! with step `fetch|add|shared|done|error`. The frontend opens the "Setup"
//! terminal from the `done` payload (it owns tabs).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use crate::host::HostHub;
use crate::live::LiveHub;
use crate::registry::{self, Entry};

const SEA_NAMES: [&str; 48] = [
    "anchovy",
    "barracuda",
    "beluga",
    "blenny",
    "clownfish",
    "cod",
    "coral",
    "cuttlefish",
    "dolphin",
    "dugong",
    "eel",
    "flounder",
    "goby",
    "grouper",
    "haddock",
    "halibut",
    "herring",
    "jellyfish",
    "krill",
    "lamprey",
    "limpet",
    "lobster",
    "mackerel",
    "manatee",
    "marlin",
    "minnow",
    "narwhal",
    "nautilus",
    "octopus",
    "orca",
    "oyster",
    "pufferfish",
    "remora",
    "salmon",
    "sardine",
    "scallop",
    "seahorse",
    "shrimp",
    "snapper",
    "squid",
    "starfish",
    "stingray",
    "sturgeon",
    "swordfish",
    "tuna",
    "turbot",
    "urchin",
    "walrus",
];

pub struct Out {
    pub code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

/// Run `argv` in `cwd` on the host that owns `cwd` (see `HostHub::call`).
pub async fn exec(
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
pub async fn exec_on(
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
pub async fn git(hub: &HostHub, dir: &Path, args: &[&str]) -> Result<String, String> {
    git_on(
        hub,
        &crate::host::host_for_path(&dir.to_string_lossy()),
        dir,
        args,
    )
    .await
}

pub async fn git_on(
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

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    out.chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

fn random_sea_name() -> String {
    let n = atlas_host::paths::random_hex(2)
        .ok()
        .and_then(|h| usize::from_str_radix(&h, 16).ok())
        .unwrap_or(0);
    SEA_NAMES[n % SEA_NAMES.len()].to_string()
}

/// Smallest free multiple of 100 among the repo's worktrees.
fn free_offset(entries: &[Entry], repo: &str) -> u16 {
    (1..100)
        .map(|k| k * 100)
        .find(|o| {
            !entries
                .iter()
                .any(|e| e.parent.as_deref() == Some(repo) && e.port_offset == *o)
        })
        .unwrap_or(0)
}

fn setting(key: &str) -> Option<String> {
    crate::shell_env::load_settings()
        .extra
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn workspace_dir() -> PathBuf {
    setting("workspaceDir")
        .map(|s| PathBuf::from(atlas_core::util::expand_tilde(&s)))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".atlas/worktrees")
        })
}

async fn branch_prefix(hub: &HostHub, repo: &Path) -> Option<String> {
    match setting("branchPrefix").as_deref().unwrap_or("git-username") {
        "none" => None,
        "custom" => setting("branchPrefixCustom")
            .map(|s| slug(&s))
            .filter(|s| !s.is_empty()),
        _ => git(hub, repo, &["config", "user.name"])
            .await
            .ok()
            .map(|n| slug(n.trim()))
            .filter(|s| !s.is_empty()),
    }
}

/// `origin/<default>` when the remote has a HEAD, else `HEAD`.
pub async fn default_base(hub: &HostHub, repo: &Path) -> String {
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

async fn ref_exists(hub: &HostHub, repo: &Path, r: &str) -> bool {
    git(hub, repo, &["rev-parse", "--verify", "--quiet", r])
        .await
        .is_ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
    pub repo_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "yes")]
    pub run_setup: bool,
    /// GitHub PR number: check out its head branch.
    #[serde(default)]
    pub pr: Option<u64>,
}

fn yes() -> bool {
    true
}

#[tauri::command]
pub fn worktree_create(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    args: CreateArgs,
) -> Result<Value, String> {
    let id = atlas_host::paths::new_id();
    let (app2, hub, cid) = (app.clone(), hub.inner().clone(), id.clone());
    tauri::async_runtime::spawn(async move {
        let progress = |step: &str, message: &str| {
            let _ = app2.emit(
                "worktree://progress",
                json!({"creation_id": cid, "step": step, "message": message}),
            );
        };
        match create(&app2, &hub, args, &progress).await {
            Ok(done) => {
                let mut v = done;
                v["creation_id"] = json!(cid);
                v["step"] = json!("done");
                let _ = app2.emit("worktree://progress", v);
            }
            Err(e) => progress("error", &e),
        }
    });
    Ok(json!({"creation_id": id}))
}

/// Shared by the UI command and the control socket (`atlas wt create`).
pub async fn create(
    app: &AppHandle,
    hub: &HostHub,
    args: CreateArgs,
    progress: &(dyn Fn(&str, &str) + Send + Sync),
) -> Result<Value, String> {
    let entries = registry::load()?;
    let repo = entries
        .iter()
        .find(|e| e.id == args.repo_id)
        .cloned()
        .ok_or_else(|| format!("unknown project {}", args.repo_id))?;
    let repo = match &repo.parent {
        Some(p) => entries
            .iter()
            .find(|e| &e.id == p)
            .cloned()
            .ok_or("parent missing")?,
        None => repo,
    };
    let root = PathBuf::from(&repo.root);
    let host = repo.host().to_string();
    let repo_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut existing_branch: Option<String> = None;
    let mut name = args.name.trim().to_string();
    if let Some(n) = args.pr {
        let o = exec(
            hub,
            &root,
            &[
                "gh",
                "pr",
                "view",
                &n.to_string(),
                "--json",
                "headRefName,title",
            ],
            30_000,
        )
        .await?;
        if o.code != Some(0) {
            return Err(format!("gh pr view {n}: {}", o.stderr.trim()));
        }
        let v: Value = serde_json::from_str(&o.stdout).map_err(|e| e.to_string())?;
        existing_branch = v["headRefName"].as_str().map(str::to_string);
        if name.is_empty() {
            name = existing_branch.clone().unwrap_or_default();
        }
    }
    let mut s = slug(&name);
    if s.is_empty() {
        s = random_sea_name();
    }
    let branch = match (
        args.branch.filter(|b| !b.trim().is_empty()),
        existing_branch,
    ) {
        (Some(b), _) | (None, Some(b)) => b,
        (None, None) => match branch_prefix(hub, &root).await {
            Some(p) => format!("{p}/{s}"),
            None => s.clone(),
        },
    };
    let ws_dir = if repo.is_remote() {
        PathBuf::from(hub.home(&host).await?).join(".atlas/worktrees")
    } else {
        workspace_dir()
    };
    let parent_dir = if setting("nestWorkspaces").as_deref() == Some("false") {
        ws_dir
    } else {
        ws_dir.join(&repo_name)
    };
    let mut dir = parent_dir.join(&s);
    let mut k = 2;
    while hub.exists(&host, &dir.to_string_lossy()).await {
        dir = parent_dir.join(format!("{s}-{k}"));
        k += 1;
    }
    hub.mkdir(&host, &parent_dir.to_string_lossy()).await?;
    let dir_s = dir.to_string_lossy().into_owned();

    progress("fetch", "git fetch origin");
    let _ = git(hub, &root, &["fetch", "origin", "--prune"]).await;
    let base = match args.base.filter(|b| !b.trim().is_empty()) {
        Some(b) => b,
        None => default_base(hub, &root).await,
    };

    progress("add", &format!("git worktree add {dir_s}"));
    let remote = format!("origin/{branch}");
    if ref_exists(hub, &root, &format!("refs/heads/{branch}")).await {
        git(hub, &root, &["worktree", "add", &dir_s, &branch]).await?;
    } else if ref_exists(hub, &root, &format!("refs/remotes/{remote}")).await {
        git(
            hub,
            &root,
            &["worktree", "add", "--track", "-b", &branch, &dir_s, &remote],
        )
        .await?;
    } else {
        git(
            hub,
            &root,
            &[
                "worktree",
                "add",
                "--no-track",
                "-b",
                &branch,
                &dir_s,
                &base,
            ],
        )
        .await?;
    }
    let _ = git_on(
        hub,
        &host,
        &dir,
        &["config", "--local", &format!("branch.{branch}.base"), &base],
    )
    .await;
    if git_on(
        hub,
        &host,
        &dir,
        &["config", "--get", "push.autoSetupRemote"],
    )
    .await
    .is_err()
    {
        let _ = git_on(
            hub,
            &host,
            &dir,
            &["config", "--local", "push.autoSetupRemote", "true"],
        )
        .await;
    }

    let cfg = hub.load_config(&host, &repo.root, 0).await.ok();
    let wcfg = cfg
        .as_ref()
        .and_then(|c| c.worktree.clone())
        .unwrap_or_default();
    if !wcfg.shared.is_empty() {
        progress("shared", &wcfg.shared.join(", "));
        for rel in &wcfg.shared {
            share(hub, &host, &root, &dir, rel).await?;
        }
    }

    let real = exec_on(hub, &host, &dir, &["realpath", &dir_s], 10_000)
        .await
        .ok()
        .filter(|o| o.code == Some(0))
        .map(|o| PathBuf::from(o.stdout.trim()))
        .unwrap_or(dir.clone());
    let mut entries = registry::load()?;
    let offset = free_offset(&entries, &repo.id);
    let entry = Entry {
        id: registry::id_for_host(repo.host.as_deref(), &real),
        root: real.to_string_lossy().into_owned(),
        pinned: false,
        added: chrono::Utc::now().to_rfc3339(),
        parent: Some(repo.id.clone()),
        port_offset: offset,
        branch: Some(branch.clone()),
        host: repo.host.clone(),
        identity: None,
    };
    entries.retain(|e| e.id != entry.id);
    entries.push(entry.clone());
    registry::save(&entries)?;
    let _ = app.emit("projects://changed", ());

    let setup = wcfg.setup.filter(|_| args.run_setup).map(|cmd| {
        json!({
            "argv": ["sh", "-c", cmd],
            "env": hook_env(&root, &real, &s, offset),
        })
    });
    Ok(json!({
        "worktree": {"id": entry.id, "path": entry.root, "branch": branch, "base": base,
                     "portOffset": offset, "repoId": repo.id, "name": s, "linkedPr": args.pr},
        "setup": setup,
    }))
}

fn hook_env(root: &Path, wt: &Path, name: &str, offset: u16) -> Value {
    json!({
        "ATLAS_ROOT_PATH": root,
        "ATLAS_WORKTREE_PATH": wt,
        "ATLAS_WORKSPACE_NAME": name,
        "ATLAS_PORT_OFFSET": offset.to_string(),
    })
}

/// Link a gitignored dir (symlink) or copy a file from the main checkout, on its host.
async fn share(hub: &HostHub, host: &str, root: &Path, wt: &Path, rel: &str) -> Result<(), String> {
    if rel.contains("..") || Path::new(rel).is_absolute() {
        return Err(format!(
            "worktree.shared: {rel} must be relative to the repo"
        ));
    }
    let (src, dst) = (root.join(rel), wt.join(rel));
    // Positional args keep paths out of the shell string.
    let script = r#"[ -e "$1" ] || exit 0; [ -e "$2" ] && exit 0; mkdir -p "$(dirname "$2")"; if [ -d "$1" ]; then ln -s "$1" "$2"; else cp "$1" "$2"; fi"#;
    let (s, d) = (src.to_string_lossy(), dst.to_string_lossy());
    let o = exec_on(
        hub,
        host,
        wt,
        &["sh", "-c", script, "share", &s, &d],
        60_000,
    )
    .await?;
    if o.code == Some(0) {
        Ok(())
    } else {
        Err(format!("{rel}: {}", o.stderr.trim()))
    }
}

#[tauri::command]
pub async fn worktree_remove(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    live: State<'_, LiveHub>,
    id: String,
    force: bool,
) -> Result<Value, String> {
    let res = remove(&hub, &id, force).await?;
    live.remove(&id);
    let _ = app.emit("projects://changed", ());
    Ok(res)
}

pub async fn remove(hub: &HostHub, id: &str, force: bool) -> Result<Value, String> {
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
        let mut argv: Vec<String> = vec!["env".into()];
        for (k, v) in env.as_object().into_iter().flatten() {
            argv.push(format!("{k}={}", v.as_str().unwrap_or_default()));
        }
        argv.extend(["sh".into(), "-c".into(), cmd]);
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
    if let Some(b) = &e.branch {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Fix: Login bug!!"), "fix-login-bug");
        assert_eq!(slug("  --  "), "");
        assert_eq!(slug(&"a".repeat(60)).len(), 40);
    }

    #[test]
    fn offsets_skip_used_slots_per_repo() {
        let e = |parent: &str, o| Entry {
            id: String::new(),
            root: String::new(),
            pinned: false,
            added: String::new(),
            parent: Some(parent.into()),
            port_offset: o,
            branch: None,
            host: None,
            identity: None,
        };
        assert_eq!(free_offset(&[], "r"), 100);
        assert_eq!(
            free_offset(&[e("r", 100), e("r", 300), e("x", 200)], "r"),
            200
        );
    }
}
