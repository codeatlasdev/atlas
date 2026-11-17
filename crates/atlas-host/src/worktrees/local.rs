//! hostd's own worktree lifecycle for a checkout on THIS machine (Atlas Mobile, desktop closed):
//! list, create and remove, with the layout, branch naming, `worktree:` hooks and "would this lose work?"
//! rule of the desktop's `worktree_create` / `worktree_auto_remove`, over the same registry file.
//!
//! Every function takes the registry `base` (`~/.atlas`, see `ServeOpts::registry`) so tests run in a
//! sandbox. SSH checkouts are never listed or touched here (the desktop owns those).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::{Value, json};

use super::clean::{self, Fut, Out};
use super::naming::{free_offset, random_sea_name, slug};
use super::registry::{self, Entry};

/// Longest a phone-triggered `worktree.archive` hook may run.
const ARCHIVE_TIMEOUT_MS: u64 = 120_000;

/// `~/.atlas/desktop/settings.json` as the desktop writes it; missing or unreadable = defaults.
fn settings(base: &Path) -> serde_json::Map<String, Value> {
    std::fs::read_to_string(base.join("desktop").join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn setting(s: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    s.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|v| !v.is_empty())
}

/// Where new worktrees go (`workspaceDir`, default `~/.atlas/worktrees`).
pub fn workspace_dir(s: &serde_json::Map<String, Value>) -> PathBuf {
    setting(s, "workspaceDir")
        .map(|v| PathBuf::from(atlas_core::util::expand_tilde(&v)))
        .unwrap_or_else(|| atlas_core::paths::atlas_home().join("worktrees"))
}

/// One command: its exit code (`None` = timed out) and output.
async fn run(
    cwd: &Path,
    argv: &[&str],
    env: &BTreeMap<String, String>,
    timeout_ms: u64,
) -> Result<Out, String> {
    let argv: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
    let v = crate::exec::run(&cwd.to_string_lossy(), &argv, env, None, timeout_ms).await?;
    let text = |k: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(v[k].as_str().unwrap_or_default())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    };
    Ok(Out {
        code: if v["timed_out"] == true {
            None
        } else {
            v["code"].as_i64()
        },
        stdout: text("stdout"),
        stderr: text("stderr"),
    })
}

/// `git -C dir args…`; non-zero exit → stderr as the error.
async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut argv = vec!["git"];
    argv.extend(args);
    let o = run(dir, &argv, &BTreeMap::new(), 60_000).await?;
    if o.code == Some(0) {
        Ok(o.stdout)
    } else {
        Err(format!("git {}: {}", args.join(" "), o.stderr.trim()))
    }
}

async fn ref_exists(dir: &Path, r: &str) -> bool {
    run(
        dir,
        &["git", "show-ref", "--verify", "--quiet", r],
        &BTreeMap::new(),
        15_000,
    )
    .await
    .is_ok_and(|o| o.code == Some(0))
}

/// `origin/<default>` when the remote has a HEAD, else `HEAD` (the desktop's `default_base`).
async fn default_base(repo: &Path) -> String {
    match git(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "HEAD".to_string(),
    }
}

/// A `git` runner for [`clean::keep_reason`]: `git <args>` in `dir` on this machine.
fn runner(dir: PathBuf) -> impl Fn(Vec<String>) -> Fut + Send + Sync {
    move |args| {
        let dir = dir.clone();
        Box::pin(async move {
            let mut argv = vec!["git".to_string()];
            argv.extend(args);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run(&dir, &refs, &BTreeMap::new(), 60_000).await
        })
    }
}

fn load_local(base: &Path) -> Result<Vec<Entry>, String> {
    Ok(registry::load_at(&registry::path_in(base))?
        .into_iter()
        .filter(|e| e.host.is_none())
        .collect())
}

/// The main checkout `id` belongs to (`id` itself for a repo, its parent for a worktree).
fn repo_of(all: &[Entry], id: &str) -> Result<Entry, String> {
    let e = all
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown project {id}"))?;
    match &e.parent {
        Some(p) => all
            .iter()
            .find(|x| &x.id == p)
            .cloned()
            .ok_or_else(|| "parent missing".to_string()),
        None => Ok(e.clone()),
    }
}

/// A worktree's directory, by registry id (a repo's own id gives its main checkout). Never a path
/// from the caller: the phone can only name what the registry knows, on this machine.
pub fn dir_of(base: &Path, id: &str) -> Result<PathBuf, String> {
    load_local(base)?
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| PathBuf::from(e.root))
        .ok_or_else(|| format!("unknown worktree {id}"))
}

async fn row(e: &Entry, main: bool) -> Value {
    let dir = Path::new(&e.root);
    let exists = dir.is_dir();
    let (mut branch, mut dirty) = (e.branch.clone(), false);
    if exists {
        if let Ok(b) = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await {
            branch = Some(b.trim().to_string()).filter(|b| !b.is_empty());
        }
        dirty = git(dir, &["status", "--porcelain"])
            .await
            .is_ok_and(|s| !s.trim().is_empty());
    }
    json!({
        "id": e.id,
        "path": e.root,
        "name": dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "branch": branch,
        "main": main,
        "dirty": dirty,
        "exists": exists,
        "portOffset": e.port_offset,
    })
}

/// The repo of `project` (a repo or worktree id) and its linked worktrees, main checkout first,
/// git state read live.
pub async fn list(base: &Path, project: &str) -> Result<Value, String> {
    let all = load_local(base)?;
    let repo = repo_of(&all, project)?;
    let mut rows = vec![row(&repo, true).await];
    for e in all
        .iter()
        .filter(|e| e.parent.as_deref() == Some(repo.id.as_str()))
    {
        rows.push(row(e, false).await);
    }
    Ok(json!({"repoId": repo.id, "worktrees": rows}))
}

/// Everything `worktree_create` accepts. Both fields are free text from a phone: `name` is slugged and
/// `base` must name an existing ref, so neither can carry an option or a path.
#[derive(Default)]
pub struct CreateReq {
    pub name: Option<String>,
    pub base: Option<String>,
}

/// `branchPrefix` setting: the git user name (default), a custom slug or none.
async fn branch_prefix(s: &serde_json::Map<String, Value>, repo: &Path) -> Option<String> {
    match setting(s, "branchPrefix")
        .as_deref()
        .unwrap_or("git-username")
    {
        "none" => None,
        "custom" => setting(s, "branchPrefixCustom")
            .map(|v| slug(&v))
            .filter(|v| !v.is_empty()),
        _ => git(repo, &["config", "user.name"])
            .await
            .ok()
            .map(|n| slug(n.trim()))
            .filter(|v| !v.is_empty()),
    }
}

/// The `base` ref, validated: a plain ref name that resolves to a commit here.
async fn resolve_base(repo: &Path, base: Option<String>) -> Result<String, String> {
    let Some(b) = base.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()) else {
        return Ok(default_base(repo).await);
    };
    if b.starts_with('-') || b.contains(char::is_whitespace) || b.contains("..") {
        return Err(format!("{b:?} is not a branch name"));
    }
    git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{b}^{{commit}}"),
        ],
    )
    .await
    .map_err(|_| format!("unknown base {b}"))?;
    Ok(b)
}

/// `worktree.shared`: a gitignored directory of the main checkout is linked, a file copied; nothing
/// happens when the source is missing or the target exists.
fn share(root: &Path, wt: &Path, rel: &str) -> Result<(), String> {
    if rel.contains("..") || Path::new(rel).is_absolute() {
        return Err(format!(
            "worktree.shared: {rel} must be relative to the repo"
        ));
    }
    let (src, dst) = (root.join(rel), wt.join(rel));
    if src.symlink_metadata().is_err() || dst.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{rel}: {e}"))?;
    }
    if src.is_dir() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dst).map_err(|e| format!("{rel}: {e}"))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&src, &dst).map_err(|e| format!("{rel}: {e}"))?;
    } else {
        std::fs::copy(&src, &dst).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

/// The env of `worktree.setup` / `archive`, as the desktop sets it.
pub fn hook_env(root: &Path, wt: &Path, name: &str, offset: u16) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "ATLAS_ROOT_PATH".to_string(),
            root.to_string_lossy().into_owned(),
        ),
        (
            "ATLAS_WORKTREE_PATH".to_string(),
            wt.to_string_lossy().into_owned(),
        ),
        ("ATLAS_WORKSPACE_NAME".to_string(), name.to_string()),
        ("ATLAS_PORT_OFFSET".to_string(), offset.to_string()),
    ])
}

/// The `worktree:` block of `root`'s `atlas.yaml` (all empty when absent or invalid, as for the desktop).
struct Hooks {
    setup: Option<String>,
    archive: Option<String>,
    shared: Vec<String>,
    seed: Option<Vec<String>>,
    setup_timeout: u64,
}

async fn worktree_config(root: &Path) -> Hooks {
    let r = root.to_path_buf();
    let w = tokio::task::spawn_blocking(move || atlas_tui::config::load(&r))
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|c| c.worktree);
    match w {
        Some(w) => Hooks {
            setup: w.setup,
            archive: w.archive,
            shared: w.shared,
            seed: w.seed,
            setup_timeout: w.setup_timeout,
        },
        None => Hooks {
            setup: None,
            archive: None,
            shared: Vec::new(),
            seed: None,
            setup_timeout: 600,
        },
    }
}

/// Create a worktree of `project` on this machine. The same steps as the desktop's `worktree_create`
/// (fetch, `git worktree add` with the base recorded, `shared` links, copy-on-write `seed`, registry
/// entry with the next free port offset) and then `worktree.setup`, which the desktop runs in a tab: here
/// it runs headless and its outcome is `setup: {ok, code, output}`. A failing setup never undoes the checkout.
pub async fn create(base: &Path, project: &str, req: CreateReq) -> Result<Value, String> {
    let all = load_local(base)?;
    let repo = repo_of(&all, project)?;
    let root = PathBuf::from(&repo.root);
    let s = settings(base);
    let repo_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut name = slug(req.name.as_deref().unwrap_or_default().trim());
    if name.is_empty() {
        name = random_sea_name();
    }
    let base_ref = resolve_base(&root, req.base).await?;
    let prefix = branch_prefix(&s, &root).await;
    let branch_for = |n: &str| match &prefix {
        Some(p) => format!("{p}/{n}"),
        None => n.to_string(),
    };

    let ws_dir = workspace_dir(&s);
    let parent_dir = if setting(&s, "nestWorkspaces").as_deref() == Some("false") {
        ws_dir.clone()
    } else {
        ws_dir.join(&repo_name)
    };
    // A taken directory means the same name was used before, so its branch is taken too (git refuses two
    // worktrees on one branch): the suffix goes on both, and the phone's second tap gets a fresh checkout.
    let stem = name;
    let mut name = stem.clone();
    let mut dir = parent_dir.join(&name);
    let mut k = 2;
    while dir.exists() {
        name = format!("{stem}-{k}");
        dir = parent_dir.join(&name);
        k += 1;
    }
    let branch = branch_for(&name);
    std::fs::create_dir_all(&parent_dir).map_err(|e| format!("{}: {e}", parent_dir.display()))?;
    atlas_core::hygiene::mark_never_index(&parent_dir);
    atlas_core::hygiene::mark_never_index(&ws_dir);
    let dir_s = dir.to_string_lossy().into_owned();

    let _ = git(&root, &["fetch", "origin", "--prune"]).await;
    let remote = format!("origin/{branch}");
    if ref_exists(&root, &format!("refs/heads/{branch}")).await {
        git(&root, &["worktree", "add", &dir_s, &branch]).await?;
    } else if ref_exists(&root, &format!("refs/remotes/{remote}")).await {
        git(
            &root,
            &["worktree", "add", "--track", "-b", &branch, &dir_s, &remote],
        )
        .await?;
    } else {
        git(
            &root,
            &[
                "worktree",
                "add",
                "--no-track",
                "-b",
                &branch,
                &dir_s,
                &base_ref,
            ],
        )
        .await?;
    }
    let _ = git(
        &dir,
        &[
            "config",
            "--local",
            &format!("branch.{branch}.base"),
            &base_ref,
        ],
    )
    .await;
    if git(&dir, &["config", "--get", "push.autoSetupRemote"])
        .await
        .is_err()
    {
        let _ = git(&dir, &["config", "--local", "push.autoSetupRemote", "true"]).await;
    }

    let wcfg = worktree_config(&root).await;
    for rel in &wcfg.shared {
        share(&root, &dir, rel)?;
    }
    let seed = {
        let (r, d, configured) = (root.clone(), dir.clone(), wcfg.seed.clone());
        tokio::task::spawn_blocking(move || {
            let plan = atlas_core::seed::plan(&r, &d, configured.as_deref());
            if plan.items.is_empty() && plan.skipped.is_empty() {
                return None;
            }
            Some(atlas_core::seed::run(&r, &d, plan))
        })
        .await
        .map_err(|e| e.to_string())?
    };

    let real = dir.canonicalize().unwrap_or(dir.clone());
    // Offset and insert under one lock (shared with the desktop): two worktrees created at once used to
    // read the same free offset and start stacks on the same ports.
    let entry = registry::edit_at(&registry::path_in(base), |entries| {
        let entry = Entry {
            id: registry::id_for_host(None, &real),
            root: real.to_string_lossy().into_owned(),
            pinned: false,
            added: chrono::Utc::now().to_rfc3339(),
            parent: Some(repo.id.clone()),
            port_offset: free_offset(entries, &repo.id),
            branch: Some(branch.clone()),
            host: None,
            identity: None,
            project: false,
        };
        entries.retain(|e| e.id != entry.id);
        entries.push(entry.clone());
        Ok(entry)
    })?;

    let setup = match &wcfg.setup {
        Some(cmd) => {
            let argv = atlas_core::sys::shell_argv(cmd);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            let o = run(
                &real,
                &refs,
                &hook_env(&root, &real, &name, entry.port_offset),
                wcfg.setup_timeout.saturating_mul(1000),
            )
            .await?;
            let output = format!("{}{}", o.stdout, o.stderr);
            let skip = output.chars().count().saturating_sub(4000);
            json!({"ok": o.code == Some(0), "code": o.code, "output": output.chars().skip(skip).collect::<String>()})
        }
        None => Value::Null,
    };
    Ok(json!({
        "worktree": {"id": entry.id, "path": entry.root, "branch": branch, "base": base_ref,
                     "portOffset": entry.port_offset, "repoId": repo.id, "name": name},
        "setup": setup,
        "seed": seed,
    }))
}

/// Remove worktree `id` only when nothing would be lost: the desktop's auto clean-up rule
/// ([`clean::keep_reason`]: dirty, secrets, unpushed), never `--force`, branch kept so it can be
/// restored. Answers `{kept: reason}` or `{removed: true, branch}`; a phone cannot force.
pub async fn remove(base: &Path, id: &str) -> Result<Value, String> {
    let all = load_local(base)?;
    let e = all
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or("unknown worktree")?;
    let parent = e
        .parent
        .clone()
        .ok_or("the main checkout cannot be deleted")?;
    let root = PathBuf::from(
        &all.iter()
            .find(|x| x.id == parent)
            .ok_or("parent missing")?
            .root,
    );
    let wt = PathBuf::from(&e.root);
    let wt_s = wt.to_string_lossy().into_owned();
    if wt.is_dir() {
        if let Some(why) = clean::keep_reason(&runner(wt.clone()), None).await? {
            return Ok(json!({"kept": why}));
        }
        // This worktree's stack stops first, as the desktop does, so nothing keeps its files open.
        let _ = atlas_tui::runner::down(&wt, std::time::Duration::from_secs(10)).await;
        if let Some(cmd) = worktree_config(&root).await.archive {
            let name = wt
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let argv = atlas_core::sys::shell_argv(&cmd);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            let _ = run(
                &wt,
                &refs,
                &hook_env(&root, &wt, &name, e.port_offset),
                ARCHIVE_TIMEOUT_MS,
            )
            .await;
        }
        git(&root, &["worktree", "remove", &wt_s]).await?;
    } else {
        let _ = git(&root, &["worktree", "prune"]).await;
    }
    registry::edit_at(&registry::path_in(base), |all| {
        all.retain(|x| x.id != e.id);
        Ok(())
    })?;
    Ok(json!({"removed": true, "branch": e.branch}))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    /// A sandbox: `base` plays `~/.atlas` (registry + settings), `repo` is a real git repo with an
    /// `origin` that has the pushed main branch.
    struct Box {
        _tmp: tempfile::TempDir,
        base: PathBuf,
        repo: PathBuf,
        remote: PathBuf,
        id: String,
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=Ada Lovelace",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    fn sandbox() -> Box {
        let tmp = tempfile::tempdir().unwrap();
        // Canonical: macOS temp dirs are symlinks (/var → /private/var) and git resolves them.
        let root = tmp.path().canonicalize().unwrap();
        let base = root.join("atlas");
        let repo = root.join("shop");
        let remote = root.join("remote.git");
        std::fs::create_dir_all(&repo).unwrap();
        git(&root, &["init", "-q", "--bare", "remote.git"]);
        git(&repo, &["init", "-q", "."]);
        std::fs::write(repo.join("f.txt"), "base\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        git(
            &repo,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        git(&repo, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
        git(&repo, &["fetch", "-q", "origin"]);
        git(&repo, &["remote", "set-head", "origin", "main"]);
        let id = registry::id_for(&repo);
        std::fs::create_dir_all(base.join("desktop")).unwrap();
        std::fs::write(
            base.join("desktop/projects.json"),
            json!([{"id": id, "root": repo, "pinned": false, "added": "x"}]).to_string(),
        )
        .unwrap();
        // Worktrees go inside the sandbox, never ~/.atlas/worktrees.
        std::fs::write(
            base.join("desktop/settings.json"),
            json!({"workspaceDir": root.join("worktrees"), "branchPrefix": "none"}).to_string(),
        )
        .unwrap();
        Box {
            _tmp: tmp,
            base,
            repo,
            remote,
            id,
        }
    }

    #[tokio::test]
    async fn create_makes_a_real_worktree_in_the_desktop_layout_and_registers_it() {
        let b = sandbox();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("Fix: Login bug!".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let wt = &v["worktree"];
        let path = PathBuf::from(wt["path"].as_str().unwrap());
        // <workspaceDir>/<repo>/<slug>, same as the desktop
        assert_eq!(
            path,
            b.repo
                .parent()
                .unwrap()
                .join("worktrees/shop/fix-login-bug")
        );
        assert!(path.join("f.txt").is_file(), "checked out");
        assert_eq!(wt["branch"], "fix-login-bug");
        assert_eq!(wt["base"], "origin/main");
        assert_eq!(wt["portOffset"], 100);
        assert_eq!(wt["name"], "fix-login-bug");
        assert_eq!(wt["repoId"], b.id);
        // git agrees, and the desktop's per-branch base config is set like it does
        assert_eq!(
            git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "fix-login-bug"
        );
        assert_eq!(
            git(&path, &["config", "branch.fix-login-bug.base"]).trim(),
            "origin/main"
        );
        // the registry the desktop reads now has it, as a child of the repo
        let all = registry::load_at(&registry::path_in(&b.base)).unwrap();
        let e = all
            .iter()
            .find(|e| e.id == wt["id"].as_str().unwrap())
            .unwrap();
        assert_eq!(e.parent.as_deref(), Some(b.id.as_str()));
        assert_eq!(e.branch.as_deref(), Some("fix-login-bug"));
        assert_eq!(e.port_offset, 100);
    }

    #[tokio::test]
    async fn an_empty_name_gets_a_sea_name_and_a_taken_one_a_suffix() {
        let b = sandbox();
        let a = create(&b.base, &b.id, CreateReq::default()).await.unwrap();
        assert!(!a["worktree"]["name"].as_str().unwrap().is_empty());
        let one = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("same".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let two = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("same".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        assert!(
            one["worktree"]["path"]
                .as_str()
                .unwrap()
                .ends_with("/shop/same")
        );
        assert!(
            two["worktree"]["path"]
                .as_str()
                .unwrap()
                .ends_with("/shop/same-2")
        );
        // the second dir got a branch of its own (git refuses two worktrees on one branch)
        assert_ne!(one["worktree"]["id"], two["worktree"]["id"]);
        assert_eq!(two["worktree"]["portOffset"], 300, "100 and 200 are taken");
    }

    #[tokio::test]
    async fn base_must_be_an_existing_ref_never_an_option() {
        let b = sandbox();
        let bad = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("x".into()),
                base: Some("--upload-pack=touch /tmp/pwned".into()),
            },
        )
        .await;
        assert!(bad.is_err(), "an option-looking base is refused");
        let missing = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("x".into()),
                base: Some("no-such-ref".into()),
            },
        )
        .await;
        assert!(missing.is_err());
        assert!(
            registry::load_at(&registry::path_in(&b.base))
                .unwrap()
                .len()
                == 1,
            "a failed create registers nothing"
        );
        let ok = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("y".into()),
                base: Some("origin/main".into()),
            },
        )
        .await;
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[tokio::test]
    async fn create_honors_worktree_shared_seed_and_setup_from_atlas_yaml() {
        let b = sandbox();
        std::fs::write(b.repo.join(".gitignore"), ".env\nnode_modules/\n").unwrap();
        std::fs::write(b.repo.join(".env"), "SECRET=1").unwrap();
        std::fs::write(
            b.repo.join("atlas.yaml"),
            "name: shop\nservices:\n  api: { command: 'true' }\nworktree:\n  shared: ['.env']\n  seed: []\n  setup: 'echo \"$ATLAS_WORKSPACE_NAME:$ATLAS_PORT_OFFSET\" > setup.out'\n",
        )
        .unwrap();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("hooks".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let path = PathBuf::from(v["worktree"]["path"].as_str().unwrap());
        assert_eq!(
            std::fs::read_to_string(path.join(".env")).unwrap(),
            "SECRET=1",
            "shared file copied"
        );
        assert_eq!(
            std::fs::read_to_string(path.join("setup.out"))
                .unwrap()
                .trim(),
            "hooks:100",
            "setup ran in the worktree with the desktop's env"
        );
        assert_eq!(v["setup"]["ok"], true);
    }

    #[tokio::test]
    async fn a_failing_setup_is_reported_but_keeps_the_worktree() {
        let b = sandbox();
        std::fs::write(
            b.repo.join("atlas.yaml"),
            "name: shop\nservices:\n  api: { command: 'true' }\nworktree:\n  setup: 'echo boom >&2; exit 3'\n",
        )
        .unwrap();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("bad".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(v["setup"]["ok"], false);
        assert!(v["setup"]["output"].as_str().unwrap().contains("boom"));
        assert!(PathBuf::from(v["worktree"]["path"].as_str().unwrap()).is_dir());
    }

    #[tokio::test]
    async fn list_shows_the_repo_and_its_worktrees_with_git_state() {
        let b = sandbox();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("a".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let wt_id = v["worktree"]["id"].as_str().unwrap().to_string();
        let path = PathBuf::from(v["worktree"]["path"].as_str().unwrap());
        let l = list(&b.base, &b.id).await.unwrap();
        let rows = l["worktrees"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["main"], true);
        assert_eq!(rows[0]["id"], b.id);
        let w = rows.iter().find(|r| r["id"] == wt_id).unwrap();
        assert_eq!(
            (w["main"].clone(), w["branch"].clone()),
            (json!(false), json!("a"))
        );
        assert_eq!(w["dirty"], false);
        // listing reads git live: a dirty tree shows
        std::fs::write(path.join("new.txt"), "x").unwrap();
        let l = list(&b.base, &b.id).await.unwrap();
        let w = l["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == wt_id)
            .unwrap()
            .clone();
        assert_eq!(w["dirty"], true);
        // asking with a worktree's id answers for its repo (the phone may hold either)
        assert_eq!(
            list(&b.base, &wt_id).await.unwrap()["worktrees"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(list(&b.base, "nope").await.is_err());
    }

    #[tokio::test]
    async fn remove_deletes_a_clean_pushed_worktree_and_keeps_its_branch() {
        let b = sandbox();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("done".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let id = v["worktree"]["id"].as_str().unwrap();
        let path = PathBuf::from(v["worktree"]["path"].as_str().unwrap());
        let r = remove(&b.base, id).await.unwrap();
        assert_eq!(r["removed"], true);
        assert!(!path.exists(), "directory gone");
        assert!(
            registry::load_at(&registry::path_in(&b.base))
                .unwrap()
                .iter()
                .all(|e| e.id != id)
        );
        // like the desktop's auto clean: the branch stays so the work can be restored
        assert!(git(&b.repo, &["branch", "--list", "done"]).contains("done"));
    }

    #[tokio::test]
    async fn remove_refuses_dirty_untracked_secrets_and_unpushed_work() {
        let b = sandbox();
        for (what, reason) in [
            ("tracked", "dirty"),
            ("untracked", "dirty"),
            ("secret", "secrets"),
            ("commit", "unpushed"),
        ] {
            let v = create(
                &b.base,
                &b.id,
                CreateReq {
                    name: Some(what.into()),
                    base: None,
                },
            )
            .await
            .unwrap();
            let id = v["worktree"]["id"].as_str().unwrap();
            let path = PathBuf::from(v["worktree"]["path"].as_str().unwrap());
            match what {
                "tracked" => std::fs::write(path.join("f.txt"), "edit\n").unwrap(),
                "untracked" => std::fs::write(path.join("new.txt"), "x").unwrap(),
                "secret" => {
                    std::fs::write(path.join(".gitignore"), ".env*\n").unwrap();
                    git(&path, &["add", ".gitignore"]);
                    git(&path, &["commit", "-qm", "ignore"]);
                    git(&path, &["push", "-q", "origin", "HEAD"]);
                    std::fs::write(path.join(".env.local"), "S=1").unwrap();
                }
                _ => {
                    git(
                        &path,
                        &["commit", "-q", "--allow-empty", "-m", "local only"],
                    );
                }
            }
            let r = remove(&b.base, id).await.unwrap();
            assert_eq!(r["kept"], reason, "{what}");
            assert!(path.exists(), "{what}: nothing deleted");
            assert!(
                registry::load_at(&registry::path_in(&b.base))
                    .unwrap()
                    .iter()
                    .any(|e| e.id == id),
                "{what}: still registered"
            );
        }
    }

    #[tokio::test]
    async fn remove_never_touches_a_main_checkout_or_an_unknown_id() {
        let b = sandbox();
        assert!(
            remove(&b.base, &b.id)
                .await
                .unwrap_err()
                .contains("main checkout")
        );
        assert!(
            remove(&b.base, "nope")
                .await
                .unwrap_err()
                .contains("unknown worktree")
        );
        assert!(b.repo.join("f.txt").is_file());
    }

    #[tokio::test]
    async fn remove_runs_the_archive_hook_before_deleting() {
        let b = sandbox();
        let marker = b.repo.parent().unwrap().join("archived.txt");
        std::fs::write(
            b.repo.join("atlas.yaml"),
            format!(
                "name: shop\nservices:\n  api: {{ command: 'true' }}\nworktree:\n  archive: 'cp f.txt {}'\n",
                marker.display()
            ),
        )
        .unwrap();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("arch".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        remove(&b.base, v["worktree"]["id"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(marker).unwrap(),
            "base\n",
            "archive ran inside the worktree"
        );
    }

    #[tokio::test]
    async fn dir_of_resolves_worktrees_and_repos_but_not_strangers() {
        let b = sandbox();
        let v = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("d".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let wt = PathBuf::from(v["worktree"]["path"].as_str().unwrap());
        assert_eq!(
            dir_of(&b.base, v["worktree"]["id"].as_str().unwrap()).unwrap(),
            wt
        );
        assert_eq!(dir_of(&b.base, &b.id).unwrap(), b.repo);
        assert!(dir_of(&b.base, "/etc").is_err(), "an id, never a path");
        let _ = &b.remote;
    }

    /// SSH checkouts belong to the desktop: hostd neither lists nor removes them.
    #[tokio::test]
    async fn ssh_entries_are_invisible_to_hostd() {
        let b = sandbox();
        let mut all = registry::load_at(&registry::path_in(&b.base)).unwrap();
        all.push(Entry {
            id: "rem".into(),
            root: "/srv/x".into(),
            pinned: false,
            added: "x".into(),
            parent: Some(b.id.clone()),
            port_offset: 100,
            branch: Some("b".into()),
            host: Some("ssh:box".into()),
            identity: None,
            project: false,
        });
        std::fs::write(
            registry::path_in(&b.base),
            serde_json::to_string(&all).unwrap(),
        )
        .unwrap();
        assert_eq!(
            list(&b.base, &b.id).await.unwrap()["worktrees"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(remove(&b.base, "rem").await.is_err());
        assert!(dir_of(&b.base, "rem").is_err());
    }

    /// The phone parses these payloads (`apps/mobile/test/worktree_test.dart`): they are the REAL output of `list`
    /// and `create` on a real repo, with the temp paths and the registry hashes pinned. `ATLAS_UPDATE_FIXTURES=1`
    /// rewrites them.
    #[tokio::test]
    async fn the_phone_fixtures_are_the_real_payloads() {
        let b = sandbox();
        let made = create(
            &b.base,
            &b.id,
            CreateReq {
                name: Some("phone task".into()),
                base: None,
            },
        )
        .await
        .unwrap();
        let wt_id = made["worktree"]["id"].as_str().unwrap().to_string();
        let tmp_root = b.repo.parent().unwrap().to_string_lossy().into_owned();
        let pin = |v: &Value| {
            serde_json::to_string_pretty(v)
                .unwrap()
                .replace(&b.id, "repo0000000a")
                .replace(&wt_id, "wt00000000b1")
                .replace(&tmp_root, "/Users/me")
        };
        std::fs::write(
            PathBuf::from(made["worktree"]["path"].as_str().unwrap()).join("new.txt"),
            "x",
        )
        .unwrap();
        let listed = list(&b.base, &b.id).await.unwrap();
        for (name, v) in [
            ("worktree_create.json", &made),
            ("worktree_list.json", &listed),
        ] {
            let text = pin(v);
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/mobile/test/fixtures")
                .join(name);
            if std::env::var_os("ATLAS_UPDATE_FIXTURES").is_some() {
                std::fs::write(&fixture, format!("{text}\n")).unwrap();
            }
            let want: Value =
                serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap_or_default())
                    .unwrap_or(Value::Null);
            assert_eq!(
                want,
                serde_json::from_str::<Value>(&text).unwrap(),
                "{name} changed shape: rerun with ATLAS_UPDATE_FIXTURES=1 and update the phone"
            );
        }
    }

    /// The desktop calls these same functions (`worktree/create.rs`), so one layout rules both: the settings
    /// the desktop's Settings writes decide the folder, and the hook env keeps the names `atlas.yaml` hooks use.
    #[test]
    fn layout_settings_and_hook_env_are_the_ones_the_desktop_uses() {
        let none = serde_json::Map::new();
        assert_eq!(
            workspace_dir(&none),
            atlas_core::paths::atlas_home().join("worktrees")
        );
        let mut s = serde_json::Map::new();
        s.insert("workspaceDir".into(), json!("/w"));
        s.insert("empty".into(), json!(""));
        assert_eq!(workspace_dir(&s), PathBuf::from("/w"));
        assert_eq!(setting(&s, "workspaceDir").as_deref(), Some("/w"));
        assert_eq!(setting(&s, "empty"), None, "blank means unset");
        let env = hook_env(Path::new("/r"), Path::new("/w/x"), "x", 300);
        assert_eq!(
            env.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "ATLAS_PORT_OFFSET",
                "ATLAS_ROOT_PATH",
                "ATLAS_WORKSPACE_NAME",
                "ATLAS_WORKTREE_PATH"
            ]
        );
        assert_eq!(env["ATLAS_PORT_OFFSET"], "300");
    }
}
