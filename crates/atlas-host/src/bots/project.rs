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
    let slug = git_slug(url);
    let shared_dir = base_dir.join("projects").join(&slug);
    let worktree = base_dir.join("bots").join(bot_id).join("work");

    // Clone into shared dir if it doesn't exist yet.
    if !shared_dir.exists() {
        std::fs::create_dir_all(shared_dir.parent().unwrap_or(base_dir))
            .map_err(|e| format!("mkdir error: {e}"))?;
        let shared_str = shared_dir.to_string_lossy().into_owned();
        let mut args: Vec<&str> = vec!["clone"];
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
        let branch_ref = branch.unwrap_or("HEAD");
        git_run(&["worktree", "add", &wt_str, branch_ref], Some(&shared_dir)).await?;
    }

    Ok(worktree)
}

fn prepare_path(path: &str) -> Result<PathBuf, String> {
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
}
