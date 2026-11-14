//! The changed files of a worktree: what `atlas_affected` assumes when the agent passes none.

use std::path::Path;
use std::process::Command;

/// Refs tried, in order, for the base when none is given.
const DEFAULT_BASES: [&str; 3] = ["origin/main", "main", "master"];

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn ref_exists(root: &Path, r: &str) -> bool {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{r}^{{commit}}"),
        ],
    )
    .is_ok()
}

/// Files changed in the worktree at `root` (a folder inside a git work tree), relative to `root`,
/// sorted: committed since the merge base with `base` (default `origin/main`, else `main`, else
/// `master`), plus staged, unstaged and untracked files; files git ignores are not changes.
/// Files of the repo outside `root` are left out.
pub fn changed_files(root: &Path, base: Option<&str>) -> Result<Vec<String>, String> {
    if git(root, &["rev-parse", "--is-inside-work-tree"]).is_err() {
        return Err(format!(
            "{} is not inside a git repository: pass the changed `files` explicitly",
            root.display()
        ));
    }
    let base = match base {
        Some(b) => {
            if !ref_exists(root, b) {
                return Err(format!("base ref '{b}' does not exist in this repository"));
            }
            b
        }
        None => DEFAULT_BASES
            .into_iter()
            .find(|b| ref_exists(root, b))
            .ok_or("no origin/main, main or master to diff against: pass `base` or `files`")?,
    };
    let mb = git(root, &["merge-base", base, "HEAD"])
        .map_err(|e| format!("no merge base between '{base}' and HEAD: {e}"))?;
    let mb = mb.trim();
    let tracked = git(root, &["diff", "--name-only", "--relative", "-z", mb])?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut files: Vec<String> = tracked
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}
