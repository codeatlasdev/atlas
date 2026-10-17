//! Branch picker of a worktree: list local + remote branches (most recently checked out first) and
//! switch, create or track-checkout. Everything runs through hostd `exec` (same path for SSH hosts);
//! the logic is generic over a [`Git`] runner so the tests drive it against real temp repos.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::host::HostHub;
use crate::registry;

/// The future of one git invocation. Owned: nothing borrows the caller.
pub type Fut = Pin<Box<dyn Future<Output = Result<Out, String>> + Send>>;
/// Runs `git <args>` in a worktree.
pub type Run<'a> = &'a (dyn Fn(Vec<String>) -> Fut + Send + Sync);

pub struct Out {
    pub code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

/// A branch row of the picker. Serialized as-is to the frontend (`BranchInfo` in `api.ts`).
#[derive(Serialize, Debug, PartialEq)]
pub struct Branch {
    /// `feat/x` or `origin/feat/x`.
    pub name: String,
    pub kind: &'static str,
    pub current: bool,
    /// Committer date of the tip, ISO 8601 (the UI renders it relative).
    pub date: String,
    /// Another worktree of the repo has this branch checked out.
    pub worktree: Option<Occupied>,
    /// Remote branch that already has a local branch of the same name (selecting it switches to that one).
    pub local: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Occupied {
    pub path: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Branches {
    /// `None` on a detached HEAD.
    pub current: Option<String>,
    pub branches: Vec<Branch>,
}

#[derive(Deserialize, Debug, Default)]
pub struct SwitchReq {
    pub branch: String,
    /// `git switch -c <branch>` from the current HEAD.
    #[serde(default)]
    pub create: bool,
    /// Stash (incl. untracked) before switching and pop on the target.
    #[serde(default)]
    pub stash: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Switched {
    Ok {
        branch: String,
        stashed: bool,
    },
    /// Local changes would be overwritten; git's own message.
    Dirty {
        message: String,
    },
    /// Checked out by another worktree at `path`.
    Worktree {
        path: String,
    },
}

/// `<sep>`-joined `for-each-ref` fields (a tab never appears in a ref name, date or path).
const FIELDS: &str = "%(refname)%09%(committerdate:iso-strict)%09%(HEAD)%09%(worktreepath)";

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

/// Branch names in the order they were last checked out, newest first: the `HEAD` reflog line
/// `checkout: moving from A to B` means B was entered after A was left.
fn checkout_order(reflog: &str) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for to in reflog
        .lines()
        .filter_map(|l| l.strip_prefix("checkout: moving from "))
        .filter_map(|l| l.rsplit_once(" to ").map(|(_, to)| to))
    {
        if !seen.contains(&to) {
            seen.push(to);
        }
    }
    seen
}

pub async fn list(run: Run<'_>) -> Result<Branches, String> {
    let refs = ok(
        run,
        &[
            "for-each-ref",
            // The LAST --sort is the primary key: newest first, ties (same second) by name so the
            // order is the same on every refresh.
            "--sort=refname",
            "--sort=-committerdate",
            &format!("--format={FIELDS}"),
            "refs/heads",
            "refs/remotes",
        ],
    )
    .await?;
    // No reflog yet (fresh clone, expired) is not an error: order falls back to commit date.
    let reflog = ok(run, &["reflog", "show", "--format=%gs", "HEAD"])
        .await
        .unwrap_or_default();
    let order = checkout_order(&reflog);
    // The worktree this runs in: its own branch is current, not "occupied".
    // Both this and `%(worktreepath)` are git's own absolute paths, so they compare as strings (no local
    // filesystem access: the checkout can be on an SSH host).
    let top = ok(run, &["rev-parse", "--show-toplevel"]).await?;
    let top = top.trim();

    let mut locals: Vec<Branch> = Vec::new();
    let mut remotes: Vec<Branch> = Vec::new();
    for line in refs.lines() {
        let mut f = line.split('\t');
        let (Some(full), Some(date), Some(head), Some(wt)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let current = head == "*";
        let worktree = (!wt.is_empty() && !current && wt != top).then(|| Occupied {
            path: wt.to_string(),
        });
        if let Some(name) = full.strip_prefix("refs/heads/") {
            locals.push(Branch {
                name: name.into(),
                kind: "local",
                current,
                date: date.into(),
                worktree,
                local: None,
            });
        } else if let Some(name) = full.strip_prefix("refs/remotes/") {
            // `origin/HEAD` is a pointer and a bare `origin` is the remote itself, not a branch.
            if name.ends_with("/HEAD") || !name.contains('/') {
                continue;
            }
            remotes.push(Branch {
                name: name.into(),
                kind: "remote",
                current: false,
                date: date.into(),
                worktree: None,
                local: None,
            });
        }
    }
    let rank = |n: &str| order.iter().position(|o| *o == n).unwrap_or(usize::MAX);
    // Stable sort keeps the date order of `for-each-ref` among equal ranks.
    locals.sort_by_key(|b| (!b.current, rank(&b.name)));
    let names: Vec<String> = locals.iter().map(|b| b.name.clone()).collect();
    for r in &mut remotes {
        let short = r.name.split_once('/').map(|(_, s)| s).unwrap_or(&r.name);
        r.local = names.iter().find(|n| n.as_str() == short).cloned();
    }
    let current = locals.iter().find(|b| b.current).map(|b| b.name.clone());
    locals.extend(remotes);
    Ok(Branches {
        current,
        branches: locals,
    })
}

/// What a failed `git switch` means, from its message (git has no machine-readable error).
fn classify(stderr: &str) -> Option<Switched> {
    if let Some(rest) = stderr
        .split("is already used by worktree at '")
        .nth(1)
        .or_else(|| stderr.split("is already checked out at '").nth(1))
    {
        return Some(Switched::Worktree {
            path: rest.split('\'').next().unwrap_or(rest).to_string(),
        });
    }
    if stderr.contains("would be overwritten")
        || stderr.contains("Please commit your changes or stash them")
    {
        return Some(Switched::Dirty {
            message: stderr.trim().to_string(),
        });
    }
    None
}

pub async fn switch(run: Run<'_>, req: SwitchReq) -> Result<Switched, String> {
    let name = req.branch.trim();
    // A name starting with `-` would be read as an option by git.
    if name.is_empty() || name.starts_with('-') {
        return Err(format!("invalid branch name {name:?}"));
    }
    if req.create {
        ok(run, &["check-ref-format", "--branch", name])
            .await
            .map_err(|_| format!("invalid branch name {name:?}"))?;
    }
    // `origin/x` with no local `origin/x` branch is a remote ref: check out the local `x` it tracks.
    let is_remote = !req.create
        && git(
            run,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
        )
        .await?
        .code
            != Some(0)
        && git(
            run,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/{name}"),
            ],
        )
        .await?
        .code
            == Some(0);
    let target = if is_remote {
        name.split_once('/').map(|(_, s)| s).unwrap_or(name)
    } else {
        name
    };
    let local_twin = is_remote
        && git(
            run,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{target}"),
            ],
        )
        .await?
        .code
            == Some(0);

    let stashed = if req.stash {
        let o = ok(
            run,
            &[
                "stash",
                "push",
                "--include-untracked",
                "-m",
                &format!("atlas: before switching to {target}"),
            ],
        )
        .await?;
        !o.contains("No local changes")
    } else {
        false
    };
    let mut args = vec!["switch"];
    if req.create {
        args.push("-c");
    } else if is_remote && !local_twin {
        args.extend(["--track", name]);
    }
    if !(is_remote && !local_twin) {
        args.push(target);
    }
    let o = git(run, &args).await?;
    if o.code == Some(0) {
        return Ok(Switched::Ok {
            branch: target.to_string(),
            stashed,
        });
    }
    if stashed {
        // The switch failed after we stashed: put the work back, never leave it hidden in the stash.
        let _ = ok(run, &["stash", "pop", "--index"]).await;
    }
    match classify(&o.stderr) {
        Some(s) => Ok(s),
        None => Err(format!("git {}: {}", args.join(" "), o.stderr.trim())),
    }
}

/// The runner for a registered worktree: `git -C <root> …` through hostd `exec`.
fn hostd_runner(hub: Arc<HostHub>, dir: PathBuf) -> impl Fn(Vec<String>) -> Fut + Send + Sync {
    move |args| {
        let (hub, dir) = (hub.clone(), dir.clone());
        Box::pin(async move {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            let o = crate::worktree::exec(
                &hub,
                &dir,
                &[&["git", "-C", &dir.to_string_lossy()][..], &a].concat(),
                60_000,
            )
            .await?;
            Ok(Out {
                code: o.code,
                stdout: o.stdout,
                stderr: o.stderr,
            })
        })
    }
}

fn entry(id: &str) -> Result<registry::Entry, String> {
    registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown worktree {id}"))
}

/// Local + remote branches of the worktree, most recently checked out first.
#[tauri::command]
pub async fn git_branches(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Branches, String> {
    let run = hostd_runner(hub.inner().clone(), PathBuf::from(entry(&id)?.root));
    list(&run).await
}

/// `git switch` in the worktree (create / track / stash variants). A successful switch updates the
/// registry's branch and emits `projects://changed` so the sidebar, breadcrumb and palettes follow.
#[tauri::command]
pub async fn git_switch(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    id: String,
    req: SwitchReq,
) -> Result<Switched, String> {
    let e = entry(&id)?;
    let run = hostd_runner(hub.inner().clone(), PathBuf::from(&e.root));
    let out = switch(&run, req).await?;
    if let Switched::Ok { branch, .. } = &out {
        let mut all = registry::load()?;
        if let Some(x) = all.iter_mut().find(|x| x.id == id) {
            x.branch = Some(branch.clone());
        }
        registry::save(&all)?;
        let _ = app.emit("projects://changed", ());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::Path;
    use std::process::Command;

    /// A real temp repo; `run` executes real git in it (or in a sibling worktree dir).
    struct Repo {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
    }

    /// Every git call of a test gets the next minute of THAT test's own fake clock, keyed by the temp dir shared by
    /// its repo, linked worktrees and bare remote: commit dates never tie, never depend on the wall clock, and do
    /// not depend on which other tests ran first (ties made the remote order, and the committed fixture, vary).
    fn tick(dir: &Path) -> String {
        static CLOCKS: std::sync::Mutex<Vec<(PathBuf, u64)>> = std::sync::Mutex::new(Vec::new());
        let key = dir.parent().unwrap_or(dir).to_path_buf();
        let mut clocks = CLOCKS.lock().unwrap();
        let i = clocks
            .iter()
            .position(|(k, _)| *k == key)
            .unwrap_or_else(|| {
                clocks.push((key, 1_700_000_000));
                clocks.len() - 1
            });
        clocks[i].1 += 60;
        format!("{} +0000", clocks[i].1)
    }

    fn git_in(dir: &Path, args: &[&str]) -> Out {
        let date = tick(dir);
        let o = Command::new("git")
            .env("GIT_COMMITTER_DATE", &date)
            .env("GIT_AUTHOR_DATE", &date)
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
        /// Commit `content` to `f.txt` on a new branch, then come back to where we were.
        fn branch_with(&self, name: &str, content: &str) {
            let was = self
                .git(&["rev-parse", "--abbrev-ref", "HEAD"])
                .trim()
                .to_string();
            self.git(&["switch", "-qc", name]);
            std::fs::write(self.dir.join("f.txt"), content).unwrap();
            self.git(&["commit", "-qam", name]);
            self.git(&["switch", "-q", &was]);
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

    fn names(b: &Branches) -> Vec<String> {
        b.branches.iter().map(|b| b.name.clone()).collect()
    }

    // Branch names may themselves contain " to " (`fix/go to prod` is legal): the target is what follows the LAST one.
    #[test]
    fn checkout_order_uses_the_last_to_and_dedups_newest_first() {
        // `git reflog` prints newest first.
        let reflog = "checkout: moving from main to feat/a\n\
                      checkout: moving from fix/go-to-prod to main\n\
                      commit: x\n\
                      checkout: moving from feat/a to fix/go-to-prod\n\
                      checkout: moving from main to feat/a\n";
        assert_eq!(checkout_order(reflog), ["feat/a", "main", "fix/go-to-prod"]);
    }

    // git has no machine-readable switch error: both refusals are told apart by their stable message text.
    #[test]
    fn classify_tells_dirty_and_worktree_refusals_apart() {
        let wt = "fatal: 'old' is already used by worktree at '/tmp/wt other'";
        assert_eq!(
            classify(wt),
            Some(Switched::Worktree {
                path: "/tmp/wt other".into()
            })
        );
        let wt2 = "fatal: 'old' is already checked out at '/tmp/wt'";
        assert_eq!(
            classify(wt2),
            Some(Switched::Worktree {
                path: "/tmp/wt".into()
            })
        );
        let dirty = "error: Your local changes to the following files would be overwritten by checkout:\n\tf\nAborting";
        assert!(matches!(classify(dirty), Some(Switched::Dirty { .. })));
        assert_eq!(classify("fatal: invalid reference: nope"), None);
    }

    // The picker lists the branch you just left first: most recently CHECKED OUT, not most recently committed.
    #[test]
    fn lists_local_branches_by_checkout_recency_then_remotes() {
        let r = Repo::new();
        r.branch_with("alpha", "a\n");
        r.branch_with("beta", "b\n");
        r.branch_with("gamma", "c\n");
        // Checkout order: beta, alpha, main (current) — gamma never checked out after creation.
        r.git(&["switch", "-q", "beta"]);
        r.git(&["switch", "-q", "alpha"]);
        r.git(&["switch", "-q", "main"]);
        // A remote with one branch that has no local counterpart and one that does.
        let remote = r.dir.parent().unwrap().join("remote.git");
        sh(
            &r.dir,
            &["clone", "-q", "--bare", ".", remote.to_str().unwrap()],
        );
        r.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        r.git(&["fetch", "-q", "origin"]);
        r.git(&["push", "-q", "origin", "main:only-remote"]);
        r.git(&["fetch", "-q", "origin"]);
        let got = block(list(&runner(r.dir.clone()))).unwrap();
        assert_eq!(got.current.as_deref(), Some("main"));
        let n = names(&got);
        let pos = |x: &str| {
            n.iter()
                .position(|b| b == x)
                .unwrap_or_else(|| panic!("{x} missing in {n:?}"))
        };
        assert_eq!(n[0], "main", "current branch first: {n:?}");
        assert!(
            pos("alpha") < pos("beta"),
            "alpha was checked out after beta: {n:?}"
        );
        assert!(
            pos("beta") < pos("gamma"),
            "never re-checked-out branch after checked-out ones: {n:?}"
        );
        assert!(
            pos("gamma") < pos("origin/only-remote"),
            "locals before remotes: {n:?}"
        );
        assert!(
            !n.contains(&"origin/HEAD".to_string()) && !n.contains(&"origin".to_string()),
            "{n:?}"
        );
        let cur: Vec<_> = got
            .branches
            .iter()
            .filter(|b| b.current)
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(cur, ["main"]);
        // `origin/main` has a local twin; the row says so (select switches to it, no duplicate).
        let om = got
            .branches
            .iter()
            .find(|b| b.name == "origin/main")
            .unwrap();
        assert_eq!(om.local.as_deref(), Some("main"));
        assert_eq!(got.branches[pos("origin/only-remote")].local, None);
    }

    // A branch checked out by ANOTHER worktree is reported with that worktree's path; the current one is not "occupied".
    #[test]
    fn reports_a_branch_checked_out_in_another_worktree() {
        let r = Repo::new();
        r.branch_with("busy", "b\n");
        let wt = r.dir.parent().unwrap().join("wt-busy");
        r.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "busy"]);
        let got = block(list(&runner(r.dir.clone()))).unwrap();
        let busy = got.branches.iter().find(|b| b.name == "busy").unwrap();
        let want = std::fs::canonicalize(&wt).unwrap();
        assert_eq!(
            busy.worktree
                .as_ref()
                .map(|o| std::fs::canonicalize(&o.path).unwrap()),
            Some(want)
        );
        let main = got.branches.iter().find(|b| b.name == "main").unwrap();
        assert_eq!(
            main.worktree, None,
            "the current worktree's own branch is not 'in another worktree'"
        );
        // From the linked worktree's point of view, `main` (checked out in the main checkout) is occupied.
        let from_wt = block(list(&runner(wt.clone()))).unwrap();
        assert_eq!(from_wt.current.as_deref(), Some("busy"));
        assert!(
            from_wt
                .branches
                .iter()
                .find(|b| b.name == "main")
                .unwrap()
                .worktree
                .is_some()
        );
    }

    #[test]
    fn detached_head_has_no_current_branch() {
        let r = Repo::new();
        r.git(&["switch", "-q", "--detach", "HEAD"]);
        let got = block(list(&runner(r.dir.clone()))).unwrap();
        assert_eq!(got.current, None);
        assert!(got.branches.iter().all(|b| !b.current));
        assert_eq!(names(&got), ["main"]);
    }

    #[test]
    fn switches_to_an_existing_branch() {
        let r = Repo::new();
        r.branch_with("feat", "f\n");
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "feat".into(),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            got,
            Switched::Ok {
                branch: "feat".into(),
                stashed: false
            }
        );
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "feat");
    }

    #[test]
    fn creates_a_branch_from_the_current_head_and_checks_it_out() {
        let r = Repo::new();
        let head = r.git(&["rev-parse", "HEAD"]);
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "feat/new".into(),
                create: true,
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            got,
            Switched::Ok {
                branch: "feat/new".into(),
                stashed: false
            }
        );
        assert_eq!(
            r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "feat/new"
        );
        assert_eq!(r.git(&["rev-parse", "HEAD"]), head);
    }

    // Creating a branch that exists is an error (never a silent reset), and a leading `-` is never an option.
    #[test]
    fn create_refuses_existing_names_and_option_lookalikes() {
        let r = Repo::new();
        r.branch_with("taken", "t\n");
        let e = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "taken".into(),
                create: true,
                ..Default::default()
            },
        ));
        assert!(e.is_err(), "{e:?}");
        let e = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "--orphan".into(),
                create: true,
                ..Default::default()
            },
        ));
        assert!(e.is_err(), "{e:?}");
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
    }

    #[test]
    fn remote_branch_creates_a_tracking_local_branch() {
        let r = Repo::new();
        r.branch_with("shared", "s\n");
        let remote = r.dir.parent().unwrap().join("remote.git");
        sh(
            &r.dir,
            &["clone", "-q", "--bare", ".", remote.to_str().unwrap()],
        );
        r.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        r.git(&["fetch", "-q", "origin"]);
        r.git(&["branch", "-D", "shared"]);
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "origin/shared".into(),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            got,
            Switched::Ok {
                branch: "shared".into(),
                stashed: false
            }
        );
        assert_eq!(
            r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "shared"
        );
        assert_eq!(
            r.git(&["rev-parse", "--abbrev-ref", "shared@{upstream}"])
                .trim(),
            "origin/shared"
        );
    }

    // A remote branch whose local twin already exists switches to the twin instead of failing with "already exists".
    #[test]
    fn remote_branch_with_a_local_twin_switches_to_the_twin() {
        let r = Repo::new();
        r.branch_with("twin", "t\n");
        let remote = r.dir.parent().unwrap().join("remote.git");
        sh(
            &r.dir,
            &["clone", "-q", "--bare", ".", remote.to_str().unwrap()],
        );
        r.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        r.git(&["fetch", "-q", "origin"]);
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "origin/twin".into(),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            got,
            Switched::Ok {
                branch: "twin".into(),
                stashed: false
            }
        );
    }

    #[test]
    fn branch_in_another_worktree_is_reported_not_failed() {
        let r = Repo::new();
        r.branch_with("busy", "b\n");
        let wt = r.dir.parent().unwrap().join("wt-busy");
        r.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "busy"]);
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "busy".into(),
                ..Default::default()
            },
        ))
        .unwrap();
        match got {
            Switched::Worktree { path } => assert_eq!(
                std::fs::canonicalize(path).unwrap(),
                std::fs::canonicalize(&wt).unwrap()
            ),
            other => panic!("expected Worktree, got {other:?}"),
        }
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
    }

    // Incident class: a dirty switch must refuse with git's message and touch nothing.
    #[test]
    fn dirty_tree_that_would_be_overwritten_is_refused_untouched() {
        let r = Repo::new();
        r.branch_with("feat", "from feat\n");
        std::fs::write(r.dir.join("f.txt"), "my local edit\n").unwrap();
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "feat".into(),
                ..Default::default()
            },
        ))
        .unwrap();
        match got {
            Switched::Dirty { message } => assert!(message.contains("f.txt"), "{message}"),
            other => panic!("expected Dirty, got {other:?}"),
        }
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
        assert_eq!(
            std::fs::read_to_string(r.dir.join("f.txt")).unwrap(),
            "my local edit\n"
        );
        assert!(r.git(&["stash", "list"]).is_empty());
    }

    // "Stash and switch": changes (untracked too) are stashed, the switch happens and nothing is lost.
    #[test]
    fn stash_and_switch_keeps_every_change_recoverable() {
        let r = Repo::new();
        r.branch_with("feat", "from feat\n");
        std::fs::write(r.dir.join("f.txt"), "my local edit\n").unwrap();
        std::fs::write(r.dir.join("new.txt"), "untracked\n").unwrap();
        let got = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "feat".into(),
                stash: true,
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            got,
            Switched::Ok {
                branch: "feat".into(),
                stashed: true
            }
        );
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "feat");
        let stash = r.git(&["stash", "list"]);
        assert_eq!(stash.lines().count(), 1, "{stash}");
        assert!(stash.contains("atlas"), "{stash}");
        let files = r.git(&["stash", "show", "--include-untracked", "--name-only"]);
        assert!(
            files.contains("f.txt") && files.contains("new.txt"),
            "tracked and untracked changes both stashed: {files}"
        );
    }

    // Switching to a branch that does not exist fails with git's message, never creates it.
    #[test]
    fn unknown_branch_is_an_error() {
        let r = Repo::new();
        let e = block(switch(
            &runner(r.dir.clone()),
            SwitchReq {
                branch: "nope".into(),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(e.contains("nope"), "{e}");
        assert_eq!(r.git(&["branch", "--list", "nope"]), "");
    }

    /// Contract with the frontend: `__fixtures__/git-branches.json` / `git-switch.json` are the REAL serialization of
    /// what `list`/`switch` return on a real repo, which `branches.test.ts` parses with the app's own code.
    /// `UPDATE_FIXTURES=1` regenerates them.
    #[test]
    fn branch_payloads_match_the_frontend_fixtures() {
        let r = Repo::new();
        r.branch_with("busy", "b\n");
        r.branch_with("feat", "f\n");
        let wt = r.dir.parent().unwrap().join("wt-busy");
        r.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "busy"]);
        let remote = r.dir.parent().unwrap().join("remote.git");
        sh(
            &r.dir,
            &["clone", "-q", "--bare", ".", remote.to_str().unwrap()],
        );
        r.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        r.git(&["fetch", "-q", "origin"]);
        let list_v = serde_json::to_value(block(list(&runner(r.dir.clone()))).unwrap()).unwrap();
        // Make the machine-specific bits deterministic: the temp path and the commit dates.
        let mut list_v = list_v;
        for b in list_v["branches"].as_array_mut().unwrap() {
            b["date"] = Value::from("2026-01-01T00:00:00+00:00");
            if !b["worktree"].is_null() {
                b["worktree"]["path"] = Value::from("/work/repo-busy");
            }
        }
        let switched = [
            serde_json::to_value(Switched::Ok {
                branch: "feat".into(),
                stashed: false,
            })
            .unwrap(),
            serde_json::to_value(Switched::Dirty {
                message: "error: Your local changes".into(),
            })
            .unwrap(),
            serde_json::to_value(Switched::Worktree {
                path: "/work/repo-busy".into(),
            })
            .unwrap(),
        ];
        write_fixture("git-branches.json", &list_v);
        write_fixture("git-switch.json", &Value::Array(switched.to_vec()));
    }

    fn write_fixture(name: &str, value: &Value) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__")
            .join(name);
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{name} missing: run with UPDATE_FIXTURES=1"));
        let committed: Value = serde_json::from_str(&committed).unwrap();
        assert_eq!(
            committed, *value,
            "stale {name}: re-run with UPDATE_FIXTURES=1"
        );
    }
}
