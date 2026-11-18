//! Crewmate project checkout and atlas.yaml discovery.
//!
//! Git projects use a shared clone + per-crewmate worktree pattern:
//!   `<base>/projects/<slug>/`  — shared bare-ish clone (fetchable)
//!   `<base>/crew/<id>/work/`   — git worktree for this crewmate
//!
//! Path projects are validated and returned as-is.

use std::path::{Path, PathBuf};

use crate::crew::gitrun::Git;
use crate::crew::mirror::Mirror;
use crate::crew::spawn::layout::Isolated;
use crate::crew::store::{ProjectRef, crewmate_dir_in};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

// ─── prepare ──────────────────────────────────────────────────────────────────

/// `<base>/crew/<id>/work/`: the crewmate's git worktree (or the folder of a crewmate without a project).
pub fn work_dir(base_dir: &Path, crewmate_id: &str) -> PathBuf {
    crewmate_dir_in(base_dir, crewmate_id).join("work")
}

/// The crewmate's working directory: `/var/lib/atlas-cm/<slug>/work` when it runs as its own user
/// (the agent cannot enter the crew home), else [`work_dir`].
pub fn work_dir_for(base_dir: &Path, crewmate_id: &str, iso: Option<&Isolated>) -> PathBuf {
    match iso {
        Some(iso) => iso.work(),
        None => work_dir(base_dir, crewmate_id),
    }
}

/// Where this crewmate's agent works, without preparing anything: the `work/` checkout of a Git
/// project, the folder of a Path project, else its own `workdir` or `work/`. The same answer
/// `runner::prompt::prepare_work_dir` ends up with, for code that only needs to read the tree.
/// A crewmate that runs as its own Linux user works in its own folder (`iso`), like the agent does.
pub fn current_work_dir(
    base_dir: &Path,
    crewmate: &crate::crew::store::Crewmate,
    iso: Option<&Isolated>,
) -> Result<PathBuf, String> {
    let dir = match &crewmate.project {
        Some(ProjectRef::Git { .. }) => work_dir_for(base_dir, &crewmate.id, iso),
        Some(ProjectRef::Path { path }) => prepare_path(path)?,
        None => crewmate
            .workdir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| work_dir_for(base_dir, &crewmate.id, iso)),
    };
    // tree-access: `work/` is an entry of the root-owned slug dir (nobody can replace it) or an operator-set path; stat only
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(format!(
            "no work tree yet at {}: the crewmate has not run a task here",
            dir.display()
        ))
    }
}

/// Ensure the project is checked out and return the working directory for the crewmate.
///
/// For `Git`: shared clone + per-crewmate worktree, or, for a crewmate that runs as its own user, its
/// own full clone in its directory (a worktree's `.git` points into the shared clone, which lives in
/// the crew home the agent cannot read).
/// For `Path`: validate the path is an existing directory and return it.
pub async fn prepare(
    project: &ProjectRef,
    crewmate_id: &str,
    base_dir: &Path,
    iso: Option<Isolated>,
    git: &Git,
) -> Result<PathBuf, String> {
    match project {
        ProjectRef::Git { url, branch } => match iso {
            Some(iso) => prepare_isolated(url, branch.as_deref(), &iso, git).await,
            None => prepare_git(url, branch.as_deref(), crewmate_id, base_dir).await,
        },
        ProjectRef::Path { path } => prepare_path(path),
    }
}

// ─── fetch ────────────────────────────────────────────────────────────────────

/// Fetch latest changes (git only, non-destructive). Call before each turn.
///
/// For a worktree of the shared clone: fetches in the shared repo, then fast-forward pulls in the worktree only
/// when there are no local modifications. For a crewmate that runs as its own user: its checkout is a full clone, so
/// it fetches and pulls **in its own `work_dir`** (never in a directory its own git output names: it is the
/// crewmate's to change), after hostd brought `mirror` up to date when the crewmate has no GitHub token of its own.
///
/// `git` runs every command: the crewmate itself for an isolated checkout (its `.git/config` is the
/// crewmate's), hostd otherwise.
pub async fn fetch(work_dir: &Path, git: &Git, mirror: Option<&Mirror>) -> Result<(), String> {
    let upload_pack = mirror.map(Mirror::upload_pack);
    if git.is_crewmate() {
        if let Some(m) = mirror
            && let Err(e) = m.refresh().await
        {
            // i18n-ignore: log
            eprintln!(
                "atlas crew: mirror of {} not refreshed: {e}",
                work_dir.display()
            );
        }
        // A clone made before the mirror (or under the other mode) is moved to the current one: its `origin` reads
        // the mirror and pushes to the project.
        if let Some(m) = mirror {
            let _ = git_out(
                git,
                Some(work_dir),
                &["remote", "set-url", "origin", &m.local_url()],
                false,
            )
            .await;
            let _ = git_out(
                git,
                Some(work_dir),
                &["remote", "set-url", "--push", "origin", m.url()],
                false,
            )
            .await;
        }
        let _ = git_out(
            git,
            Some(work_dir),
            &fetch_args("fetch", &upload_pack),
            true,
        )
        .await;
    } else {
        // Find the shared (main) worktree via `git worktree list --porcelain`.
        let list = git_out(
            git,
            Some(work_dir),
            &["worktree", "list", "--porcelain"],
            false,
        )
        .await
        .unwrap_or_default();
        let shared = list
            .lines()
            .find_map(|l| l.strip_prefix("worktree "))
            .map(|s| PathBuf::from(s.trim()));
        if let Some(shared_dir) = shared {
            let _ = git_out(git, Some(&shared_dir), &["fetch", "--quiet"], true).await;
        }
    }

    // Pull in the worktree only when the working tree is clean.
    let status = git_out(git, Some(work_dir), &["status", "--porcelain"], false)
        .await
        .unwrap_or_default();
    if status.is_empty() {
        let mut args = fetch_args("pull", &upload_pack);
        args.insert(1, "--ff-only");
        let _ = git_out(git, Some(work_dir), &args, true).await;
    }

    Ok(())
}

/// `<sub> --quiet [--upload-pack=<cmd>]`: the crewmate's reads of hostd's mirror name the command that may serve it.
fn fetch_args<'a>(sub: &'a str, upload_pack: &'a Option<String>) -> Vec<&'a str> {
    let mut args = vec![sub, "--quiet"];
    if let Some(u) = upload_pack {
        args.push("--upload-pack");
        args.push(u);
    }
    args
}

/// Take a deleted crewmate's checkout out of the shared clone: `git worktree remove --force` (the
/// agent's uncommitted work goes with the crewmate), a prune of what is left, and its local branches
/// `atlas/<id>/*`. Without it the shared clone keeps the worktree's metadata and the branch forever,
/// and a later `worktree add` for the same branch can refuse. A crewmate with no Git checkout has
/// nothing to remove. Errors are returned so the caller can say what was left; every step still runs.
pub async fn remove_checkout(base_dir: &Path, crewmate_id: &str) -> Result<(), String> {
    let worktree = work_dir(base_dir, crewmate_id);
    // tree-access: the store's own `crew/<id>/work`, a shared-clone worktree of a crewmate that is not isolated
    if !worktree.join(".git").exists() {
        return Ok(());
    }
    // The shared clone is the first entry of the worktree list, read from inside the worktree.
    let list = git_run(&["worktree", "list", "--porcelain"], Some(&worktree)).await?;
    let shared = list
        .lines()
        .find_map(|l| l.strip_prefix("worktree "))
        .map(|s| PathBuf::from(s.trim()))
        .ok_or("git worktree list named no shared clone")?;

    let mut left = vec![];
    let wt = worktree.to_string_lossy().into_owned();
    if let Err(e) = git_run(&["worktree", "remove", "--force", &wt], Some(&shared)).await {
        left.push(e);
    }
    if let Err(e) = git_run(&["worktree", "prune"], Some(&shared)).await {
        left.push(e);
    }
    let pattern = format!("refs/heads/atlas/{crewmate_id}/");
    match git_run(
        &["for-each-ref", "--format=%(refname:short)", &pattern],
        Some(&shared),
    )
    .await
    {
        Ok(branches) => {
            for b in branches.lines().filter(|b| !b.is_empty()) {
                if let Err(e) = git_run(&["branch", "-D", b], Some(&shared)).await {
                    left.push(e);
                }
            }
        }
        Err(e) => left.push(e),
    }
    if left.is_empty() {
        Ok(())
    } else {
        Err(left.join("; "))
    }
}

// ─── atlas.yaml ───────────────────────────────────────────────────────────────

/// Infer `owner/repo` from the `origin` git remote in `work_dir`.
///
/// Handles both HTTPS (`https://github.com/owner/repo`) and SSH
/// (`git@github.com:owner/repo`) remotes.
#[allow(dead_code)]
pub async fn infer_github_repo(work_dir: &Path, git: &Git) -> Option<String> {
    let out = git
        .run(Some(work_dir), &["remote", "get-url", "origin"])
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let remote = String::from_utf8_lossy(&out.stdout).trim().to_string();
    parse_github_repo(&remote)
}

// ─── Internals ────────────────────────────────────────────────────────────────

pub(crate) fn git_slug(url: &str) -> String {
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

/// Test-only: the program `git_run` starts in place of `git` (a fake that hangs, or records its calls).
#[cfg(test)]
pub(crate) static TEST_GIT_PROGRAM: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

pub(crate) fn git_program() -> String {
    #[cfg(test)]
    if let Some(p) = TEST_GIT_PROGRAM.lock().clone() {
        return p;
    }
    "git".to_string()
}

/// git in hostd's own tree (the shared clone of a non-isolated crewmate).
async fn git_run(args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    git_out(&Git::local(), cwd, args, true).await
}

/// `args` run by `git` (bounded by a deadline, its process tree killed with it: a clone that never
/// returns used to hold the checkout and the task until the 2 h task timeout, and a cancel could not
/// reach it, #296), its trimmed stdout, or its stderr as the error. `net`: it talks to the network.
async fn git_out(
    git: &Git,
    cwd: Option<&Path>,
    args: &[&str],
    net: bool,
) -> Result<String, String> {
    let out = if net {
        git.run_net(cwd, args).await?
    } else {
        git.run(cwd, args).await?
    };
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "git error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The lock of one repo's shared clone: two crewmates of the same repo take turns, crewmates of different repos check
/// out at the same time.
fn checkout_lock(shared_dir: &Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    type Locks = std::collections::HashMap<PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>;
    static LOCKS: parking_lot::Mutex<Option<Locks>> = parking_lot::Mutex::new(None);
    LOCKS
        .lock()
        .get_or_insert_with(Default::default)
        .entry(shared_dir.to_path_buf())
        .or_default()
        .clone()
}

/// The branch checked out in `dir`; `None` for a detached HEAD or a folder that is not a git repo.
/// `git` is who runs it (`CrewStore::git_in`: the crewmate itself for its own checkout).
pub async fn current_branch(dir: &Path, git: &Git) -> Option<String> {
    let b = git_out(
        git,
        Some(dir),
        &["symbolic-ref", "--short", "-q", "HEAD"],
        false,
    )
    .await
    .ok()?;
    (!b.is_empty()).then_some(b)
}

async fn prepare_git(
    url: &str,
    branch: Option<&str>,
    crewmate_id: &str,
    base_dir: &Path,
) -> Result<PathBuf, String> {
    let url = ssh_to_https(url);
    let url = url.as_str();
    let slug = git_slug(url);
    let shared_dir = base_dir.join("projects").join(&slug);
    // One checkout of a repo at a time: the clone starts in the background when a crewmate gets its project, and a task
    // may ask for the same checkout while it runs; the second caller waits and then finds it ready (it used to hit a
    // half-made clone). Per repo: a clone of one repo never holds up the checkout of another (#296).
    let lock = checkout_lock(&shared_dir);
    let _one_at_a_time = lock.lock().await;
    let worktree = work_dir(base_dir, crewmate_id);

    // Clone into shared dir if it doesn't exist yet.
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if !shared_dir.exists() {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
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

    // Add the per-crewmate worktree if the directory doesn't exist.
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if !worktree.exists() {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
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
        // Each crewmate gets its own local branch tracking it: git refuses a second worktree on a branch already checked
        // out (by the shared clone or another crewmate), and tracking lets `fetch` fast-forward it.
        // The full crewmate id: a prefix of it is not unique, and two crewmates sharing one collided on the same local branch.
        let local = format!(
            "atlas/{crewmate_id}/{}",
            upstream.trim_start_matches("origin/")
        );
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

/// The checkout of a crewmate that runs as its own user: a full clone in `<slug>/work`, which is
/// empty after provisioning. The clone is made **by the crewmate** (`git` is the spawn helper's git
/// request, run as its user), so the checkout is its own: it can write `.git` and edit every file, and
/// nothing in it was ever run by hostd. `core.sharedRepository=group` keeps what git writes there
/// writable by the crew group, which is how hostd adds attachments.
/// Idempotent: an existing checkout is returned as it is.
async fn prepare_isolated(
    url: &str,
    branch: Option<&str>,
    iso: &Isolated,
    git: &Git,
) -> Result<PathBuf, String> {
    // Same reason as `prepare_git`: a clone may be asked for twice at once (warm-up and first task).
    static CHECKOUT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one_at_a_time = CHECKOUT.lock().await;

    let work = iso.work();
    if super::spawn::layout::has_checkout(&work) {
        return Ok(work);
    }
    // `work/` comes from provisioning (the crewmate's directory is root's: hostd cannot make it).
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if !work.is_dir() {
        return Err(format!(
            "{} does not exist: the Linux user {} is not provisioned yet",
            work.display(),
            iso.user
        ));
    }
    let url = ssh_to_https(url);
    let work_str = work.to_string_lossy().into_owned();
    // A crewmate with a GitHub token of its own clones the real URL with it. Any other clones hostd's mirror, which
    // hostd fills with the crew user's login: that token is never given to a process that runs as the crewmate.
    let mirror = (!git.has_own_token()).then(|| Mirror::of(&url, iso));
    let source = match &mirror {
        Some(m) => {
            m.refresh().await?;
            m.local_url()
        }
        None => url.clone(),
    };
    let upload_pack = mirror.as_ref().map(Mirror::upload_pack);
    let mut args: Vec<&str> = vec![
        "clone",
        "--filter=blob:none",
        "-c",
        // What git writes in the checkout stays group-writable: hostd (the crew group) adds attachments.
        "core.sharedRepository=group",
    ];
    if let Some(u) = &upload_pack {
        args.push("--upload-pack");
        args.push(u);
    }
    if let Some(b) = branch {
        args.push("--branch");
        args.push(b);
    }
    args.push(&source);
    args.push(&work_str);
    git_out(git, Some(&iso.dir()), &args, true).await?;
    if mirror.is_some() {
        // Fetches read the mirror; a push still goes where the project lives.
        git_out(
            git,
            Some(&work),
            &["remote", "set-url", "--push", "origin", &url],
            false,
        )
        .await?;
    }
    Ok(work)
}

pub(crate) fn prepare_path(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    match std::fs::metadata(&p) {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
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

    // Incident: the first task of a crewmate on codeatlasdev/ac (branch feat/portal-cliente) failed with "fatal:
    // 'feat/portal-cliente' is already used by worktree": the shared clone had that branch checked out, and git refuses
    // a second worktree on it. The same happens to two crewmates on one branch. Each crewmate needs its own worktree that follows
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
        let a = prepare(&p, "crewmate-a", &base, None, &Git::local())
            .await
            .expect("first crewmate");
        let b = prepare(&p, "crewmate-b", &base, None, &Git::local())
            .await
            .expect("second crewmate on the same branch");
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
        assert_eq!(
            prepare(&p, "crewmate-a", &base, None, &Git::local())
                .await
                .unwrap(),
            a
        );
    }

    // The clone now starts in the background as soon as a crewmate gets its project (the first task waited ~90 s on
    // atlas-test-ship for it), so a task can call `prepare` while that clone is still running. Both must end with the
    // same working checkout; before, the second saw a half-made clone and failed.
    #[tokio::test]
    async fn a_task_waits_for_a_clone_already_in_progress() {
        for round in 0..5 {
            let tmp = tempfile::tempdir().unwrap();
            let url = remote_repo(tmp.path());
            let base = tmp.path().join("hostd");
            let p = ProjectRef::Git { url, branch: None };
            let git = Git::local();
            let (warm, task) = tokio::join!(
                prepare(&p, "crewmate-a", &base, None, &git),
                prepare(&p, "crewmate-a", &base, None, &git)
            );
            let (warm, task) = (
                warm.unwrap_or_else(|e| panic!("round {round}, background clone: {e}")),
                task.unwrap_or_else(|e| panic!("round {round}, task: {e}")),
            );
            assert_eq!(warm, task);
            assert!(task.join(".git").exists());
        }
    }

    // Issue #200: a crewmate that runs as its own user gets a full clone in its own directory (a
    // worktree would point into the crew home, which the crewmate cannot read), tracking the branch. The
    // clone is made by the crewmate: the helper's git request, here run by the real helper loop.
    // Isolation and the helper are Unix only.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_isolated_crewmate_gets_its_own_full_clone_not_a_worktree_of_a_shared_one() {
        let tmp = tempfile::tempdir().unwrap();
        let url = remote_repo(tmp.path());
        let base = tmp.path().join("hostd");
        let rig = crate::crew::spawn::testing::rig();
        let setup = rig.setup();
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let iso = setup.of(id).unwrap();
        let git = Git::crewmate(&setup.socket, iso.user.clone());
        let p = ProjectRef::Git {
            url,
            branch: Some("feat/x".into()),
        };
        let w = prepare(&p, id, &base, Some(iso.clone()), &git)
            .await
            .unwrap();
        assert_eq!(w, iso.work());
        // A full clone: `.git` is a directory with its own objects, not a file pointing elsewhere.
        assert!(w.join(".git").is_dir(), "a worktree has a .git file");
        // Nothing was put in the crew home: no shared clone, no worktree.
        assert!(!base.join("projects").exists());
        assert!(!base.join("crew").exists());
        // It follows the branch it was asked for.
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&w)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "feat/x");
        // What git writes there stays group-writable (hostd adds attachments through the crew group).
        let cfg = std::process::Command::new("git")
            .args(["config", "--get", "core.sharedRepository"])
            .current_dir(&w)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&cfg.stdout).trim(), "group");
        // A second task reuses it.
        assert_eq!(prepare(&p, id, &base, Some(iso), &git).await.unwrap(), w);
    }

    // A commit lands on the remote; `fetch` brings the crewmate's own checkout up to date through the helper.
    // Isolation and the helper are Unix only.
    #[cfg(unix)]
    #[tokio::test]
    async fn fetch_updates_the_isolated_checkout_through_the_helper() {
        let tmp = tempfile::tempdir().unwrap();
        let url = remote_repo(tmp.path());
        let rig = crate::crew::spawn::testing::rig();
        let setup = rig.setup();
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let iso = setup.of(id).unwrap();
        let git = Git::crewmate(&setup.socket, iso.user.clone());
        let p = ProjectRef::Git { url, branch: None };
        let w = prepare(&p, id, &tmp.path().join("hostd"), Some(iso.clone()), &git)
            .await
            .unwrap();
        let src = tmp.path().join("src");
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
                "two",
            ],
        );
        fetch(
            &w,
            &git,
            crate::crew::mirror::mirror_for(&p, Some(&iso), &git).as_ref(),
        )
        .await
        .unwrap();
        let log = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(&w)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 2);
    }

    // Security review of the crew isolation, N4: `fetch` ran `git worktree list` in the crewmate's checkout and
    // fetched in the first directory it named. The crewmate controls that output (a directory it made, swapped for a
    // link to another crewmate's tree while hostd ran `gh auth token`), so the next `git fetch` ran, as the crewmate,
    // in a directory of the crewmate's choosing. For an isolated crewmate the checkout is a full clone: it fetches
    // in its own `work_dir`. Here the checkout is a linked worktree of a repository elsewhere in the crewmate's
    // directory, so `worktree list` names that other directory first; a hook of that repository records where git
    // ran.
    #[cfg(unix)]
    #[tokio::test]
    async fn fetch_for_an_isolated_crewmate_runs_in_its_own_work_dir_and_not_where_worktree_list_points()
     {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let url = remote_repo(tmp.path());
        let rig = crate::crew::spawn::testing::rig();
        let setup = rig.setup();
        let iso = setup.of("3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b").unwrap();
        let git = Git::crewmate(&setup.socket, iso.user.clone());
        let shared = rig.cm.join("shared");
        sh(
            tmp.path(),
            &["clone", "-q", &url, &shared.to_string_lossy()],
        );
        let work = rig.work();
        sh(
            &shared,
            &[
                "worktree",
                "add",
                "-q",
                "--track",
                "-B",
                "wt",
                &work.to_string_lossy(),
                "origin/main",
            ],
        );
        let seen = rig.cm.join("where.log");
        let hook = shared.join(".git/hooks/reference-transaction");
        std::fs::write(&hook, format!("#!/bin/sh\npwd -P >> {}\n", seen.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        sh(
            &tmp.path().join("src"),
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "two",
            ],
        );
        fetch(&work, &git, None).await.unwrap();
        let ran: Vec<String> = std::fs::read_to_string(&seen)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        let work = std::fs::canonicalize(&work).unwrap().display().to_string();
        assert!(
            !ran.is_empty(),
            "no git command touched a ref, so this proved nothing"
        );
        assert!(
            ran.iter().all(|d| *d == work),
            "git ran outside the crewmate's work_dir {work}: {ran:?}"
        );
    }

    // Security review of the crew isolation, N2: `fetch` and `clone` put the crew user's `gh auth token` into
    // the environment of the git process that ran as the crewmate, in a repo whose config and hooks the crewmate
    // controls (and `/proc/<pid>/environ` is readable by its own uid): a crewmate could read the token of the
    // operator's account, which reaches every repository that account can. hostd now fetches into a mirror it
    // owns with the token and the crewmate clones and fetches from that path with none.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_crew_users_github_token_never_reaches_a_process_run_as_a_crewmate() {
        use std::os::unix::fs::PermissionsExt;
        let _bin = crate::crew::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        const TOKEN: &str = "ghp_crew_user_login_token_0123456789";
        let tmp = tempfile::tempdir().unwrap();
        let gh = tmp.path().join("gh");
        std::fs::write(
            &gh,
            format!("#!/bin/sh\n[ \"$1 $2\" = \"auth token\" ] && echo {TOKEN}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        *crate::crew::proc::TEST_GH_PROGRAM.lock() = Some(gh.display().to_string());

        let url = remote_repo(tmp.path());
        let rig = crate::crew::spawn::testing::rig();
        let log = rig.env_recording_hooks();
        let setup = rig.setup();
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let iso = setup.of(id).unwrap();
        let git = Git::crewmate(&setup.socket, iso.user.clone());
        let p = ProjectRef::Git { url, branch: None };
        let w = prepare(&p, id, &tmp.path().join("hostd"), Some(iso.clone()), &git).await;
        let w = w.unwrap();
        sh(
            &tmp.path().join("src"),
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "two",
            ],
        );
        fetch(
            &w,
            &git,
            crate::crew::mirror::mirror_for(&p, Some(&iso), &git).as_ref(),
        )
        .await
        .unwrap();
        *crate::crew::proc::TEST_GH_PROGRAM.lock() = None;

        let seen = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            seen.contains("HOME="),
            "the recording hooks never ran, so this proved nothing: {seen:?}"
        );
        assert!(
            !seen.contains(TOKEN) && !seen.contains("ATLAS_GIT_TOKEN"),
            "the crew user's GitHub token is in a crewmate's git process:\n{seen}"
        );
        let log = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(&w)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 2);
    }

    #[test]
    fn work_dir_for_is_the_store_dir_without_isolation_and_the_crewmates_own_with_it() {
        let base = Path::new("/h");
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        assert_eq!(work_dir_for(base, id, None), work_dir(base, id));
        let iso = Isolated::under("/r", id).unwrap();
        assert_eq!(
            work_dir_for(base, id, Some(&iso)),
            PathBuf::from("/r/3f2a9c1e5b7d/work")
        );
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

    // Merge of #223 (isolation) with #262 (`locate` reads the work tree): `locate` asked for the
    // store's `work/` checkout, but an isolated crewmate's agent works in its own folder, so the tool
    // would have read a tree that does not exist (or a stale one).
    #[test]
    fn the_current_work_dir_of_an_isolated_crewmate_is_its_own_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("hostd");
        let root = tmp.path().join("cm");
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let mate: crate::crew::store::Crewmate = serde_json::from_value(serde_json::json!({
            "id": id, "name": "Iso", "role": "r", "agent": "omp", "approval": "auto",
            "created_at": 1, "project": {"kind": "git", "url": "https://example.com/o/r.git"},
        }))
        .unwrap();
        let iso = Isolated::under(root.to_str().unwrap(), id).unwrap();
        std::fs::create_dir_all(iso.work()).unwrap();
        std::fs::create_dir_all(work_dir(&base, id)).unwrap();
        assert_eq!(
            current_work_dir(&base, &mate, Some(&iso)).unwrap(),
            iso.work()
        );
        assert_eq!(
            current_work_dir(&base, &mate, None).unwrap(),
            work_dir(&base, id)
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

    fn git_out(dir: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(o.status.success(), "git {args:?}");
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    // Regression (#297): deleting a crewmate removed `crew/<id>/` with `remove_dir_all` only, so the
    // shared clone kept the worktree's metadata and the local branch `atlas/<id>/...` for good.
    #[tokio::test]
    async fn removing_a_checkout_leaves_no_worktree_or_branch_in_the_shared_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let url = remote_repo(tmp.path());
        let base = tmp.path().join("hostd");
        let p = ProjectRef::Git {
            url,
            branch: Some("feat/x".into()),
        };
        let a = prepare(&p, "crewmate-a", &base, None, &Git::local())
            .await
            .unwrap();
        let b = prepare(&p, "crewmate-b", &base, None, &Git::local())
            .await
            .unwrap();
        // Uncommitted work in the worktree goes with the crewmate.
        std::fs::write(a.join("scratch.txt"), "wip").unwrap();
        let shared = std::fs::read_dir(base.join("projects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(
            git_out(&shared, &["branch", "--list", "atlas/crewmate-a/*"])
                .contains("atlas/crewmate-a/feat/x")
        );

        remove_checkout(&base, "crewmate-a").await.unwrap();

        assert!(!a.exists(), "the worktree directory is gone");
        let worktrees = git_out(&shared, &["worktree", "list", "--porcelain"]);
        assert!(!worktrees.contains("crewmate-a"), "{worktrees}");
        assert!(
            git_out(&shared, &["branch", "--list", "atlas/crewmate-a/*"])
                .trim()
                .is_empty()
        );
        // The other crewmate is untouched, and a new worktree for the same id can be made again.
        assert!(worktrees.contains("crewmate-b") && b.join(".git").exists());
        assert!(
            git_out(&shared, &["branch", "--list", "atlas/crewmate-b/*"]).contains("crewmate-b")
        );
        prepare(&p, "crewmate-a", &base, None, &Git::local())
            .await
            .expect("same id again");
        // A crewmate with no checkout has nothing to remove.
        remove_checkout(&base, "never-had-one").await.unwrap();
    }

    // Regression (security review of #448, B2): the branch, the remote and the fetch ran git as the crew
    // user in the crewmate's own tree. All of them go through the helper now: a planted `core.fsmonitor`
    // only ever runs as the crewmate (the planted command records `$USER`).
    #[cfg(unix)]
    #[tokio::test]
    async fn the_branch_the_remote_and_the_fetch_never_run_a_crewmates_git_config_as_hostd() {
        use crate::crew::spawn::testing::{Rig, USER, rig};
        let rig = rig();
        let marker = rig.booby_trapped_repo();
        let hostd = tempfile::tempdir().unwrap();
        let store =
            crate::crew::store::CrewStore::new(hostd.path()).with_isolation(Some(rig.setup()));
        let work = rig.work();
        let git = store.git_in(&work);
        assert!(git.is_crewmate());
        assert!(current_branch(&work, &git).await.is_some());
        let _ = infer_github_repo(&work, &git).await;
        fetch(&work, &git, None).await.unwrap();
        let users = Rig::marker_users(&marker);
        assert!(!users.is_empty(), "no command triggered core.fsmonitor");
        assert!(users.iter().all(|u| u == USER), "{users:?}");
    }
}
