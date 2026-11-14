//! When may a worktree be removed without losing work (#58 auto clean up), as one rule shared by the
//! desktop (`worktree_auto_remove`) and hostd (`worktree_remove` for the phone). Generic over a [`Run`]
//! so the tests drive it against real temp repos.

use std::future::Future;
use std::pin::Pin;

/// The future of one git invocation. Owned: nothing borrows the caller.
pub type Fut = Pin<Box<dyn Future<Output = Result<Out, String>> + Send>>;
/// Runs `git <args>` in a worktree.
pub type Run<'a> = &'a (dyn Fn(Vec<String>) -> Fut + Send + Sync);

pub struct Out {
    pub code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

async fn git(run: Run<'_>, args: &[&str]) -> Result<Out, String> {
    run(args.iter().map(|s| s.to_string()).collect()).await
}

/// stdout of a git command that must succeed.
async fn ok(run: Run<'_>, args: &[&str]) -> Result<String, String> {
    let o = git(run, args).await?;
    if o.code == Some(0) {
        Ok(o.stdout)
    } else {
        Err(format!("git {}: {}", args.join(" "), o.stderr.trim()))
    }
}

/// Head sha of the branch's PR when `gh` says it is MERGED; `None` for anything else (open, closed, no PR, no gh).
pub async fn merged_head(gh: Run<'_>, branch: &str) -> Option<String> {
    let o = git(gh, &["pr", "view", branch, "--json", "state,headRefOid"])
        .await
        .ok()?;
    if o.code != Some(0) {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&o.stdout).ok()?;
    let sha = v["headRefOid"].as_str()?;
    (v["state"] == "MERGED" && sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| sha.to_string())
}

/// Why auto clean up must keep this worktree (`"dirty"`, `"secrets"`, `"unpushed"`), or `None` when removing it
/// loses nothing. Gitignored `.env*` files count as work: `git worktree remove` deletes ignored files silently.
/// `pushed` is the head sha of a merged PR: commits up to it are on GitHub even if the remote branch is gone.
/// Anything git cannot answer (an unknown tip) counts as unpushed.
pub async fn keep_reason(
    run: Run<'_>,
    pushed: Option<&str>,
) -> Result<Option<&'static str>, String> {
    if pushed.is_some_and(|t| t.len() != 40 || !t.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("the PR head is not a commit sha".into());
    }
    if !ok(run, &["status", "--porcelain", "--untracked-files=all"])
        .await?
        .trim()
        .is_empty()
    {
        return Ok(Some("dirty"));
    }
    let env = [
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "--",
        ":(glob).env*",
        ":(glob)**/.env*",
    ];
    if !ok(run, &env).await?.trim().is_empty() {
        return Ok(Some("secrets"));
    }
    let mut args = vec!["rev-list", "--count", "HEAD", "--not", "--remotes"];
    args.extend(pushed);
    let o = git(run, &args).await?;
    if o.code != Some(0) || o.stdout.trim() != "0" {
        return Ok(Some("unpushed"));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct Repo {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
    }

    fn git_in(dir: &Path, args: &[&str]) -> Out {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        Out {
            code: o.status.code().map(i64::from),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let o = git_in(dir, args);
        assert_eq!(o.code, Some(0), "git {args:?}: {}", o.stderr);
        o.stdout
    }

    impl Repo {
        fn new() -> Repo {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("repo");
            std::fs::create_dir(&dir).unwrap();
            sh(&dir, &["init", "-q", "."]);
            std::fs::write(dir.join("f.txt"), "base\n").unwrap();
            sh(&dir, &["add", "."]);
            sh(&dir, &["commit", "-qm", "base"]);
            Repo { _tmp: tmp, dir }
        }
        fn git(&self, args: &[&str]) -> String {
            sh(&self.dir, args)
        }
    }

    fn runner(dir: PathBuf) -> impl Fn(Vec<String>) -> Fut + Send + Sync {
        move |args| {
            let dir = dir.clone();
            Box::pin(async move {
                let a: Vec<&str> = args.iter().map(String::as_str).collect();
                Ok(git_in(&dir, &a))
            })
        }
    }

    fn block<T>(f: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    // #58 auto clean up: a worktree is only removed when nothing in it would be lost. Dirty (tracked or untracked)
    // and commits on no remote are kept; a merged PR whose head the local branch matches is safe even after the
    // remote branch was deleted (squash merge + fetch --prune), which `--remotes` alone would call unpushed.
    #[test]
    fn auto_clean_keeps_dirty_and_unpushed_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        sh(tmp.path(), &["init", "-q", "--bare", "remote.git"]);
        let r = Repo::new();
        r.git(&["remote", "add", "origin", &remote.to_string_lossy()]);
        r.git(&["push", "-q", "origin", "HEAD:refs/heads/main"]);
        r.git(&["fetch", "-q", "origin"]);
        let run = runner(r.dir.clone());
        assert_eq!(block(keep_reason(&run, None)), Ok(None), "clean and pushed");

        // Gitignored secrets are not in `status` and `git worktree remove` deletes them: a `.env` anywhere keeps it.
        std::fs::write(r.dir.join(".gitignore"), ".env*\nbuild/\n").unwrap();
        r.git(&["add", ".gitignore"]);
        r.git(&["commit", "-qm", "ignore"]);
        r.git(&["push", "-q", "origin", "HEAD:refs/heads/main"]);
        r.git(&["fetch", "-q", "origin"]);
        std::fs::create_dir_all(r.dir.join("build/app")).unwrap();
        std::fs::write(r.dir.join("build/app/out.js"), "x").unwrap();
        assert_eq!(
            block(keep_reason(&run, None)),
            Ok(None),
            "build output is disposable"
        );
        std::fs::create_dir_all(r.dir.join("apps/api")).unwrap();
        std::fs::write(r.dir.join("apps/api/.env.local"), "SECRET=1").unwrap();
        assert_eq!(
            block(keep_reason(&run, None)),
            Ok(Some("secrets")),
            "nested .env.local"
        );
        std::fs::remove_dir_all(r.dir.join("apps")).unwrap();

        std::fs::write(r.dir.join("new.txt"), "x").unwrap();
        assert_eq!(
            block(keep_reason(&run, None)),
            Ok(Some("dirty")),
            "untracked file"
        );
        std::fs::remove_file(r.dir.join("new.txt")).unwrap();
        std::fs::write(r.dir.join("f.txt"), "edit\n").unwrap();
        assert_eq!(
            block(keep_reason(&run, None)),
            Ok(Some("dirty")),
            "tracked edit"
        );

        r.git(&["commit", "-qam", "local only"]);
        assert_eq!(block(keep_reason(&run, None)), Ok(Some("unpushed")));
        let tip = r.git(&["rev-parse", "HEAD"]).trim().to_string();
        assert_eq!(
            block(keep_reason(&run, Some(&tip))),
            Ok(None),
            "HEAD is the merged PR head"
        );

        r.git(&["commit", "-q", "--allow-empty", "-m", "after the PR"]);
        assert_eq!(
            block(keep_reason(&run, Some(&tip))),
            Ok(Some("unpushed")),
            "work after the merged head"
        );
        assert!(
            block(keep_reason(&run, Some("--all"))).is_err(),
            "a tip is a sha, never an option"
        );
        assert_eq!(
            block(keep_reason(&run, Some(&"0".repeat(40)))),
            Ok(Some("unpushed")),
            "unknown tip"
        );
    }

    // #58: Rust reads the merged PR itself; the frontend only says "merged mode". Only a MERGED PR with a sha head is
    // trusted, so an open PR (or a caller passing HEAD's sha for an unmerged branch) can never mark commits pushed.
    #[test]
    fn merged_head_trusts_only_a_merged_pr_from_gh() {
        let sha = "a".repeat(40);
        let gh = |code: i64, out: String| {
            move |args: Vec<String>| -> Fut {
                // `merged_head` must ask for the branch's PR, nothing else (the runner prefixes `gh`).
                assert_eq!(args[..3], ["pr", "view", "feat"], "{args:?}");
                let out = out.clone();
                Box::pin(async move {
                    Ok(Out {
                        code: Some(code),
                        stdout: out,
                        stderr: String::new(),
                    })
                })
            }
        };
        let merged = gh(0, format!(r#"{{"state":"MERGED","headRefOid":"{sha}"}}"#));
        assert_eq!(block(merged_head(&merged, "feat")), Some(sha.clone()));
        let open = gh(0, format!(r#"{{"state":"OPEN","headRefOid":"{sha}"}}"#));
        assert_eq!(block(merged_head(&open, "feat")), None);
        let bad = gh(0, r#"{"state":"MERGED","headRefOid":"--all"}"#.into());
        assert_eq!(block(merged_head(&bad, "feat")), None);
        let none = gh(1, "no pull requests found".into());
        assert_eq!(block(merged_head(&none, "feat")), None);
    }
}
