//! Review and ship from a phone: status, diff, stage, commit, push, pull request.
//!
//! A FIXED surface, never a free-form `git` argv: every command here builds its own argument
//! list, and the only caller-controlled strings are file paths (validated inside the checkout,
//! passed after `--`), a commit message (on stdin), a PR title/body/base (one argv word each).
//! `root` must be a project of the desktop registry or one of its worktrees (both are entries
//! of `projects.json`), compared after canonicalization so a symlink or `..` cannot step out.
//! hostd does the work itself, so it all works with the desktop closed; the parsers and the AI
//! commit-message logic are shared with the desktop through [`atlas_core::git_review`].

use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use atlas_core::git_review::{self as review, FileDiff, Hunk, Line};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

const GIT_TIMEOUT: Duration = Duration::from_secs(60);
/// Pushing and `gh` talk to the network.
const NET_TIMEOUT: Duration = Duration::from_secs(120);
/// A diff answer is cut here: the phone draws it, it does not need a megabyte patch.
const DIFF_CAP: usize = 256 * 1024;
const MESSAGE_CAP: usize = 16 * 1024;

/// What `git`/`gh` printed.
struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `root` as a registered project or worktree checkout, canonical. Anything else is refused:
/// the phone never names a folder hostd was not told about.
pub(crate) fn resolve_root(data_dir: &Path, root: &str) -> Result<PathBuf, String> {
    let want = Path::new(root)
        .canonicalize()
        .map_err(|_| "unknown project".to_string())?;
    for e in crate::projects::entries(data_dir)? {
        if Path::new(&e.root).canonicalize().is_ok_and(|r| r == want) {
            return Ok(want);
        }
    }
    Err("unknown project".into())
}

/// Paths of a stage/unstage/diff request: relative, inside the checkout, no option lookalikes.
fn check_paths(root: &Path, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no paths".into());
    }
    for p in paths {
        let rel = Path::new(p);
        let bad = p.is_empty()
            || p.contains('\0')
            || p.starts_with('-')
            || rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
        if bad {
            return Err(format!("path outside the project: {p}"));
        }
        // A symlink inside the checkout pointing out is the one way a clean relative path escapes.
        let joined = root.join(rel);
        let mut probe = joined.as_path();
        while !probe.exists() {
            match probe.parent() {
                Some(up) => probe = up,
                None => break,
            }
        }
        if probe.canonicalize().is_ok_and(|c| !c.starts_with(root)) {
            return Err(format!("path outside the project: {p}"));
        }
    }
    Ok(())
}

async fn run(
    bin: &str,
    root: &Path,
    args: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
) -> Result<Out, String> {
    let mut cmd = tokio::process::Command::new(bin);
    atlas_core::sys::CommandExt::no_window(&mut cmd)
        .args(args)
        .current_dir(root)
        .envs(atlas_core::login_env::vars())
        // A credential prompt would hang until the timeout: fail with git's own message instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("{bin}: {e}"))?;
    if let (Some(text), Some(mut w)) = (stdin, child.stdin.take()) {
        let text = text.to_string();
        tokio::spawn(async move {
            let _ = w.write_all(text.as_bytes()).await;
        });
    }
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("{bin} {} timed out", args.first().copied().unwrap_or("")))?
        .map_err(|e| format!("{bin}: {e}"))?;
    Ok(Out {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// `git <args>`; a non-zero exit is git's stderr as the error.
async fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let o = run("git", root, args, None, GIT_TIMEOUT).await?;
    if o.code == Some(0) {
        Ok(o.stdout)
    } else {
        Err(o.stderr.trim().to_string())
    }
}

pub async fn status(data_dir: &Path, root: &str) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    let out = git(&root, &review::STATUS_ARGS).await?;
    let (branch, upstream, ab, entries) = review::parse_status(&out);
    // Ignored paths are noise on a phone.
    let entries: Vec<Value> = entries.into_iter().filter(|e| e["x"] != "!").collect();
    Ok(json!({"branch": branch, "upstream": upstream, "ab": ab, "entries": entries}))
}

/// One file's change as an all-added diff (untracked files have no `git diff`).
fn untracked(root: &Path, rel: &str) -> FileDiff {
    let content = std::fs::metadata(root.join(rel))
        .ok()
        .filter(|m| m.is_file() && m.len() <= review::UNTRACKED_MAX)
        .and_then(|_| std::fs::read(root.join(rel)).ok())
        .and_then(|b| String::from_utf8(b).ok());
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
        hunks: if n > 0 {
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

/// Parsed diff: `staged` = the index vs HEAD, else the working tree vs the index plus untracked
/// files. `path` narrows it to one file. Cut at [`DIFF_CAP`] bytes of patch (`truncated`).
pub async fn diff(
    data_dir: &Path,
    root: &str,
    path: Option<String>,
    staged: bool,
) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    if let Some(p) = &path {
        check_paths(&root, std::slice::from_ref(p))?;
    }
    let mut args = review::REVIEW_DIFF.to_vec();
    if staged {
        args.push("--cached");
    }
    if let Some(p) = &path {
        args.extend(["--", p.as_str()]);
    }
    let mut text = git(&root, &args).await?;
    let truncated = text.len() > DIFF_CAP;
    if truncated {
        // Back to a char boundary, then to the last whole line so the parser sees no torn hunk.
        let mut end = DIFF_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        end = text[..end].rfind('\n').map_or(0, |i| i + 1);
        text.truncate(end);
    }
    let mut files = review::parse(&text, staged);
    if !staged && !truncated {
        let mut ls = vec!["ls-files", "--others", "--exclude-standard", "-z"];
        if let Some(p) = &path {
            ls.extend(["--", p.as_str()]);
        }
        for rel in git(&root, &ls).await?.split('\0').filter(|s| !s.is_empty()) {
            files.push(untracked(&root, rel));
        }
    }
    Ok(json!({"files": files, "truncated": truncated}))
}

pub async fn stage(data_dir: &Path, root: &str, paths: &[String]) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    check_paths(&root, paths)?;
    let mut args = vec!["add", "--"];
    args.extend(paths.iter().map(String::as_str));
    git(&root, &args).await.map(|_| json!({}))
}

pub async fn unstage(data_dir: &Path, root: &str, paths: &[String]) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    check_paths(&root, paths)?;
    // `restore --staged` fails on a repo with no commit yet; `rm --cached` is the first-commit form.
    let has_head = git(&root, &["rev-parse", "--verify", "-q", "HEAD"])
        .await
        .is_ok();
    let mut args = if has_head {
        vec!["restore", "--staged", "--"]
    } else {
        vec!["rm", "--cached", "-r", "-q", "--"]
    };
    args.extend(paths.iter().map(String::as_str));
    git(&root, &args).await.map(|_| json!({}))
}

pub async fn commit(data_dir: &Path, root: &str, message: &str) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    if message.trim().is_empty() {
        return Err("empty commit message".into());
    }
    if message.len() > MESSAGE_CAP {
        return Err("commit message too long".into());
    }
    // The message goes on stdin: no quoting, nothing the message says can become an option.
    let o = run(
        "git",
        &root,
        &["commit", "-F", "-"],
        Some(message),
        GIT_TIMEOUT,
    )
    .await?;
    if o.code != Some(0) {
        // git writes "nothing to commit" to stdout.
        let why = if o.stderr.trim().is_empty() {
            &o.stdout
        } else {
            &o.stderr
        };
        return Err(why.trim().to_string());
    }
    let sha = git(&root, &["rev-parse", "--short", "HEAD"]).await?;
    Ok(json!({"sha": sha.trim()}))
}

/// Push the current branch. A branch with no upstream is pushed to `origin` and tracked, which
/// is what the first push of a feature branch needs; never a force, never another branch.
pub async fn push(data_dir: &Path, root: &str) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    let branch = git(&root, &["branch", "--show-current"]).await?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("detached HEAD: there is no branch to push".into());
    }
    let has_upstream = git(
        &root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await
    .is_ok();
    let args: Vec<&str> = if has_upstream {
        vec!["push"]
    } else {
        vec!["push", "--set-upstream", "origin", branch]
    };
    let o = run("git", &root, &args, None, NET_TIMEOUT).await?;
    if o.code != Some(0) {
        return Err(o.stderr.trim().to_string());
    }
    Ok(json!({"branch": branch, "output": o.stderr.trim()}))
}

/// `gh pr create`, as the desktop does. Returns `{url, number}`; the caller pushes first.
pub async fn pr_create(
    data_dir: &Path,
    root: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    if title.trim().is_empty() {
        return Err("empty title".into());
    }
    let base = base.map(str::trim).filter(|b| !b.is_empty());
    if let Some(b) = base
        && (b.starts_with('-') || b.contains(char::is_whitespace))
    {
        return Err(format!("bad base branch: {b}"));
    }
    let mut args = vec!["pr", "create", "--title", title, "--body", body];
    if let Some(b) = base {
        args.extend(["--base", b]);
    }
    let o = run("gh", &root, &args, None, NET_TIMEOUT).await?;
    if o.code != Some(0) {
        return Err(o.stderr.trim().to_string());
    }
    let url = o.stdout.trim().lines().last().unwrap_or("").to_string();
    let number = url
        .rsplit_once("/pull/")
        .and_then(|(_, n)| n.parse::<u64>().ok());
    Ok(json!({"url": url, "number": number}))
}

/// The staged diff, or every uncommitted change when nothing is staged: `(diff, scope)`.
async fn collect_diff(root: &Path) -> Result<(String, &'static str), String> {
    let staged = git(root, &review::STAGED_CHANGES).await?;
    if !staged.trim().is_empty() {
        return Ok((staged, "staged"));
    }
    let mut all = git(root, &review::ALL_CHANGES).await.unwrap_or_default();
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])
        .await
        .unwrap_or_default();
    for path in untracked.split('\0').filter(|p| !p.is_empty()) {
        // `--no-index` exits 1 when the files differ: the normal case, so not through `git()`.
        let o = run("git", root, &review::new_file_args(path), None, GIT_TIMEOUT).await?;
        if matches!(o.code, Some(0) | Some(1)) {
            all.push_str(&o.stdout);
        }
    }
    Ok((all, "all"))
}

/// What Settings → Git chose (`gitCommitAgent`, `defaultAgent` of the desktop's settings.json)
/// and the agent CLIs found on the login PATH.
fn provider(data_dir: &Path) -> review::Provider {
    let settings: Value = std::fs::read_to_string(data_dir.join("desktop").join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let get = |k: &str| settings.get(k).and_then(Value::as_str);
    let installed: Vec<&str> = review::provider_ids()
        .filter(|a| {
            atlas_core::agents::get(a)
                .is_some_and(|d| atlas_core::login_env::which(&d.bin).is_some())
        })
        .collect();
    review::resolve_provider(get("gitCommitAgent"), get("defaultAgent"), &installed)
}

/// An AI commit message for the staged changes (or everything when nothing is staged), written
/// by the agent CLI the desktop would use. Nothing is committed.
pub async fn commit_message(data_dir: &Path, root: &str) -> Result<Value, String> {
    let root = resolve_root(data_dir, root)?;
    let agent = match provider(data_dir) {
        review::Provider::Agent(a) => a,
        review::Provider::Off => {
            return Err("AI commit messages are turned off in Settings → Git".into());
        }
        review::Provider::None => return Err("no_provider".into()),
    };
    let (diff, scope) = collect_diff(&root).await?;
    if diff.trim().is_empty() {
        return Err("Nothing to describe: no staged or uncommitted changes".into());
    }
    let d = review::digest(&diff, scope);
    if d.text.trim().is_empty() && d.summarized.is_empty() {
        return Err(
            "Every changed file looks like a secret, so none was sent. Write the message yourself."
                .into(),
        );
    }
    let docs: Vec<(&str, String)> = ["CONTRIBUTING.md", "AGENTS.md"]
        .into_iter()
        .filter_map(|n| std::fs::read_to_string(root.join(n)).ok().map(|t| (n, t)))
        .collect();
    let recent: Vec<String> = git(&root, &["log", "-n", "12", "--no-merges", "--format=%s"])
        .await
        .map(|o| o.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let prompt = review::build_prompt(&review::commit_rules(&docs), &recent, "", &d);
    let mut argv: Vec<String> = review::provider_argv(agent).expect("known agent").to_vec();
    if let Some(bin) =
        atlas_core::agents::get(agent).and_then(|a| atlas_core::login_env::which(&a.bin))
    {
        argv[0] = bin.to_string_lossy().into_owned();
    }
    // The sender stays alive for the whole run: dropping it would cancel the provider.
    let (_keep, rx) = tokio::sync::oneshot::channel::<()>();
    let last = review::run_provider(
        &argv,
        &root,
        &atlas_core::login_env::vars(),
        &prompt,
        rx,
        |_| {},
    )
    .await;
    match last {
        review::Event::Done { message } => Ok(json!({
            "message": message,
            "agent": agent,
            "scope": scope,
            "withheld": d.withheld.len(),
        })),
        review::Event::Error { message } => Err(message),
        _ => Err("cancelled".into()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
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

    /// A registry with one project: a real repo with a commit, whose `origin` is a real bare repo.
    struct Rig {
        data: tempfile::TempDir,
        work: tempfile::TempDir,
        remote: tempfile::TempDir,
    }

    impl Rig {
        fn new() -> Rig {
            let data = tempfile::tempdir().unwrap();
            let work = tempfile::tempdir().unwrap();
            let remote = tempfile::tempdir().unwrap();
            sh(remote.path(), &["init", "-q", "--bare", "."]);
            sh(work.path(), &["init", "-q", "."]);
            // hostd commits with the repo's own config: CI runners have no global identity.
            for (k, v) in [
                ("user.email", "a@b"),
                ("user.name", "n"),
                ("commit.gpgsign", "false"),
            ] {
                sh(work.path(), &["config", k, v]);
            }
            std::fs::write(work.path().join("a.txt"), "one\ntwo\n").unwrap();
            sh(work.path(), &["add", "."]);
            sh(work.path(), &["commit", "-qm", "init"]);
            sh(
                work.path(),
                &["remote", "add", "origin", remote.path().to_str().unwrap()],
            );
            std::fs::create_dir_all(data.path().join("desktop")).unwrap();
            std::fs::write(
                data.path().join("desktop/projects.json"),
                json!([{"id": "p", "root": work.path()}]).to_string(),
            )
            .unwrap();
            Rig { data, work, remote }
        }
        fn root(&self) -> &str {
            self.work.path().to_str().unwrap()
        }
        fn d(&self) -> &Path {
            self.data.path()
        }
    }

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn status_and_diff_follow_the_working_tree() {
        let r = Rig::new();
        std::fs::write(r.work.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(r.work.path().join("new.txt"), "fresh\n").unwrap();
        let st = status(r.d(), r.root()).await.unwrap();
        assert_eq!(st["branch"], "main");
        let by = |p: &str| {
            st["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["path"] == p)
                .cloned()
        };
        assert_eq!(by("a.txt").unwrap()["y"], "M");
        assert_eq!(by("new.txt").unwrap()["x"], "?");
        // Ignored paths do not reach a phone.
        std::fs::write(r.work.path().join(".gitignore"), "junk/\n").unwrap();
        std::fs::create_dir_all(r.work.path().join("junk")).unwrap();
        std::fs::write(r.work.path().join("junk/x"), "").unwrap();
        let st = status(r.d(), r.root()).await.unwrap();
        assert!(
            st["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["x"] != "!"),
            "{st}"
        );

        let d = diff(r.d(), r.root(), None, false).await.unwrap();
        let files = d["files"].as_array().unwrap();
        let a = files.iter().find(|f| f["path"] == "a.txt").unwrap();
        assert_eq!(a["status"], "modified");
        assert_eq!(a["staged"], false);
        let new = files.iter().find(|f| f["path"] == "new.txt").unwrap();
        assert_eq!(new["status"], "untracked");
        assert_eq!(new["hunks"][0]["lines"][0]["text"], "fresh");
        // Nothing staged yet.
        let s = diff(r.d(), r.root(), None, true).await.unwrap();
        assert_eq!(s["files"], json!([]));
        // `path` narrows it.
        let one = diff(r.d(), r.root(), Some("a.txt".into()), false)
            .await
            .unwrap();
        assert_eq!(one["files"].as_array().unwrap().len(), 1);
    }

    // Regression (#414): with `diff.mnemonicPrefix` (`c/ w/ i/`) or `diff.noprefix` in the user's git config, the
    // Changes view named files wrong and the commit-message digest did not withhold a `.env`. Real hostd functions,
    // the setting in the repo's own config.
    #[tokio::test]
    async fn diff_and_commit_digest_hold_whatever_prefix_the_users_git_config_asks_for() {
        for (key, value) in [("diff.mnemonicPrefix", "true"), ("diff.noprefix", "true")] {
            let r = Rig::new();
            sh(r.work.path(), &["config", key, value]);
            std::fs::write(r.work.path().join("a.txt"), "one\nTWO\n").unwrap();
            std::fs::write(r.work.path().join(".env"), "TOKEN=untracked-secret\n").unwrap();

            // The Changes view.
            let d = diff(r.d(), r.root(), None, false).await.unwrap();
            let a = d["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["path"] == "a.txt")
                .unwrap_or_else(|| panic!("{key}: {d}"));
            assert_eq!(a["status"], "modified", "{key}");

            // Nothing staged: tracked changes plus the untracked `.env` through `--no-index`.
            let (text, scope) = collect_diff(r.work.path()).await.unwrap();
            assert_eq!(scope, "all");
            let dg = review::digest(&text, scope);
            assert_eq!(dg.withheld, [".env"], "{key}: {text}");
            assert!(dg.text.contains("+TWO"), "{key}: {}", dg.text);
            assert!(!dg.text.contains("untracked-secret"), "{key}: {}", dg.text);

            // Staged.
            sh(r.work.path(), &["add", ".env"]);
            let (text, scope) = collect_diff(r.work.path()).await.unwrap();
            assert_eq!(scope, "staged");
            let dg = review::digest(&text, scope);
            assert_eq!(dg.withheld, [".env"], "{key}: {text}");
            assert!(!dg.text.contains("untracked-secret"), "{key}: {}", dg.text);
        }
    }

    #[tokio::test]
    async fn stage_then_unstage_moves_a_file_through_the_index() {
        let r = Rig::new();
        std::fs::write(r.work.path().join("a.txt"), "changed\n").unwrap();
        stage(r.d(), r.root(), &paths(&["a.txt"])).await.unwrap();
        let s = diff(r.d(), r.root(), None, true).await.unwrap();
        assert_eq!(s["files"][0]["path"], "a.txt");
        assert_eq!(s["files"][0]["staged"], true);
        unstage(r.d(), r.root(), &paths(&["a.txt"])).await.unwrap();
        assert_eq!(
            diff(r.d(), r.root(), None, true).await.unwrap()["files"],
            json!([])
        );
        // The change itself is still there.
        assert_eq!(
            std::fs::read_to_string(r.work.path().join("a.txt")).unwrap(),
            "changed\n"
        );
    }

    #[tokio::test]
    async fn unstage_works_before_the_first_commit() {
        let data = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        sh(work.path(), &["init", "-q", "."]);
        std::fs::create_dir_all(data.path().join("desktop")).unwrap();
        std::fs::write(
            data.path().join("desktop/projects.json"),
            json!([{"id": "p", "root": work.path()}]).to_string(),
        )
        .unwrap();
        std::fs::write(work.path().join("f.txt"), "x\n").unwrap();
        let root = work.path().to_str().unwrap();
        stage(data.path(), root, &paths(&["f.txt"])).await.unwrap();
        unstage(data.path(), root, &paths(&["f.txt"]))
            .await
            .unwrap();
        assert_eq!(
            diff(data.path(), root, None, true).await.unwrap()["files"],
            json!([])
        );
    }

    // The whole point: review on the phone, ship from the phone. Real repo, real bare remote.
    #[tokio::test]
    async fn stage_commit_push_lands_on_the_remote() {
        let r = Rig::new();
        sh(r.work.path(), &["checkout", "-qb", "feat/x"]);
        std::fs::write(r.work.path().join("a.txt"), "shipped\n").unwrap();
        stage(r.d(), r.root(), &paths(&["a.txt"])).await.unwrap();
        // The message has quotes, a leading dash and a body: it must arrive verbatim.
        let msg = "-fix: \"quote\" `tick` $(x)\n\nbody line";
        let c = commit(r.d(), r.root(), msg).await.unwrap();
        assert!(!c["sha"].as_str().unwrap().is_empty());
        assert_eq!(sh(r.work.path(), &["log", "-1", "--format=%B"]).trim(), msg);
        // First push of the branch: upstream is set to origin.
        let p = push(r.d(), r.root()).await.unwrap();
        assert_eq!(p["branch"], "feat/x");
        let remote_head = sh(r.remote.path(), &["rev-parse", "refs/heads/feat/x"]);
        assert_eq!(
            remote_head.trim(),
            sh(r.work.path(), &["rev-parse", "HEAD"]).trim()
        );
        // A second commit goes out with the plain form.
        std::fs::write(r.work.path().join("a.txt"), "again\n").unwrap();
        stage(r.d(), r.root(), &paths(&["a.txt"])).await.unwrap();
        commit(r.d(), r.root(), "second").await.unwrap();
        push(r.d(), r.root()).await.unwrap();
        assert_eq!(
            sh(r.remote.path(), &["rev-parse", "refs/heads/feat/x"]).trim(),
            sh(r.work.path(), &["rev-parse", "HEAD"]).trim()
        );
        let st = status(r.d(), r.root()).await.unwrap();
        assert_eq!(st["upstream"], "origin/feat/x");
    }

    #[tokio::test]
    async fn commit_refuses_empty_messages_and_nothing_staged_with_gits_reason() {
        let r = Rig::new();
        assert_eq!(
            commit(r.d(), r.root(), "  \n").await.unwrap_err(),
            "empty commit message"
        );
        let e = commit(r.d(), r.root(), "x").await.unwrap_err();
        assert!(
            e.contains("nothing to commit") || e.contains("nothing added"),
            "{e}"
        );
    }

    #[tokio::test]
    async fn push_refuses_a_detached_head() {
        let r = Rig::new();
        sh(r.work.path(), &["checkout", "-q", "--detach"]);
        assert!(
            push(r.d(), r.root())
                .await
                .unwrap_err()
                .contains("detached")
        );
    }

    // A root outside the registry is refused for every command, even a real git repo, a symlink to a
    // registered one is fine (same checkout), and a path that merely starts with a project's path is not.
    #[tokio::test]
    async fn a_root_outside_the_projects_is_refused_everywhere() {
        let r = Rig::new();
        let other = tempfile::tempdir().unwrap();
        sh(other.path(), &["init", "-q", "."]);
        let o = other.path().to_str().unwrap();
        let no = "unknown project";
        assert_eq!(status(r.d(), o).await.unwrap_err(), no);
        assert_eq!(diff(r.d(), o, None, false).await.unwrap_err(), no);
        assert_eq!(stage(r.d(), o, &paths(&["a"])).await.unwrap_err(), no);
        assert_eq!(unstage(r.d(), o, &paths(&["a"])).await.unwrap_err(), no);
        assert_eq!(commit(r.d(), o, "x").await.unwrap_err(), no);
        assert_eq!(commit_message(r.d(), o).await.unwrap_err(), no);
        assert_eq!(push(r.d(), o).await.unwrap_err(), no);
        assert_eq!(pr_create(r.d(), o, "t", "b", None).await.unwrap_err(), no);
        // A subfolder of a project is not a project root.
        std::fs::create_dir_all(r.work.path().join("sub")).unwrap();
        let sub = r.work.path().join("sub");
        assert_eq!(status(r.d(), sub.to_str().unwrap()).await.unwrap_err(), no);
        // `..` out of the project to another registered-looking path does not resolve either.
        let dotdot = format!("{}/sub/../..", r.root());
        assert_eq!(status(r.d(), &dotdot).await.unwrap_err(), no);
        // A relative or missing root.
        assert_eq!(status(r.d(), "relative").await.unwrap_err(), no);
        assert_eq!(status(r.d(), "/does/not/exist").await.unwrap_err(), no);
        // The symlink to the real project resolves to it.
        let link = other.path().join("link");
        std::os::unix::fs::symlink(r.work.path(), &link).unwrap();
        assert!(status(r.d(), link.to_str().unwrap()).await.is_ok());
    }

    #[tokio::test]
    async fn a_worktree_registered_as_a_project_is_a_valid_root() {
        let r = Rig::new();
        let wt = tempfile::tempdir().unwrap();
        let wtp = wt.path().join("wt");
        sh(
            r.work.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt-branch",
                wtp.to_str().unwrap(),
            ],
        );
        std::fs::write(
            r.d().join("desktop/projects.json"),
            json!([{"id": "p", "root": r.work.path()}, {"id": "w", "root": wtp, "parent": "p"}])
                .to_string(),
        )
        .unwrap();
        std::fs::write(wtp.join("w.txt"), "w\n").unwrap();
        let st = status(r.d(), wtp.to_str().unwrap()).await.unwrap();
        assert_eq!(st["branch"], "wt-branch");
    }

    #[tokio::test]
    async fn paths_outside_the_checkout_are_refused() {
        let r = Rig::new();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), r.work.path().join("escape")).unwrap();
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "-rf",
            "--force",
            "",
            "a\0b",
            "escape/secret",
            "escape",
        ] {
            let e = stage(r.d(), r.root(), &paths(&[bad])).await.unwrap_err();
            assert!(e.starts_with("path outside the project"), "{bad:?}: {e}");
            assert!(
                unstage(r.d(), r.root(), &paths(&[bad])).await.is_err(),
                "{bad:?}"
            );
            assert!(
                diff(r.d(), r.root(), Some(bad.into()), false)
                    .await
                    .is_err(),
                "{bad:?}"
            );
        }
        assert_eq!(stage(r.d(), r.root(), &[]).await.unwrap_err(), "no paths");
        // One bad path refuses the whole request: nothing was staged.
        std::fs::write(r.work.path().join("ok.txt"), "x").unwrap();
        assert!(
            stage(r.d(), r.root(), &paths(&["ok.txt", "../x"]))
                .await
                .is_err()
        );
        assert_eq!(
            diff(r.d(), r.root(), None, true).await.unwrap()["files"],
            json!([])
        );
        // A dotted or spaced name inside the checkout is fine.
        std::fs::write(r.work.path().join("my file.txt"), "x").unwrap();
        stage(r.d(), r.root(), &paths(&["my file.txt", "./ok.txt"]))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_huge_diff_is_cut_on_a_line_and_flagged() {
        let r = Rig::new();
        let big: String = (0..40_000)
            .map(|i| format!("line {i} {}\n", "x".repeat(20)))
            .collect();
        std::fs::write(r.work.path().join("a.txt"), &big).unwrap();
        let d = diff(r.d(), r.root(), None, false).await.unwrap();
        assert_eq!(d["truncated"], true);
        let lines = d["files"][0]["hunks"][0]["lines"].as_array().unwrap();
        assert!(!lines.is_empty() && lines.len() < 40_000);
        // The last kept line is whole.
        assert!(
            lines.last().unwrap()["text"]
                .as_str()
                .unwrap()
                .ends_with(&"x".repeat(20))
        );
    }

    #[tokio::test]
    async fn commit_message_says_why_when_no_agent_is_set_up() {
        let r = Rig::new();
        std::fs::write(
            r.d().join("desktop/settings.json"),
            json!({"gitCommitAgent": "off"}).to_string(),
        )
        .unwrap();
        std::fs::write(r.work.path().join("a.txt"), "x\n").unwrap();
        let e = commit_message(r.d(), r.root()).await.unwrap_err();
        assert!(e.contains("turned off"), "{e}");
    }

    /// The phone parses these payloads (`apps/mobile/test/git_changes_test.dart`): real serializer
    /// output of a real repo. `ATLAS_UPDATE_FIXTURES=1` rewrites them.
    #[tokio::test]
    async fn payloads_match_the_phone_fixtures() {
        let r = Rig::new();
        std::fs::write(r.work.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(r.work.path().join("new.txt"), "fresh\n").unwrap();
        stage(r.d(), r.root(), &paths(&["a.txt"])).await.unwrap();
        std::fs::write(r.work.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
        let fixtures = [
            ("git_status.json", status(r.d(), r.root()).await.unwrap()),
            (
                "git_diff.json",
                diff(r.d(), r.root(), None, false).await.unwrap(),
            ),
            (
                "git_diff_staged.json",
                diff(r.d(), r.root(), None, true).await.unwrap(),
            ),
            (
                "git_commit.json",
                json!({"sha": commit(r.d(), r.root(), "m").await.unwrap()["sha"].as_str().map(|_| "abc1234")}),
            ),
        ];
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/mobile/test/fixtures");
        for (name, v) in fixtures {
            let path = dir.join(name);
            if std::env::var_os("ATLAS_UPDATE_FIXTURES").is_some() {
                std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
            }
            let want: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                    .unwrap_or(Value::Null);
            assert_eq!(
                want, v,
                "{name} changed shape: rerun with ATLAS_UPDATE_FIXTURES=1 and update the phone"
            );
        }
    }

    #[tokio::test]
    async fn pr_create_validates_before_it_runs_gh() {
        let r = Rig::new();
        assert_eq!(
            pr_create(r.d(), r.root(), " ", "b", None)
                .await
                .unwrap_err(),
            "empty title"
        );
        let e = pr_create(r.d(), r.root(), "t", "b", Some("--web"))
            .await
            .unwrap_err();
        assert!(e.starts_with("bad base branch"), "{e}");
    }
}
