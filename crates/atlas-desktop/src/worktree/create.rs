//! Creating a worktree: settings lookup (workspace dir, branch prefix), the progress
//! reporting `worktree_create` streams to the frontend, and the shared-path linking.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use super::exec::{default_base, exec, exec_on, git, git_on, ref_exists};
use super::naming::{free_offset, random_sea_name, slug};
use crate::host::HostHub;
use crate::registry::{self, Entry};

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
pub(super) async fn create(
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
            "argv": super::exec::shell_argv(&host, &cmd),
            "env": hook_env(&root, &real, &s, offset),
        })
    });
    Ok(json!({
        "worktree": {"id": entry.id, "path": entry.root, "branch": branch, "base": base,
                     "portOffset": offset, "repoId": repo.id, "name": s, "linkedPr": args.pr},
        "setup": setup,
    }))
}

/// Windows, local checkout: what the POSIX `share` script does, natively. A directory is linked
/// (a symlink when Developer Mode allows one, else a junction, which needs no rights); a file is
/// copied; nothing happens when the source is missing or the target exists.
#[cfg(windows)]
fn share_local(src: &Path, dst: &Path) -> std::io::Result<()> {
    use atlas_core::sys::CommandExt as _;
    use std::os::windows::process::CommandExt as _;
    if !src.exists() || dst.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !src.is_dir() {
        return std::fs::copy(src, dst).map(|_| ());
    }
    if std::os::windows::fs::symlink_dir(src, dst).is_ok() {
        return Ok(());
    }
    let win = |p: &Path| p.to_string_lossy().replace('/', "\\");
    let st = std::process::Command::new("cmd.exe")
        .raw_arg(format!("/d /c mklink /J \"{}\" \"{}\"", win(dst), win(src)))
        .no_window()
        .stdout(std::process::Stdio::null())
        .status()?;
    if st.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("mklink /J failed"))
    }
}

pub(super) fn hook_env(root: &Path, wt: &Path, name: &str, offset: u16) -> Value {
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
    #[cfg(windows)]
    if host == crate::host::LOCAL {
        return share_local(&src, &dst).map_err(|e| format!("{rel}: {e}"));
    }
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
