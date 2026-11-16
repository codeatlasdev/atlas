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

/// Untracked files read at once. Each read is a hostd round trip (a network one for an SSH checkout), so reading
/// them one after the other cost `files x latency`; a few in flight hide it without flooding a remote host.
const UNTRACKED_PARALLEL: usize = 8;

/// Each of `rels` as an all-added diff, in `rels` order: a window of `UNTRACKED_PARALLEL` reads that slides as the
/// oldest one finishes. (Written as a loop, not `stream.map(..).buffered(..)`: that closure over borrowed arguments is
/// "not general enough" for rustc inside a `#[tauri::command]` future.)
async fn untracked_all(hub: &HostHub, dir: &Path, rels: &[&str]) -> Vec<FileDiff> {
    use futures_util::StreamExt;
    use futures_util::stream::FuturesOrdered;
    // One host lookup for the whole diff: it reads the project registry from disk.
    let host = crate::host::host_for_path(&dir.to_string_lossy());
    let mut todo = rels.iter();
    let mut window = FuturesOrdered::new();
    for rel in todo.by_ref().take(UNTRACKED_PARALLEL) {
        window.push_back(untracked(hub, &host, dir, rel));
    }
    let mut out = Vec::with_capacity(rels.len());
    while let Some(done) = window.next().await {
        out.push(done);
        if let Some(rel) = todo.next() {
            window.push_back(untracked(hub, &host, dir, rel));
        }
    }
    out
}

/// The file's text when it is at most `UNTRACKED_MAX` bytes of UTF-8: one `fs_read` of one byte more than the cap,
/// so a bigger file shows up as such without a separate `fs_stat` round trip. `None`: too big, unreadable or binary.
async fn read_capped(hub: &HostHub, host: &str, path: &str) -> Option<String> {
    use base64::Engine;
    let r = hub
        .call(
            "fs_read",
            json!({"host": host, "path": path, "max": UNTRACKED_MAX + 1}),
        )
        .await
        .ok()?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(r["data"].as_str()?)
        .ok()?;
    if bytes.len() as u64 > UNTRACKED_MAX {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// An untracked file as an all-added diff; read through hostd so SSH checkouts work.
async fn untracked(hub: &HostHub, host: &str, dir: &Path, rel: &str) -> FileDiff {
    let p = dir.join(rel).to_string_lossy().into_owned();
    let content = read_capped(hub, host, &p).await;
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
        let rels: Vec<&str> = list.split('\0').filter(|s| !s.is_empty()).collect();
        files.extend(untracked_all(&hub, &dir, &rels).await);
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

/// Tracked + untracked (not ignored) files, for Quick Open.
#[tauri::command]
pub async fn git_files(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Vec<String>, String> {
    let dir = root(&id)?;
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

#[cfg(test)]
mod untracked_tests {
    use super::*;
    use atlas_host::HostClient;
    use base64::Engine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
    const FILES: usize = 300;

    /// 300 untracked files: mostly small text, plus every shape the diff treats differently, at known places.
    /// Returns the paths in the order `git ls-files --others` would list them.
    fn fixture(dir: &Path) -> Vec<String> {
        let mut rels = Vec::new();
        for i in 0..FILES {
            let (rel, bytes): (String, Vec<u8>) = match i {
                10 => ("empty.txt".into(), Vec::new()),
                20 => (
                    "over cap ünï.bin".into(),
                    vec![b'a'; UNTRACKED_MAX as usize + 1],
                ),
                30 => ("at cap.txt".into(), vec![b'b'; UNTRACKED_MAX as usize]),
                40 => ("not utf8.dat".into(), vec![0xff, 0xfe, b'\n', 0x80]),
                50 => ("vanished.txt".into(), Vec::new()),
                _ => (
                    format!("dir{}/file{i}.txt", i % 7),
                    format!("first {i}\nsecond\nno final newline").into_bytes(),
                ),
            };
            let p = dir.join(&rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            if i == 50 {
                // listed by `ls-files`, gone by the time it is read
            } else {
                std::fs::write(&p, bytes).unwrap();
            }
            rels.push(rel);
        }
        rels
    }

    /// Counts what a fake hostd is asked and how many requests were open at once.
    #[derive(Default)]
    struct Probe {
        stats: AtomicUsize,
        reads: AtomicUsize,
        open: AtomicUsize,
        peak: AtomicUsize,
    }

    /// A hostd that serves `fs_stat` and `fs_read` from the real disk in hostd's reply shapes, answers each request
    /// after `latency` (the network round trip of an SSH checkout) and never waits for one request to answer the next.
    async fn slow_hostd(latency: Duration) -> (tempfile::TempDir, HostClient, Arc<Probe>) {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = atlas_core::ipc::Listener::bind(&sock).unwrap();
        let probe = Arc::new(Probe::default());
        let p = probe.clone();
        tokio::spawn(async move {
            let (r, w) = atlas_core::ipc::split(listener.accept().await.unwrap());
            let w = Arc::new(tokio::sync::Mutex::new(w));
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.lock().await.write_all(b"{\"ok\":true}\n").await.unwrap();
            while let Ok(Some(line)) = r.next_line().await {
                let (p, w) = (p.clone(), w.clone());
                tokio::spawn(async move {
                    let v: Value = serde_json::from_str(&line).unwrap();
                    let path = v["args"]["path"].as_str().unwrap_or("").to_string();
                    let now = p.open.fetch_add(1, Ordering::SeqCst) + 1;
                    p.peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(latency).await;
                    let reply = match v["cmd"].as_str() {
                        Some("fs_stat") => {
                            p.stats.fetch_add(1, Ordering::SeqCst);
                            match std::fs::symlink_metadata(&path) {
                                Ok(m) => json!({"kind": "file", "size": m.len(), "mtime": 0}),
                                Err(e) => json!({"__err": e.to_string()}),
                            }
                        }
                        Some("fs_read") => {
                            use std::io::Read;
                            p.reads.fetch_add(1, Ordering::SeqCst);
                            let max = v["args"]["max"].as_u64().unwrap_or(4 * 1024 * 1024);
                            match std::fs::File::open(&path) {
                                Ok(f) => {
                                    let len = f.metadata().unwrap().len();
                                    let mut data = Vec::new();
                                    f.take(max).read_to_end(&mut data).unwrap();
                                    json!({"data": B64.encode(&data), "truncated": len > data.len() as u64, "offset": 0})
                                }
                                Err(e) => json!({"__err": e.to_string()}),
                            }
                        }
                        _ => json!({"__err": "unsupported"}),
                    };
                    p.open.fetch_sub(1, Ordering::SeqCst);
                    let out = match reply["__err"].as_str() {
                        Some(e) => json!({"id": v["id"], "ok": false, "error": e}),
                        None => json!({"id": v["id"], "ok": true, "data": reply}),
                    };
                    let _ = w
                        .lock()
                        .await
                        .write_all(format!("{out}\n").as_bytes())
                        .await;
                });
            }
        });
        let c = HostClient::connect(&sock, "t", "test").await.unwrap();
        (tmp, c, probe)
    }

    /// The real hostd, in this process, on a temp dir (its registry too: never the user's `~/.atlas`).
    async fn real_hostd() -> (tempfile::TempDir, HostClient) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let registry = tmp.path().join("registry");
        std::fs::create_dir_all(&registry).unwrap();
        // One update check on file, so the daemon does not go to the network for one.
        let followed = atlas_core::channel::effective(
            std::env::var(atlas_core::channel::ENV).ok().as_deref(),
            atlas_core::channel::Channel::default(),
        );
        std::fs::write(
            registry.join(atlas_core::update_cache::FILE),
            json!({"last_checked": 1_700_000_000u64, "available_version": null, "channel": followed})
                .to_string(),
        )
        .unwrap();
        let (d, opts) = (
            dir.clone(),
            atlas_host::server::ServeOpts {
                registry: Some(registry),
                ..Default::default()
            },
        );
        tokio::spawn(async move { atlas_host::server::serve_with(&d, opts).await });
        let client = loop {
            if let Ok(c) = HostClient::connect_dir(&dir, "test").await {
                break c;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        (tmp, client)
    }

    async fn hub_with(c: HostClient) -> HostHub {
        let hub = HostHub::default();
        hub.set(crate::host::LOCAL, Some(c)).await;
        hub
    }

    fn lines_of(f: &FileDiff) -> Vec<String> {
        f.hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| l.text.clone()))
            .collect()
    }

    /// What the Changes view shows for the fixture, whichever hostd answered.
    fn assert_diffs(rels: &[String], out: &[FileDiff]) {
        assert_eq!(out.len(), rels.len());
        for (rel, f) in rels.iter().zip(out) {
            assert_eq!(&f.path, rel, "the order of `ls-files` is kept");
            assert_eq!(
                (f.status, f.staged, f.old_path.as_deref()),
                ("untracked", false, None)
            );
            match rel.as_str() {
                "empty.txt" | "at cap.txt" | "vanished.txt" | "over cap ünï.bin"
                | "not utf8.dat" => {}
                _ => {
                    assert!(!f.binary, "{rel}");
                    assert_eq!(f.hunks.len(), 1, "{rel}");
                    assert_eq!(f.hunks[0].header, "@@ -0,0 +1,3 @@");
                    let n = rel.rsplit_once("file").unwrap().1.trim_end_matches(".txt");
                    assert_eq!(
                        lines_of(f),
                        [&format!("first {n}")[..], "second", "no final newline"]
                    );
                    assert_eq!(f.hunks[0].lines[2].new_no, Some(3));
                }
            }
        }
        let by = |name: &str| out.iter().find(|f| f.path == name).unwrap();
        // Empty: a text file with nothing in it (no hunk, not binary).
        assert!(!by("empty.txt").binary && by("empty.txt").hunks.is_empty());
        // Exactly at the cap is still shown; one byte over is a binary placeholder, as are non-UTF-8 and vanished files.
        assert!(!by("at cap.txt").binary);
        assert_eq!(by("at cap.txt").hunks[0].header, "@@ -0,0 +1,1 @@");
        for name in ["over cap ünï.bin", "not utf8.dat", "vanished.txt"] {
            assert!(by(name).binary && by(name).hunks.is_empty(), "{name}");
        }
    }

    // Regression #310: `git_diff` read untracked files one after the other, `fs_stat` then `fs_read` for each:
    // 2 x N serial round trips before the Changes view painted.
    #[tokio::test]
    async fn untracked_files_are_one_read_each_and_read_in_parallel_without_losing_their_order() {
        let work = tempfile::tempdir().unwrap();
        let rels = fixture(work.path());
        let (_t, c, probe) = slow_hostd(Duration::from_millis(5)).await;
        let hub = hub_with(c).await;
        let list: Vec<&str> = rels.iter().map(String::as_str).collect();

        let t0 = std::time::Instant::now();
        let out = untracked_all(&hub, work.path(), &list).await;
        eprintln!("untracked_all: {} files in {:?}", out.len(), t0.elapsed());

        assert_diffs(&rels, &out);
        assert_eq!(probe.stats.load(Ordering::SeqCst), 0, "no separate stat");
        assert_eq!(
            probe.reads.load(Ordering::SeqCst),
            FILES,
            "one read per file"
        );
        let peak = probe.peak.load(Ordering::SeqCst);
        assert!((2..=8).contains(&peak), "in flight: {peak}, want 2..=8");
    }

    // The same fixture against the real hostd: same answers as the stand-in, so the test above is about hostd's
    // actual `fs_read` (a `max` of cap + 1 and `truncated`), not about the fake.
    #[tokio::test]
    async fn a_real_hostd_gives_the_same_untracked_diffs() {
        let work = tempfile::tempdir().unwrap();
        let rels = fixture(work.path());
        let (_t, c) = real_hostd().await;
        let hub = hub_with(c).await;
        let list: Vec<&str> = rels.iter().map(String::as_str).collect();

        let t0 = std::time::Instant::now();
        let out = untracked_all(&hub, work.path(), &list).await;
        eprintln!(
            "untracked_all (real hostd): {} files in {:?}",
            out.len(),
            t0.elapsed()
        );

        assert_diffs(&rels, &out);
    }
}
