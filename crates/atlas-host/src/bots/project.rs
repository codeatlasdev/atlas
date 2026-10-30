//! Bot project checkout and atlas.yaml discovery.
//!
//! Git projects use a shared clone + per-bot worktree pattern:
//!   `<base>/projects/<slug>/`  — shared bare-ish clone (fetchable)
//!   `<base>/bots/<id>/work/`   — git worktree for this bot
//!
//! Path projects are validated and returned as-is.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio::process::Command;

use crate::bots::store::ProjectRef;

// ─── Public types ─────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct AtlasYamlInfo {
    pub name: String,
    pub services: Vec<String>,
    /// Inferred `owner/repo` from the git remote, for github trigger defaults.
    #[allow(dead_code)]
    pub git_remote: Option<String>,
}

// ─── prepare ──────────────────────────────────────────────────────────────────

/// Ensure the project is checked out and return the working directory for the bot.
///
/// For `Git`: shared clone + per-bot worktree.
/// For `Path`: validate the path is an existing directory and return it.
pub async fn prepare(
    project: &ProjectRef,
    bot_id: &str,
    base_dir: &Path,
) -> Result<PathBuf, String> {
    match project {
        ProjectRef::Git { url, branch } => {
            prepare_git(url, branch.as_deref(), bot_id, base_dir).await
        }
        ProjectRef::Path { path } => prepare_path(path),
    }
}

// ─── fetch ────────────────────────────────────────────────────────────────────

/// Fetch latest changes (git only, non-destructive). Call before each turn.
///
/// Fetches in the shared repo, then fast-forward pulls in the worktree only
/// when there are no local modifications.
pub async fn fetch(work_dir: &Path) -> Result<(), String> {
    let work_str = work_dir.to_string_lossy();

    // Find the shared (main) worktree via `git worktree list --porcelain`.
    let list = git_run(&["-C", &work_str, "worktree", "list", "--porcelain"], None)
        .await
        .unwrap_or_default();

    let shared = list
        .lines()
        .find(|l| l.starts_with("worktree "))
        .and_then(|l| l.strip_prefix("worktree "))
        .map(|s| PathBuf::from(s.trim()));

    if let Some(shared_dir) = shared {
        let _ = git_run(
            &["-C", &shared_dir.to_string_lossy(), "fetch", "--quiet"],
            None,
        )
        .await;
    }

    // Pull in the worktree only when the working tree is clean.
    let status = git_run(&["-C", &work_str, "status", "--porcelain"], None)
        .await
        .unwrap_or_default();
    if status.is_empty() {
        let _ = git_run(&["-C", &work_str, "pull", "--ff-only", "--quiet"], None).await;
    }

    Ok(())
}

// ─── atlas.yaml ───────────────────────────────────────────────────────────────

/// Read `atlas.yaml` from `work_dir` and extract the project name, service
/// names, and (if available) the inferred GitHub repo slug.
#[allow(dead_code)]
pub fn read_atlas_yaml(work_dir: &Path) -> Option<AtlasYamlInfo> {
    let content = std::fs::read_to_string(work_dir.join("atlas.yaml")).ok()?;

    let name = content
        .lines()
        .find(|l| l.starts_with("name:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();

    let mut in_services = false;
    let mut services = Vec::new();
    for line in content.lines() {
        if line.starts_with("services:") {
            in_services = true;
            continue;
        }
        if in_services {
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_services = false;
                continue;
            }
            // Handles both `  name: foo` and `  - name: foo` (list-item) formats.
            let trimmed = line.trim_start_matches([' ', '\t', '-'].as_ref());
            if let Some((k, v)) = trimmed.split_once(':')
                && k.trim() == "name"
            {
                services.push(v.trim().to_string());
            }
        }
    }

    let git_remote = infer_github_repo(work_dir);
    Some(AtlasYamlInfo {
        name,
        services,
        git_remote,
    })
}

/// Infer `owner/repo` from the `origin` git remote in `work_dir`.
///
/// Handles both HTTPS (`https://github.com/owner/repo`) and SSH
/// (`git@github.com:owner/repo`) remotes.
#[allow(dead_code)]
pub fn infer_github_repo(work_dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &work_dir.to_string_lossy(),
            "remote",
            "get-url",
            "origin",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let remote = String::from_utf8_lossy(&out.stdout).trim().to_string();
    parse_github_repo(&remote)
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn git_slug(url: &str) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(url.as_bytes());
    if encoded.len() > 64 {
        encoded[..64].to_string()
    } else {
        encoded
    }
}

/// Convert a `git@github.com:owner/repo(.git)?` SSH URL to its HTTPS equivalent
/// so that a stored HTTPS credential can authenticate it.
/// Other URLs are returned unchanged.
pub fn ssh_to_https(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    url.to_string()
}

async fn git_run(args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().await.map_err(|e| format!("git error: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "git error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

async fn prepare_git(
    url: &str,
    branch: Option<&str>,
    bot_id: &str,
    base_dir: &Path,
) -> Result<PathBuf, String> {
    // One checkout at a time: the clone starts in the background when a bot gets its project, and a task may ask for
    // the same checkout while it runs; the second caller waits and then finds it ready (it used to hit a half-made
    // clone). Clones are rare, so one lock for all of them is enough.
    static CHECKOUT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one_at_a_time = CHECKOUT.lock().await;

    let url = ssh_to_https(url);
    let url = url.as_str();
    let slug = git_slug(url);
    let shared_dir = base_dir.join("projects").join(&slug);
    let worktree = base_dir.join("bots").join(bot_id).join("work");

    // Clone into shared dir if it doesn't exist yet.
    if !shared_dir.exists() {
        std::fs::create_dir_all(shared_dir.parent().unwrap_or(base_dir))
            .map_err(|e| format!("mkdir error: {e}"))?;
        let shared_str = shared_dir.to_string_lossy().into_owned();
        // Without old file contents (fetched on demand if the agent digs into history): a full clone of an 8.8 MB repo
        // took 37 s on atlas-test-ship, this one 14 s.
        let mut args: Vec<&str> = vec!["clone", "--filter=blob:none"];
        let branch_owned;
        if let Some(b) = branch {
            args.push("--branch");
            branch_owned = b.to_string();
            args.push(&branch_owned);
        }
        args.push(url);
        args.push(&shared_str);
        git_run(&args, None).await?;
    }

    // Add the per-bot worktree if the directory doesn't exist.
    if !worktree.exists() {
        std::fs::create_dir_all(worktree.parent().unwrap_or(base_dir))
            .map_err(|e| format!("mkdir error: {e}"))?;
        let wt_str = worktree.to_string_lossy().into_owned();
        // The remote branch to follow: the one asked for, else the remote's default.
        let upstream = match branch {
            Some(b) => format!("origin/{b}"),
            None => {
                git_run(
                    &["rev-parse", "--abbrev-ref", "origin/HEAD"],
                    Some(&shared_dir),
                )
                .await?
            }
        };
        // Each bot gets its own local branch tracking it: git refuses a second worktree on a branch already checked
        // out (by the shared clone or another bot), and tracking lets `fetch` fast-forward it.
        // The full bot id: a prefix of it is not unique, and two bots sharing one collided on the same local branch.
        let local = format!("atlas/{bot_id}/{}", upstream.trim_start_matches("origin/"));
        git_run(
            &[
                "worktree", "add", "--track", "-B", &local, &wt_str, &upstream,
            ],
            Some(&shared_dir),
        )
        .await?;
    }

    Ok(worktree)
}

pub(crate) fn prepare_path(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    match std::fs::metadata(&p) {
        Ok(m) if m.is_dir() => Ok(p),
        Ok(_) => Err(format!("not a directory: {path}")),
        Err(e) => Err(format!("path error: {e}")),
    }
}

#[allow(dead_code)]
fn parse_github_repo(remote: &str) -> Option<String> {
    // https://github.com/owner/repo(.git)?
    if let Some(rest) = remote.strip_prefix("https://github.com/") {
        let repo = rest.trim_end_matches(".git");
        let mut parts = repo.splitn(3, '/');
        let owner = parts.next()?;
        let name = parts.next()?;
        if !owner.is_empty() && !name.is_empty() {
            return Some(format!("{owner}/{name}"));
        }
    }
    // git@github.com:owner/repo(.git)?
    if let Some(rest) = remote.strip_prefix("git@github.com:") {
        let repo = rest.trim_end_matches(".git");
        let mut parts = repo.splitn(3, '/');
        let owner = parts.next()?;
        let name = parts.next()?;
        if !owner.is_empty() && !name.is_empty() {
            return Some(format!("{owner}/{name}"));
        }
    }
    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            ok.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&ok.stderr)
        );
    }

    /// A remote with `main` and `feat/x`, served from disk (file://), like a GitHub repo.
    fn remote_repo(root: &Path) -> String {
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        sh(&src, &["init", "-q", "-b", "main"]);
        sh(
            &src,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "one",
            ],
        );
        sh(&src, &["branch", "feat/x"]);
        format!("file://{}", src.display())
    }

    // Incident: the first task of a bot on codeatlasdev/ac (branch feat/portal-cliente) failed with "fatal:
    // 'feat/portal-cliente' is already used by worktree": the shared clone had that branch checked out, and git refuses
    // a second worktree on it. The same happens to two bots on one branch. Each bot needs its own worktree that follows
    // the remote branch.
    #[tokio::test]
    async fn two_bots_get_worktrees_on_the_same_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let url = remote_repo(tmp.path());
        let base = tmp.path().join("hostd");
        let p = ProjectRef::Git {
            url: url.clone(),
            branch: Some("feat/x".into()),
        };
        let a = prepare(&p, "bot-a", &base).await.expect("first bot");
        let b = prepare(&p, "bot-b", &base)
            .await
            .expect("second bot on the same branch");
        assert!(a.join(".git").exists() && b.join(".git").exists());
        // Both follow origin/feat/x, so `fetch` (pull --ff-only) keeps them current.
        for w in [&a, &b] {
            let up = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
                .current_dir(w)
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&up.stdout).trim(), "origin/feat/x");
        }
        // A second task reuses the worktree instead of failing.
        assert_eq!(prepare(&p, "bot-a", &base).await.unwrap(), a);
    }

    // The clone now starts in the background as soon as a bot gets its project (the first task waited ~90 s on
    // atlas-test-ship for it), so a task can call `prepare` while that clone is still running. Both must end with the
    // same working checkout; before, the second saw a half-made clone and failed.
    #[tokio::test]
    async fn a_task_waits_for_a_clone_already_in_progress() {
        for round in 0..5 {
            let tmp = tempfile::tempdir().unwrap();
            let url = remote_repo(tmp.path());
            let base = tmp.path().join("hostd");
            let p = ProjectRef::Git { url, branch: None };
            let (warm, task) =
                tokio::join!(prepare(&p, "bot-a", &base), prepare(&p, "bot-a", &base));
            let (warm, task) = (
                warm.unwrap_or_else(|e| panic!("round {round}, background clone: {e}")),
                task.unwrap_or_else(|e| panic!("round {round}, task: {e}")),
            );
            assert_eq!(warm, task);
            assert!(task.join(".git").exists());
        }
    }

    #[test]
    fn git_slug_stable() {
        let url = "https://github.com/codeatlasdev/atlas";
        let slug = git_slug(url);
        assert!(!slug.is_empty());
        assert!(slug.len() <= 64);
        // Deterministic
        assert_eq!(slug, git_slug(url));
    }

    #[test]
    fn git_slug_truncates_at_64() {
        let long_url = "https://github.com/".to_string() + &"x".repeat(200);
        assert!(git_slug(&long_url).len() <= 64);
    }

    #[test]
    fn parse_github_repo_https() {
        assert_eq!(
            parse_github_repo("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_repo("https://github.com/owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn parse_github_repo_ssh() {
        assert_eq!(
            parse_github_repo("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn parse_github_repo_other_host() {
        assert_eq!(parse_github_repo("https://gitlab.com/owner/repo"), None);
    }

    #[test]
    fn prepare_path_rejects_file() {
        // A file that exists but is not a directory (temp paths: /etc/hosts is not on Windows).
        let f = tempfile::NamedTempFile::new().unwrap();
        assert!(prepare_path(f.path().to_str().unwrap()).is_err());
    }

    #[test]
    fn prepare_path_accepts_dir() {
        let d = tempfile::tempdir().unwrap();
        assert!(prepare_path(d.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn read_atlas_yaml_extracts_name_and_services() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("atlas.yaml"),
            r#"
name: my-project
services:
  - name: api
    command: cargo run
  - name: web
    command: bun dev
"#,
        )
        .unwrap();
        let info = read_atlas_yaml(dir.path()).unwrap();
        assert_eq!(info.name, "my-project");
        assert_eq!(info.services, vec!["api", "web"]);
    }

    #[test]
    fn read_atlas_yaml_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_atlas_yaml(dir.path()).is_none());
    }

    #[test]
    fn ssh_to_https_converts_github_ssh() {
        assert_eq!(
            ssh_to_https("git@github.com:owner/repo.git"),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            ssh_to_https("git@github.com:owner/repo"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn ssh_to_https_leaves_https_unchanged() {
        let url = "https://github.com/owner/repo.git";
        assert_eq!(ssh_to_https(url), url);
    }

    #[test]
    fn ssh_to_https_leaves_non_github_unchanged() {
        let url = "git@gitlab.com:owner/repo.git";
        assert_eq!(ssh_to_https(url), url);
    }
}
