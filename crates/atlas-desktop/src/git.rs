//! Review data for a worktree: parsed diff, status, log. Everything runs
//! through hostd `exec` (same path for SSH hosts).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::State;

use crate::host::HostHub;
use crate::worktree::git;

/// Untracked files larger than this show as binary instead of their content.
const UNTRACKED_MAX: u64 = 512 * 1024;

#[derive(Serialize, Debug, PartialEq)]
pub struct Line {
    kind: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_no: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_no: Option<u32>,
}

#[derive(Serialize, Debug)]
pub struct Hunk {
    header: String,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<Line>,
}

#[derive(Serialize, Debug)]
pub struct FileDiff {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    /// `modified|added|deleted|renamed|untracked`
    status: &'static str,
    staged: bool,
    binary: bool,
    hunks: Vec<Hunk>,
}

fn range(s: &str) -> (u32, u32) {
    let mut it = s.splitn(2, ',');
    let start = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let len = it.next().map_or(1, |n| n.parse().unwrap_or(0));
    (start, len)
}

/// Parse `git diff --no-color -U3 -M` output.
pub fn parse(diff: &str, staged: bool) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let (mut old_no, mut new_no) = (0u32, 0u32);
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("diff --git ") {
            let b = rest.rsplit_once(" b/").map(|(_, b)| b).unwrap_or(rest);
            files.push(FileDiff {
                path: b.to_string(),
                old_path: None,
                status: "modified",
                staged,
                binary: false,
                hunks: Vec::new(),
            });
            continue;
        }
        let Some(f) = files.last_mut() else { continue };
        if f.hunks.is_empty() {
            if l.starts_with("new file mode") {
                f.status = "added";
            } else if l.starts_with("deleted file mode") {
                f.status = "deleted";
            } else if let Some(p) = l.strip_prefix("rename from ") {
                f.status = "renamed";
                f.old_path = Some(p.to_string());
            } else if l.starts_with("Binary files ") || l == "GIT binary patch" {
                f.binary = true;
            }
        }
        if let Some(h) = l.strip_prefix("@@ ") {
            let mut parts = h.split_whitespace();
            let (os, ol) = range(parts.next().unwrap_or("-0").trim_start_matches('-'));
            let (ns, nl) = range(parts.next().unwrap_or("+0").trim_start_matches('+'));
            (old_no, new_no) = (os, ns);
            f.hunks.push(Hunk {
                header: l.to_string(),
                old_start: os,
                old_lines: ol,
                new_start: ns,
                new_lines: nl,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(h) = f.hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match l.as_bytes().first() {
            Some(b'+') => ("add", &l[1..]),
            Some(b'-') => ("del", &l[1..]),
            Some(b' ') => ("ctx", &l[1..]),
            Some(b'\\') => continue,
            None => ("ctx", ""),
            _ => continue,
        };
        let line = Line {
            kind,
            text: text.to_string(),
            old_no: (kind != "add").then_some(old_no),
            new_no: (kind != "del").then_some(new_no),
        };
        if kind != "add" {
            old_no += 1;
        }
        if kind != "del" {
            new_no += 1;
        }
        h.lines.push(line);
    }
    files
}

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
    let args = ["diff", "--no-color", "-U3", "--no-ext-diff", "-M"];
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

async fn default_base(hub: &HostHub, dir: &Path) -> String {
    let branch = git(hub, dir, &["branch", "--show-current"])
        .await
        .unwrap_or_default();
    let branch = branch.trim();
    if !branch.is_empty()
        && let Ok(b) = git(hub, dir, &["config", &format!("branch.{branch}.base")]).await
        && !b.trim().is_empty()
    {
        return b.trim().to_string();
    }
    "HEAD".into()
}

/// Porcelain v2 status: branch, upstream, ahead/behind vs base, entries.
#[tauri::command]
pub async fn git_status(hub: State<'_, Arc<HostHub>>, id: String) -> Result<Value, String> {
    let dir = root(&id)?;
    let out = git(&hub, &dir, &["status", "--porcelain=v2", "-z", "--branch"]).await?;
    let (mut branch, mut upstream, mut ab) = (None, None, None);
    let mut entries = Vec::new();
    let recs: Vec<&str> = out.split('\0').filter(|s| !s.is_empty()).collect();
    let mut it = recs.into_iter();
    while let Some(rec) = it.next() {
        if let Some(h) = rec.strip_prefix("# branch.head ") {
            branch = Some(h.to_string());
        } else if let Some(u) = rec.strip_prefix("# branch.upstream ") {
            upstream = Some(u.to_string());
        } else if let Some(x) = rec.strip_prefix("# branch.ab ") {
            ab = Some(x.to_string());
        } else if let Some(p) = rec.strip_prefix("? ") {
            entries.push(json!({"path": p, "x": "?", "y": "?"}));
        } else if let Some(r) = rec.strip_prefix("1 ").or_else(|| rec.strip_prefix("u ")) {
            let xy = &r[..2];
            let path = r
                .splitn(if rec.starts_with('u') { 10 } else { 8 }, ' ')
                .last()
                .unwrap_or("");
            entries.push(json!({"path": path, "x": &xy[..1], "y": &xy[1..], "conflict": rec.starts_with('u')}));
        } else if let Some(r) = rec.strip_prefix("2 ") {
            let xy = &r[..2];
            let path = r.splitn(9, ' ').last().unwrap_or("");
            let orig = it.next().unwrap_or("");
            entries.push(json!({"path": path, "orig": orig, "x": &xy[..1], "y": &xy[1..]}));
        }
    }
    let base = default_base(&hub, &dir).await;
    let vs_base = if base != "HEAD" {
        git(
            &hub,
            &dir,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{base}"),
            ],
        )
        .await
        .ok()
        .and_then(|s| {
            let mut n = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
            Some(json!({"ahead": n.next()?, "behind": n.next()?}))
        })
    } else {
        None
    };
    let merging = dir.join(".git").is_file() || dir.join(".git").is_dir();
    let gitdir = git(&hub, &dir, &["rev-parse", "--git-dir"])
        .await
        .unwrap_or_default();
    let gd = dir.join(gitdir.trim());
    let op = if !merging {
        None
    } else if gd.join("MERGE_HEAD").exists() {
        Some("merge")
    } else if gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists() {
        Some("rebase")
    } else {
        None
    };
    Ok(json!({
        "branch": branch, "upstream": upstream, "ab": ab, "base": base,
        "vs_base": vs_base, "entries": entries, "operation": op,
    }))
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
    let env: serde_json::Map<String, Value> = crate::shell_env::vars_async()
        .await
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs
index 1..2 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,4 @@ fn main
 one
-two
+TWO
+three
 four
\\ No newline at end of file
diff --git a/old.txt b/new.txt
similarity index 90%
rename from old.txt
rename to new.txt
diff --git a/img.png b/img.png
new file mode 100644
Binary files /dev/null and b/img.png differ
";

    #[test]
    fn parses_hunks_line_numbers_renames_binaries() {
        let f = parse(DIFF, false);
        assert_eq!(f.len(), 3);
        let h = &f[0].hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 4)
        );
        let got: Vec<_> = h
            .lines
            .iter()
            .map(|l| (l.kind, l.old_no, l.new_no))
            .collect();
        assert_eq!(
            got,
            [
                ("ctx", Some(1), Some(1)),
                ("del", Some(2), None),
                ("add", None, Some(2)),
                ("add", None, Some(3)),
                ("ctx", Some(3), Some(4)),
            ]
        );
        assert_eq!(
            (f[1].status, f[1].old_path.as_deref()),
            ("renamed", Some("old.txt"))
        );
        assert_eq!(f[1].path, "new.txt");
        assert!(f[2].binary);
        assert_eq!(f[2].status, "added");
    }
}
