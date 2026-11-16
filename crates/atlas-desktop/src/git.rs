//! Review data for a worktree: parsed diff, status, log. Everything runs
//! through hostd `exec` (same path for SSH hosts).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use tauri::State;

use crate::host::HostHub;
use crate::worktree::git;

use atlas_core::git_review::{
    FileDiff, Hunk, Line, REVIEW_DIFF, UNTRACKED_MAX, base_for, parse, status_json,
};

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
    let args = REVIEW_DIFF;
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

/// Quick Open walks at most this many files of a folder that is not a git repository, for at most this long.
const PLAIN_FOLDER_FILES: usize = 20_000;
const PLAIN_FOLDER_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Tracked + untracked (not ignored) files, for Quick Open. A folder with no `.git` (a plain folder) has no index to
/// ask, so it is walked instead (`atlas_core::filelist`), off the async threads and bounded.
#[tauri::command]
pub async fn git_files(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Vec<String>, String> {
    let dir = root(&id)?;
    if dir.is_dir() && !dir.join(".git").exists() {
        let listing = tokio::task::spawn_blocking(move || {
            atlas_core::filelist::list_files(&dir, PLAIN_FOLDER_FILES, PLAIN_FOLDER_BUDGET)
        })
        .await
        .map_err(|e| e.to_string())?;
        return Ok(listing.files);
    }
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

/// Name and email of the GitHub account `gh` is signed in as, to prefill the commit identity form.
/// `gh` missing or signed out is `None` (the form just starts empty).
#[tauri::command]
pub async fn gh_identity(
    hub: State<'_, Arc<HostHub>>,
    id: String,
) -> Result<Option<Value>, String> {
    let dir = root(&id)?;
    let Ok(out) = crate::worktree::exec(&hub, &dir, &["gh", "api", "user"], 15_000).await else {
        return Ok(None);
    };
    if out.code != Some(0) {
        return Ok(None);
    }
    Ok(identity_from_gh(&out.stdout).map(|(name, email)| json!({"name": name, "email": email})))
}

/// `gh api user` → `(name, email)`. A private profile has no `name`/`email`: the login and GitHub's no-reply
/// address (`<id>+<login>@users.noreply.github.com`, the one GitHub itself accepts for commits) stand in.
fn identity_from_gh(json: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(json).ok()?;
    let text = |k: &str| v[k].as_str().map(str::trim).filter(|s| !s.is_empty());
    let login = text("login")?;
    let name = text("name").unwrap_or(login);
    let email = match (text("email"), v["id"].as_u64()) {
        (Some(e), _) => e.to_string(),
        (None, Some(id)) => format!("{id}+{login}@users.noreply.github.com"),
        (None, None) => format!("{login}@users.noreply.github.com"),
    };
    Some((name.to_string(), email))
}

#[cfg(test)]
mod identity_tests {
    use super::identity_from_gh;

    #[test]
    fn a_public_profile_gives_its_name_and_email() {
        let j = r#"{"login":"octo","id":583231,"name":"The Octocat","email":"octo@github.com"}"#;
        assert_eq!(
            identity_from_gh(j),
            Some(("The Octocat".into(), "octo@github.com".into()))
        );
    }

    #[test]
    fn a_private_profile_falls_back_to_login_and_the_noreply_address() {
        let j = r#"{"login":"octo","id":583231,"name":null,"email":null}"#;
        assert_eq!(
            identity_from_gh(j),
            Some(("octo".into(), "583231+octo@users.noreply.github.com".into()))
        );
        let blank = r#"{"login":"octo","id":7,"name":"  ","email":""}"#;
        assert_eq!(
            identity_from_gh(blank),
            Some(("octo".into(), "7+octo@users.noreply.github.com".into()))
        );
    }

    #[test]
    fn gh_errors_and_garbage_give_nothing() {
        assert_eq!(identity_from_gh(""), None);
        assert_eq!(identity_from_gh("not json"), None);
        assert_eq!(identity_from_gh(r#"{"message":"Bad credentials"}"#), None);
    }
}
