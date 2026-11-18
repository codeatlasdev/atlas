//! Git review for a phone or the desktop: what hostd and the app both need, with no UI or Tauri in it.
//! The parsers turn real `git diff` / `git status --porcelain=v2` output into the shapes the clients draw, and
//! the commit-message half builds the prompt from the staged diff (secrets withheld, lockfiles named, long files
//! cut) and runs the agent CLI the user picked, non-interactively. Nothing here ever commits.

use serde::Serialize;
use serde_json::{Value, json};

use crate::sys::CommandExt as _;

// ---------- diff and status parsers ----------

/// Untracked files larger than this show as binary instead of their content.
pub const UNTRACKED_MAX: u64 = 512 * 1024;

/// Args of the `git_status` command; the tests run the real git with exactly these.
/// `--untracked-files=all` lists every untracked file (git collapses a new folder to `dir/` otherwise);
/// `--ignored=matching` keeps an ignored directory as one `dir/` entry (`traditional` + `-uall` would list every
/// file of `node_modules/`).
pub const STATUS_ARGS: [&str; 6] = [
    "status",
    "--porcelain=v2",
    "-z",
    "--branch",
    "--ignored=matching",
    "--untracked-files=all",
];

// The parsers below expect `a/` and `b/`. The user's own git config changes them (`diff.mnemonicPrefix` prints
// `c/ w/ i/`, `diff.noprefix` none), which hid an untracked `.env` from the secret filter (#414): every `git diff`
// Atlas parses by path comes from the args below and nowhere else. They also carry `--no-ext-diff`: a `diff.external`
// driver (difftastic and friends) prints no `diff --git` header, so an untracked file's content would be filed under
// the previous file's section and a `.env` would slip past the secret filter.

/// Args of the diff behind the Changes view (add `--cached`, or a base, after them).
pub const REVIEW_DIFF: [&str; 7] = [
    "diff",
    "--no-color",
    "-U3",
    "--no-ext-diff",
    "-M",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// Args of the staged diff the commit message is written from.
pub const STAGED_CHANGES: [&str; 7] = [
    "diff",
    "--cached",
    "--no-color",
    "-M",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// Args of the "nothing is staged" fallback: tracked changes (new files come through [`new_file_args`]).
pub const ALL_CHANGES: [&str; 7] = [
    "diff",
    "HEAD",
    "--no-color",
    "-M",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// Args of `git diff --no-index /dev/null <path>`, which shows an untracked file as an addition without touching the
/// index. It exits 1 when the files differ: the normal case.
pub fn new_file_args(path: &str) -> [&str; 9] {
    [
        "diff",
        "--no-index",
        "--no-color",
        "--no-ext-diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--",
        "/dev/null",
        path,
    ]
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Line {
    pub kind: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_no: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_no: Option<u32>,
}

#[derive(Serialize, Debug)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<Line>,
}

#[derive(Serialize, Debug)]
pub struct FileDiff {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// `modified|added|deleted|renamed|untracked`
    pub status: &'static str,
    pub staged: bool,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
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

/// Entries of `git status --porcelain=v2 -z --branch [--ignored]`: (branch, upstream, ab, entries).
/// Ignored paths (`! path`, a collapsed directory keeps its trailing `/`) are `{x:"!", y:"!"}`.
pub fn parse_status(out: &str) -> (Option<String>, Option<String>, Option<String>, Vec<Value>) {
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
        } else if let Some(p) = rec.strip_prefix("! ") {
            entries.push(json!({"path": p, "x": "!", "y": "!"}));
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
    (branch, upstream, ab, entries)
}

/// The base a branch is compared against: `branch.<b>.base`, else `HEAD`.
pub async fn base_for<Fut: Future<Output = Result<String, String>>>(
    branch: Option<&str>,
    git: &impl Fn(&[&str]) -> Fut,
) -> String {
    if let Some(b) = branch.filter(|b| !b.is_empty())
        && let Ok(base) = git(&["config", &format!("branch.{b}.base")]).await
        && !base.trim().is_empty()
    {
        return base.trim().to_string();
    }
    "HEAD".into()
}

/// The values [`operation`] returns. The desktop words each one (`operationText`); any other value shows as it came.
pub const OPERATIONS: [&str; 2] = ["merge", "rebase"];
/// The `status` of a [`FileDiff`]. The desktop words each one (`statusText`); `untracked` is the status of a new
/// file's diff, which the desktop shell builds itself.
pub const FILE_STATUSES: [&str; 5] = ["modified", "added", "deleted", "renamed", "untracked"];

/// `merge` or `rebase` while one is in progress in the checkout at `dir`. The git dir is `.git`, or what a linked
/// worktree's `.git` file (`gitdir: <path>`) points to: a file read, not a `rev-parse` process.
pub fn operation(dir: &std::path::Path) -> Option<&'static str> {
    let dot = dir.join(".git");
    let gd = if dot.is_dir() {
        dot
    } else {
        let pointer = std::fs::read_to_string(&dot).ok()?;
        dir.join(pointer.trim().strip_prefix("gitdir:")?.trim())
    };
    if gd.join("MERGE_HEAD").exists() {
        Some("merge")
    } else if gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists() {
        Some("rebase")
    } else {
        None
    }
}

/// What the `git_status` command returns to the app: branch, upstream, `ab` (`+1 -0`), the base and the ahead/behind
/// against it, the entries and the merge/rebase in progress. `git` runs `git -C dir <args>` where the checkout is.
pub async fn status_json<Fut: Future<Output = Result<String, String>>>(
    dir: &std::path::Path,
    git: &impl Fn(&[&str]) -> Fut,
) -> Result<Value, String> {
    let out = git(&STATUS_ARGS[..]).await?;
    let (branch, upstream, ab, entries) = parse_status(&out);
    // `--branch` already names the branch (`(detached)` has none), so no `branch --show-current`.
    let base = base_for(branch.as_deref().filter(|b| *b != "(detached)"), git).await;
    let vs_base = if base != "HEAD" {
        git(&[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{base}"),
        ])
        .await
        .ok()
        .and_then(|s| {
            let mut n = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
            Some(json!({"ahead": n.next()?, "behind": n.next()?}))
        })
    } else {
        None
    };
    Ok(json!({
        "branch": branch, "upstream": upstream, "ab": ab, "base": base,
        "vs_base": vs_base, "entries": entries, "operation": operation(dir),
    }))
}

// ---------- AI commit message ----------

/// Total characters of diff text sent to the provider (about 12k tokens).
pub const BUDGET: usize = 48_000;
/// Characters of one file's diff before it is cut.
pub const PER_FILE: usize = 6_000;
pub const SUBJECT_MAX: usize = 72;

/// What goes into the prompt about the changes.
#[derive(Serialize, Debug, PartialEq, Default)]
pub struct Digest {
    /// Diff text, per-file cut and under budget.
    pub text: String,
    /// `staged`, or `all` when nothing was staged.
    pub scope: &'static str,
    /// Paths named only (binary, lockfile, generated).
    pub summarized: Vec<String>,
    /// Paths left out entirely because they look like secrets.
    pub withheld: Vec<String>,
    /// Paths whose diff was cut to fit.
    pub truncated: Vec<String>,
    /// Paths dropped because the total budget ran out.
    pub omitted: Vec<String>,
}

/// Split `git diff` output into `(path, section)`, one per file; the path is the new one (`b/…`).
pub fn split_files(diff: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            out.push((header_path(rest.trim_end()), String::new()));
        }
        if let Some((_, sec)) = out.last_mut() {
            sec.push_str(line);
        }
    }
    out
}

/// The new path of `a/<old> b/<new>`. Both halves are the same unless renamed; with spaces in the name the
/// halves are found by their ` b/` marker, taking the one that makes the two sides symmetric first.
fn header_path(rest: &str) -> String {
    let body = rest.strip_prefix("a/").unwrap_or(rest);
    let mut at = 0;
    while let Some(i) = body[at..].find(" b/") {
        let split = at + i;
        let (old, new) = (&body[..split], &body[split + 3..]);
        if old == new {
            return new.trim_matches('"').to_string();
        }
        at = split + 1;
    }
    // A rename: the new path is after the last ` b/`.
    body.rsplit_once(" b/")
        .map_or(body, |(_, n)| n)
        .trim_matches('"')
        .to_string()
}

/// The old path of a renamed or copied file section (its lines may be the old file's content as context).
fn renamed_from(section: &str) -> Option<&str> {
    section
        .lines()
        .take_while(|l| !l.starts_with("@@ "))
        .find_map(|l| {
            l.strip_prefix("rename from ")
                .or_else(|| l.strip_prefix("copy from "))
        })
        .map(|p| p.trim_matches('"'))
}

/// True for paths whose content must never leave the machine.
pub fn is_secret(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return !matches!(
            name.as_str(),
            ".env.example" | ".env.sample" | ".env.template" | ".env.dist"
        );
    }
    const NAMES: &[&str] = &[
        "id_rsa",
        "id_dsa",
        "id_ecdsa",
        "id_ed25519",
        ".npmrc",
        ".netrc",
        ".pypirc",
        "secrets.json",
        "secrets.yaml",
        "secrets.yml",
        "credentials.json",
        "credentials",
        "service-account.json",
        "serviceaccount.json",
        ".htpasswd",
    ];
    const EXTS: &[&str] = &[
        ".pem",
        ".key",
        ".p12",
        ".pfx",
        ".jks",
        ".keystore",
        ".kdbx",
        ".ppk",
    ];
    NAMES.contains(&name.as_str()) || EXTS.iter().any(|e| name.ends_with(e))
}

/// True for lockfiles and generated files: named, never shown.
pub fn is_noise(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    const LOCKS: &[&str] = &[
        "bun.lock",
        "bun.lockb",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "cargo.lock",
        "gemfile.lock",
        "poetry.lock",
        "uv.lock",
        "composer.lock",
        "go.sum",
        "flake.lock",
        "pipfile.lock",
        "mix.lock",
        "pubspec.lock",
        "podfile.lock",
        "gradle.lockfile",
    ];
    const GENERATED: &[&str] = &[".min.js", ".min.css", ".map", ".snap", ".svg"];
    LOCKS.contains(&name.as_str())
        || GENERATED.iter().any(|e| name.ends_with(e))
        || path.split('/').any(|d| {
            matches!(
                d,
                "dist" | "build" | "node_modules" | "vendor" | ".next" | "target"
            )
        })
}

fn is_binary(section: &str) -> bool {
    section
        .lines()
        .any(|l| l.starts_with("Binary files ") || l.starts_with("GIT binary patch"))
}

/// Keep the header and the first hunks of `section` up to `max` characters, on a line boundary, and say how
/// many lines went.
fn cut(section: &str, max: usize) -> String {
    let mut end = 0;
    for line in section.split_inclusive('\n') {
        if end + line.len() > max {
            break;
        }
        end += line.len();
    }
    let lost = section[end..].lines().count();
    format!("{}[… {lost} more lines cut]\n", &section[..end])
}

/// Reduce a full diff to a [`Digest`] under [`BUDGET`]: secrets withheld, noise and binaries named, big files cut,
/// small files first so one huge file cannot starve the rest.
pub fn digest(diff: &str, scope: &'static str) -> Digest {
    let mut d = Digest {
        scope,
        ..Default::default()
    };
    let mut shown: Vec<(String, String)> = Vec::new();
    for (path, section) in split_files(diff) {
        if is_secret(&path) || renamed_from(&section).is_some_and(is_secret) {
            d.withheld.push(path);
        } else if is_noise(&path) || is_binary(&section) {
            d.summarized.push(path);
        } else {
            shown.push((path, section));
        }
    }
    // Smallest first gets the most files in; the original order is restored for reading.
    let mut order: Vec<usize> = (0..shown.len()).collect();
    order.sort_by_key(|&i| shown[i].1.len().min(PER_FILE));
    let mut used = 0;
    let mut keep: Vec<Option<String>> = vec![None; shown.len()];
    for i in order {
        let (path, section) = &shown[i];
        let text = if section.len() > PER_FILE {
            d.truncated.push(path.clone());
            cut(section, PER_FILE)
        } else {
            section.clone()
        };
        if used + text.len() > BUDGET {
            d.omitted.push(path.clone());
        } else {
            used += text.len();
            keep[i] = Some(text);
        }
    }
    d.text = keep.into_iter().flatten().collect();
    d.truncated.retain(|p| !d.omitted.contains(p));
    d.truncated.sort();
    d.omitted.sort();
    d
}

/// The prompt: rules, recent subjects, the user's instruction and the digest.
pub fn build_prompt(rules: &str, recent: &[String], instruction: &str, d: &Digest) -> String {
    let mut p = String::from(
        "Write the git commit message for the changes below.\n\
         Rules:\n\
         - Reply with the commit message only: no preamble, no code fence, no quotes.\n\
         - First line is the subject: imperative mood, at most 72 characters, no trailing period.\n\
         - Add a body after a blank line only when the change needs a why; wrap it at 72 columns.\n\
         - Follow the style of the recent subjects below.\n",
    );
    if !rules.trim().is_empty() {
        p.push_str(&format!(
            "\nThis repository's commit rules:\n{}\n",
            rules.trim()
        ));
    }
    if !recent.is_empty() {
        p.push_str("\nRecent commit subjects (match their style):\n");
        for r in recent {
            p.push_str(&format!("- {r}\n"));
        }
    }
    if !instruction.trim().is_empty() {
        p.push_str(&format!(
            "\nExtra instruction from the author: {}\n",
            instruction.trim()
        ));
    }
    if d.scope == "all" {
        p.push_str("\nNote: nothing was staged, so this is the diff of all uncommitted changes.\n");
    }
    let listed = |title: &str, v: &[String]| {
        if v.is_empty() {
            String::new()
        } else {
            format!("{title}: {}\n", v.join(", "))
        }
    };
    p.push_str("\nChanged files not shown in full:\n");
    let notes = [
        listed(
            "Only their names matter (lockfiles, binaries, generated)",
            &d.summarized,
        ),
        listed(
            "Contents withheld because they may hold secrets",
            &d.withheld,
        ),
        listed("Diff cut to fit", &d.truncated),
        listed("Left out, too much to send", &d.omitted),
    ]
    .concat();
    p.push_str(if notes.is_empty() { "none\n" } else { &notes });
    p.push_str("\nDiff:\n");
    p.push_str(&d.text);
    p
}

/// Commit rules out of CONTRIBUTING.md / AGENTS.md: the bullet and paragraph lines that talk about commits.
pub fn commit_rules(docs: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (name, text) in docs {
        for line in text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let lower = line.to_ascii_lowercase();
            if lower.contains("commit")
                || lower.contains("signed-off")
                || lower.contains("conventional")
            {
                let line: String = line.chars().take(400).collect();
                if !out.contains(&line) && out.len() + line.len() < 1_800 {
                    out.push_str(&format!("{line}\n"));
                }
            }
        }
        let _ = name;
    }
    out.trim_end().to_string()
}

/// Model output → commit message: no fences or preamble, subject within [`SUBJECT_MAX`] (the cut words move
/// to the body, never lost).
pub fn clean_message(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if text.starts_with("```") {
        text = text.lines().skip(1).collect::<Vec<_>>().join("\n");
        if let Some(end) = text.rfind("```") {
            text.truncate(end);
        }
    }
    let mut text = text.trim().to_string();
    // "Here is the commit message:" / "Commit message: x" before the real subject.
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(first) = lines.first().map(|l| l.trim()) {
        let l = first.to_ascii_lowercase();
        if l.ends_with(':') && (l.contains("commit message") || l.starts_with("here")) {
            lines.remove(0);
            while lines.first().is_some_and(|l| l.trim().is_empty()) {
                lines.remove(0);
            }
        } else if let Some(rest) = l.strip_prefix("commit message:") {
            let n = first.len() - rest.len();
            text = std::iter::once(first[n..].trim())
                .chain(lines.iter().skip(1).copied())
                .collect::<Vec<_>>()
                .join("\n");
            lines = text.lines().collect();
        }
    }
    let text = lines.join("\n");
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    let (subject, body) = text.split_once('\n').unwrap_or((text, ""));
    let (subject, body) = (subject.trim(), body.trim());
    if subject.is_empty() {
        return String::new();
    }
    // Subject, one blank line, body: whatever blank lines the model left are normalized to one.
    let (head, tail) = if subject.chars().count() <= SUBJECT_MAX {
        (subject.to_string(), String::new())
    } else {
        split_subject(subject)
    };
    match (tail.is_empty(), body.is_empty()) {
        (true, true) => head,
        (true, false) => format!("{head}\n\n{body}"),
        (false, true) => format!("{head}\n\n{tail}"),
        (false, false) => format!("{head}\n\n{tail}\n{body}"),
    }
}

/// `(head ≤ SUBJECT_MAX chars on a word boundary, rest)`; one word longer than the limit is cut by chars.
fn split_subject(subject: &str) -> (String, String) {
    // The last space inside the first SUBJECT_MAX chars; none → cut the word itself.
    let mut cut = 0;
    for (n, (i, c)) in subject.char_indices().enumerate() {
        if n == SUBJECT_MAX {
            break;
        }
        if c == ' ' {
            cut = i;
        }
    }
    if cut == 0 {
        cut = subject
            .char_indices()
            .nth(SUBJECT_MAX)
            .map_or(subject.len(), |(i, _)| i);
    }
    (
        subject[..cut].trim_end().to_string(),
        subject[cut..].trim_start().to_string(),
    )
}

/// How an agent CLI is run to answer one prompt and exit: `argv`, prompt on stdin, no tools, no saved session
/// (`one_shot` in `data/agents.json`; `None` for an agent with no verified invocation). Flags verified against
/// each CLI's `--help` (omp 18.11, claude 2.1, codex 0.162).
pub fn provider_argv(agent: &str) -> Option<&'static [String]> {
    crate::agents::get(agent)?.one_shot.as_deref()
}

/// The built-in agents that can write a commit message (have a `one_shot`), in the registry's order.
pub fn provider_ids() -> impl Iterator<Item = &'static str> {
    crate::agents::all()
        .iter()
        .filter(|a| a.one_shot.is_some())
        .map(|a| a.id.as_str())
}

// ---------- provider selection and the run ----------

/// What the user chose in Settings → Git (`gitCommitAgent`) and the agents that exist.
#[derive(Debug, PartialEq)]
pub enum Provider {
    Agent(&'static str),
    /// The feature is switched off: the UI hides the button.
    Off,
    /// Nothing usable: the UI shows how to set one up.
    None,
}

/// `setting`: `off`, an agent id, or absent/`auto` (the default agent when installed, else the first installed).
pub fn resolve_provider(
    setting: Option<&str>,
    default_agent: Option<&str>,
    installed: &[&str],
) -> Provider {
    // An agent is a candidate when it can run one-shot and is installed.
    let known = |id: &str| provider_ids().find(|k| *k == id && installed.contains(k));
    match setting {
        Some("off") => Provider::Off,
        Some(id) if id != "auto" && !id.is_empty() => {
            known(id).map_or(Provider::None, Provider::Agent)
        }
        _ => default_agent
            .and_then(known)
            .or_else(|| provider_ids().find(|k| installed.contains(k)))
            .map_or(Provider::None, Provider::Agent),
    }
}

/// The stable codes `commit_msg_generate` and [`run_provider`] send as the whole message of an error for the failures
/// the app words itself, because a sentence from Rust is English and the desktop shows the language of the person.
/// The desktop maps each to a message (`commitErrorText`); anything else is an agent's own output and shows as it
/// came. The list is the fixture `__fixtures__/commit-msg-errors.json` that the desktop test reads.
pub mod commit_error {
    /// The folder is on an SSH host: the agent would have to run there.
    pub const SSH_HOST: &str = "commit_msg.ssh_host";
    /// Settings > Git turned the feature off.
    pub const PROVIDER_OFF: &str = "commit_msg.provider_off";
    /// Nothing staged and nothing changed.
    pub const NO_CHANGES: &str = "commit_msg.no_changes";
    /// Every changed file looks like a secret, so nothing was sent.
    pub const ALL_SECRET: &str = "commit_msg.all_secret";
    /// The agent ended well and printed no message.
    pub const EMPTY_MESSAGE: &str = "commit_msg.empty_message";
    pub const ALL: [&str; 5] = [
        SSH_HOST,
        PROVIDER_OFF,
        NO_CHANGES,
        ALL_SECRET,
        EMPTY_MESSAGE,
    ];
}

/// One event of a run, as the frontend reads it from `commit-msg://<request>`.
#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// What is being sent: provider and what was cut or withheld.
    Start {
        agent: String,
        scope: &'static str,
        digest: DigestInfo,
    },
    /// Text so far (the whole message, not a delta).
    Chunk {
        text: String,
    },
    Done {
        message: String,
    },
    Error {
        message: String,
    },
    Cancelled,
}

/// The parts of a [`Digest`] the UI tells the user about (never the diff text).
#[derive(Serialize, Debug, PartialEq, Clone, Default)]
pub struct DigestInfo {
    pub summarized: usize,
    pub withheld: usize,
    pub truncated: usize,
    pub omitted: usize,
}

impl From<&Digest> for DigestInfo {
    fn from(d: &Digest) -> Self {
        Self {
            summarized: d.summarized.len(),
            withheld: d.withheld.len(),
            truncated: d.truncated.len(),
            omitted: d.omitted.len(),
        }
    }
}

/// Run `argv` with `prompt` on stdin, reporting the cleaned text so far through `emit` as it arrives.
/// Resolves to the final cleaned message, or an error; `cancel` firing kills the child and resolves `Cancelled`.
pub async fn run_provider(
    argv: &[String],
    cwd: &std::path::Path,
    env: &[(String, String)],
    prompt: &str,
    mut cancel: tokio::sync::oneshot::Receiver<()>,
    emit: impl Fn(Event),
) -> Event {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some((bin, args)) = argv.split_first() else {
        return Event::Error {
            message: "no provider command".into(),
        };
    };
    let mut child = match tokio::process::Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .no_window()
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Event::Error {
                message: format!("{bin}: {e}"),
            };
        }
    };
    let mut stdin = child.stdin.take().expect("piped");
    let prompt = prompt.to_string();
    // A provider that exits before reading the prompt (bad flag) closes the pipe: that is its error to report.
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(prompt.as_bytes()).await;
    });
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let err_task = tokio::spawn(async move {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b).await;
        String::from_utf8_lossy(&b).into_owned()
    });
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    let result = loop {
        tokio::select! {
            _ = &mut cancel => {
                let _ = child.kill().await;
                writer.abort();
                return Event::Cancelled;
            }
            n = out.read(&mut buf) => match n {
                Ok(0) | Err(_) => break child.wait().await,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    emit(Event::Chunk { text: clean_message(&String::from_utf8_lossy(&raw)) });
                }
            },
        }
    };
    let _ = writer.await;
    let stderr = err_task.await.unwrap_or_default();
    let message = clean_message(&String::from_utf8_lossy(&raw));
    match result {
        Ok(st) if st.success() && !message.is_empty() => Event::Done { message },
        Ok(st) if st.success() => Event::Error {
            message: commit_error::EMPTY_MESSAGE.into(),
        },
        Ok(st) => {
            // claude prints its auth failure on stdout; omp prints progress on stderr: show whichever says why.
            let why = if stderr.trim().is_empty() {
                String::from_utf8_lossy(&raw).into_owned()
            } else {
                stderr
            };
            Event::Error {
                message: format!("{bin} exited with {st}: {}", why.trim()),
            }
        }
        Err(e) => Event::Error {
            message: format!("{bin}: {e}"),
        },
    }
}

#[cfg(test)]
mod parser_tests {
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

    /// Verbatim `git status --porcelain=v2 -z --branch --ignored` of a repo with node_modules/ (dir), .env and
    /// target/ ignored, src/m.ts modified, two untracked files.
    const STATUS_IGNORED: &str = "# branch.oid e494ad1\0# branch.head main\0\
1 .M N... 100644 100644 100644 e69de29 e69de29 src/m.ts\0\
? new.txt\0? src/n.ts\0! .env\0! node_modules/\0! target/\0";

    // Incident: the explorer could not dim ignored files because git_status never asked for them. The payload the
    // frontend reads is this serialized value: `x`/`y` = "!" and a collapsed directory keeps its trailing `/`.
    #[test]
    fn ignored_paths_are_reported_with_bang_and_keep_the_dir_slash() {
        let (branch, _, _, entries) = parse_status(STATUS_IGNORED);
        assert_eq!(branch.as_deref(), Some("main"));
        let v = serde_json::to_value(&entries).unwrap();
        let by = |p: &str| {
            v.as_array()
                .unwrap()
                .iter()
                .find(|e| e["path"] == p)
                .cloned()
        };
        for p in [".env", "node_modules/", "target/"] {
            let e = by(p).unwrap_or_else(|| panic!("{p} missing: {v}"));
            assert_eq!(
                (e["x"].as_str(), e["y"].as_str()),
                (Some("!"), Some("!")),
                "{p}"
            );
        }
        assert_eq!(by("src/m.ts").unwrap()["y"], "M");
        assert_eq!(by("new.txt").unwrap()["x"], "?");
        assert_eq!(v.as_array().unwrap().len(), 6);
    }

    // Edge: an ignored path with spaces / unicode (`-z` is unquoted) stays whole.
    #[test]
    fn ignored_path_with_spaces_is_not_split() {
        let (_, _, _, entries) = parse_status("! my build/out dir/\0! café.log\0");
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["my build/out dir/", "café.log"]);
    }

    // The parser is only as good as the flag: run the real git against a real repo with the exact args git_status uses.
    #[test]
    fn real_git_reports_ignored_entries_for_the_args_git_status_uses() {
        let d = tempfile::tempdir().unwrap();
        let run = |a: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(d.path())
                .args(["-c", "user.email=a@b", "-c", "user.name=n"])
                .args(a)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {a:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        run(&["init", "-q", "."]);
        std::fs::write(d.path().join(".gitignore"), "node_modules/\n.env\n").unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/x")).unwrap();
        std::fs::write(d.path().join("node_modules/x/i.js"), "").unwrap();
        std::fs::write(d.path().join(".env"), "").unwrap();
        std::fs::write(d.path().join("new.txt"), "").unwrap();
        let out = run(&STATUS_ARGS);
        let (_, _, _, entries) = parse_status(&out);
        let get = |p: &str| {
            entries
                .iter()
                .find(|e| e["path"] == p)
                .map(|e| e["x"].clone())
        };
        assert_eq!(get("node_modules/"), Some(json!("!")), "{entries:?}");
        assert_eq!(get(".env"), Some(json!("!")));
        assert_eq!(get("new.txt"), Some(json!("?")));
    }

    /// Contract with the frontend: `apps/desktop/src/lib/__fixtures__/git-status.json` is the REAL serialization
    /// of the `git_status` entries (parse of a verbatim `--ignored` run), which `tree.test.ts` classifies. A
    /// rename of `x`/`y`/`path` on either side fails a test. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn git_status_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/git-status.json");
        let (branch, upstream, ab, entries) = parse_status(STATUS_IGNORED);
        let value = json!({"branch": branch, "upstream": upstream, "ab": ab, "entries": entries});
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it");
        let committed: Value = serde_json::from_str(&committed).unwrap();
        let fresh: Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}

#[cfg(test)]
mod untracked_tests {
    use super::*;

    fn real_repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let run = |a: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(d.path())
                .args(["-c", "user.email=a@b", "-c", "user.name=n"])
                .args(a)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {a:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        run(&["init", "-q", "."]);
        std::fs::write(d.path().join(".gitignore"), "node_modules/\n").unwrap();
        d
    }

    fn write(d: &tempfile::TempDir, rel: &str) {
        let p = d.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    /// Runs the real git with the exact args `git_status` uses and parses it with the real parser.
    fn status(d: &tempfile::TempDir) -> Vec<(String, String)> {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args(STATUS_ARGS)
            .output()
            .unwrap();
        assert!(o.status.success());
        let (_, _, _, entries) = parse_status(&String::from_utf8_lossy(&o.stdout));
        entries
            .iter()
            .map(|e| {
                (
                    e["path"].as_str().unwrap().to_string(),
                    e["x"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    // Incident #29: a new folder showed as ONE untracked row (`.logs/`, git's collapsed form) instead of its files,
    // so they could not be reviewed, staged or discarded one by one.
    #[test]
    fn untracked_folders_are_listed_file_by_file_not_collapsed() {
        let d = real_repo();
        write(&d, ".logs/api.log");
        write(&d, ".logs/web.log");
        write(&d, "pkg/deep/er/a.txt");
        write(&d, "top.txt");
        let got = status(&d);
        let untracked: Vec<&str> = got
            .iter()
            .filter(|(_, x)| x == "?")
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(
            untracked,
            [
                ".gitignore",
                ".logs/api.log",
                ".logs/web.log",
                "pkg/deep/er/a.txt",
                "top.txt"
            ],
            "{got:?}"
        );
        assert!(
            got.iter()
                .all(|(p, _)| !p.ends_with('/') || p == "node_modules/"),
            "{got:?}"
        );
    }

    // Same class: the explorer dims ignored directories as ONE collapsed `dir/` entry (see
    // `ignored_paths_are_reported_with_bang_and_keep_the_dir_slash`); listing untracked files individually must
    // not explode an ignored directory into thousands of `!` rows.
    #[test]
    fn ignored_directories_stay_collapsed_while_untracked_are_expanded() {
        let d = real_repo();
        write(&d, "node_modules/a/i.js");
        write(&d, "node_modules/b/j.js");
        write(&d, "src/new.ts");
        let got = status(&d);
        let ignored: Vec<&str> = got
            .iter()
            .filter(|(_, x)| x == "!")
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(ignored, ["node_modules/"], "{got:?}");
        assert!(got.contains(&("src/new.ts".into(), "?".into())), "{got:?}");
    }

    // Paths with spaces and non-ASCII inside an untracked folder stay whole and unquoted (`-z`).
    #[test]
    fn untracked_files_with_spaces_and_unicode_inside_folders_stay_whole() {
        let d = real_repo();
        write(&d, "my dir/café au lait.txt");
        let got = status(&d);
        assert!(
            got.contains(&("my dir/café au lait.txt".into(), "?".into())),
            "{got:?}"
        );
    }
}

#[cfg(test)]
mod commit_msg_tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
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
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    /// A real repo with one commit and these staged changes, and the real `git diff --cached` of them.
    fn staged(files: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "."]);
        std::fs::write(d.path().join("README.md"), "hello\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        for (name, body) in files {
            let p = d.path().join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        git(d.path(), &["add", "-A"]);
        let diff = git(d.path(), &STAGED_CHANGES);
        (d, diff)
    }

    #[test]
    fn splits_a_real_diff_one_section_per_file_with_the_new_path() {
        let (_d, diff) = staged(&[("src/a.rs", b"fn a() {}\n"), ("docs/b.md", b"# b\n")]);
        let files = split_files(&diff);
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["docs/b.md", "src/a.rs"]);
        assert!(files[1].1.contains("+fn a() {}"));
        assert!(
            !files[1].1.contains("# b"),
            "sections must not bleed into each other"
        );
    }

    #[test]
    fn a_rename_is_split_under_its_new_path() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "."]);
        std::fs::write(
            d.path().join("old.txt"),
            "same content here\nline two\nline three\n",
        )
        .unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        git(d.path(), &["mv", "old.txt", "new name.txt"]);
        let diff = git(d.path(), &STAGED_CHANGES);
        let files = split_files(&diff);
        assert_eq!(files.len(), 1, "{diff}");
        assert_eq!(files[0].0, "new name.txt");
    }

    // Privacy: the content of secret-looking files is never in what is sent, and the digest says it withheld them.
    #[test]
    fn secrets_are_withheld_and_never_reach_the_prompt() {
        let (_d, diff) = staged(&[
            (".env", b"API_KEY=sk-live-123456\n"),
            (".env.production", b"DB=postgres://u:hunter2@h/db\n"),
            ("certs/server.pem", b"-----BEGIN PRIVATE KEY-----\nAAA\n"),
            ("id_rsa", b"-----BEGIN OPENSSH PRIVATE KEY-----\n"),
            ("src/ok.rs", b"fn ok() {}\n"),
        ]);
        let d = digest(&diff, "staged");
        assert!(d.text.contains("fn ok()"));
        for leak in ["sk-live-123456", "hunter2", "PRIVATE KEY"] {
            assert!(!d.text.contains(leak), "{leak} leaked: {}", d.text);
        }
        let mut w = d.withheld.clone();
        w.sort();
        assert_eq!(w, [".env", ".env.production", "certs/server.pem", "id_rsa"]);
        let p = build_prompt("", &[], "", &d);
        assert!(!p.contains("sk-live-123456") && !p.contains("hunter2"));
        assert!(
            p.contains(".env"),
            "the prompt still says a file changed: {p}"
        );
    }

    #[test]
    fn secret_detection_covers_names_and_leaves_lookalikes_alone() {
        for s in [
            ".env",
            ".env.local",
            "apps/api/.env.production",
            "server.pem",
            "a/b/key.p12",
            "id_ed25519",
            "secrets.json",
            "credentials.json",
            ".npmrc",
            "service-account.json",
            "x.key",
            ".netrc",
        ] {
            assert!(is_secret(s), "{s} must be withheld");
        }
        for s in [
            ".env.example",
            ".env.sample",
            "environment.ts",
            "src/keyboard.rs",
            "docs/secrets-policy.md",
            "monkey.rs",
            "pem.rs",
        ] {
            assert!(!is_secret(s), "{s} is not a secret");
        }
    }

    #[test]
    fn lockfiles_binaries_and_generated_files_are_named_not_shown() {
        let (_d, diff) = staged(&[
            ("bun.lock", b"{\"lock\": \"a\"}\n".repeat(50).as_slice()),
            ("Cargo.lock", b"[[package]]\nname = \"x\"\n"),
            ("logo.png", &[0x89, b'P', b'N', b'G', 0, 1, 2, 3, 0, 0, 0]),
            ("dist/app.min.js", b"var a=1;\n"),
            ("src/main.rs", b"fn main() {}\n"),
        ]);
        let d = digest(&diff, "staged");
        assert!(d.text.contains("fn main()"));
        assert!(!d.text.contains("\"lock\""), "{}", d.text);
        let mut s = d.summarized.clone();
        s.sort();
        assert_eq!(s, ["Cargo.lock", "bun.lock", "dist/app.min.js", "logo.png"]);
        assert!(d.withheld.is_empty());
    }

    // Budget: one huge file is cut with a note saying how much; the small files around it stay whole.
    #[test]
    fn a_large_file_is_cut_with_a_note_and_small_ones_stay_whole() {
        let big: String = (0..3000)
            .map(|i| format!("let value_{i} = {i};\n"))
            .collect();
        let (_d, diff) = staged(&[
            ("src/big.rs", big.as_bytes()),
            ("src/small.rs", b"fn small() {}\n"),
        ]);
        let d = digest(&diff, "staged");
        assert_eq!(d.truncated, ["src/big.rs"]);
        assert!(d.text.contains("fn small() {}"));
        assert!(d.text.contains("value_0"));
        assert!(!d.text.contains("value_2999"), "the tail was cut");
        assert!(
            d.text.contains("more lines cut"),
            "{}",
            &d.text[..300.min(d.text.len())]
        );
        assert!(d.text.len() <= BUDGET + 1_000);
    }

    // Many files: the budget is shared; the files that did not fit are listed, never silently dropped.
    #[test]
    fn the_total_budget_holds_and_dropped_files_are_listed() {
        let body = "x".repeat(5_000) + "\n";
        let files: Vec<(String, Vec<u8>)> = (0..30)
            .map(|i| (format!("src/f{i:02}.rs"), body.clone().into_bytes()))
            .collect();
        let refs: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let (_d, diff) = staged(&refs);
        let d = digest(&diff, "staged");
        assert!(d.text.len() <= BUDGET + 1_000, "{} chars", d.text.len());
        assert!(!d.omitted.is_empty());
        let shown = split_files(&d.text).len();
        assert_eq!(
            shown + d.omitted.len(),
            30,
            "every file is shown or listed as omitted"
        );
    }

    #[test]
    fn an_empty_diff_gives_an_empty_digest() {
        let d = digest("", "staged");
        assert!(d.text.is_empty() && d.summarized.is_empty() && d.withheld.is_empty());
    }

    #[test]
    fn the_prompt_carries_rules_recent_subjects_instruction_and_what_was_cut() {
        let d = Digest {
            text: "diff --git a/x b/x\n+hi\n".into(),
            scope: "all",
            summarized: vec!["bun.lock".into()],
            withheld: vec![".env".into()],
            truncated: vec!["big.rs".into()],
            omitted: vec!["z.rs".into()],
        };
        let p = build_prompt(
            "Subjects are imperative and conventional.",
            &[
                "fix(desktop): stop the flicker".into(),
                "feat: add x".into(),
            ],
            "mention #72",
            &d,
        );
        for must in [
            "Subjects are imperative and conventional.",
            "fix(desktop): stop the flicker",
            "feat: add x",
            "mention #72",
            "+hi",
            "bun.lock",
            "big.rs",
            "z.rs",
            "72",
            "imperative",
        ] {
            assert!(p.contains(must), "prompt lacks {must:?}:\n{p}");
        }
        assert!(
            p.contains("nothing was staged"),
            "says it used all changes: {p}"
        );
    }

    #[test]
    fn the_prompt_has_no_empty_sections_when_there_is_nothing_to_say() {
        let d = Digest {
            text: "d\n".into(),
            scope: "staged",
            ..Default::default()
        };
        let p = build_prompt("", &[], "", &d);
        assert!(!p.contains("Recent commit subjects"), "{p}");
        assert!(!p.contains("Extra instruction"), "{p}");
        assert!(!p.contains("nothing was staged"), "{p}");
    }

    #[test]
    fn commit_rules_come_from_the_commit_lines_of_the_docs_only() {
        let contributing = "# Contributing\n\nBuild with cargo.\n\n- Commit subjects are short and imperative with a conventional prefix (`feat:`).\n- Run the gate.\n- Every commit carries a Signed-off-by trailer.\n".to_string();
        let agents = "# Agents\n- Use bun.\n- Never amend a commit that was pushed.\n".to_string();
        let r = commit_rules(&[("CONTRIBUTING.md", contributing), ("AGENTS.md", agents)]);
        assert!(r.contains("short and imperative"));
        assert!(r.contains("Signed-off-by"));
        assert!(r.contains("Never amend"));
        assert!(
            !r.contains("Build with cargo")
                && !r.contains("Use bun")
                && !r.contains("Run the gate")
        );
        assert_eq!(
            commit_rules(&[("AGENTS.md", "# nothing here\n".into())]),
            ""
        );
        assert!(r.len() < 2_000);
    }

    #[test]
    fn clean_message_strips_fences_preamble_and_quotes() {
        assert_eq!(clean_message("```\nfix: a b\n```"), "fix: a b");
        assert_eq!(
            clean_message("```text\nfix: a b\n\nbody line\n```\n"),
            "fix: a b\n\nbody line"
        );
        assert_eq!(
            clean_message("Here is the commit message:\n\nfeat: add x\n"),
            "feat: add x"
        );
        assert_eq!(clean_message("Commit message: fix: a"), "fix: a");
        assert_eq!(clean_message("\"fix: quoted\""), "fix: quoted");
        assert_eq!(clean_message("   \n"), "");
    }

    // The subject is capped at 72 on a word boundary, and the cut text is not lost: it moves to the body.
    #[test]
    fn a_long_subject_is_wrapped_at_72_without_losing_words() {
        let long = "feat(desktop): generate the commit message from the staged diff with the agent the user picked";
        assert!(long.chars().count() > SUBJECT_MAX);
        let out = clean_message(long);
        let subject = out.lines().next().unwrap();
        assert!(subject.chars().count() <= SUBJECT_MAX, "{subject}");
        assert!(subject.starts_with("feat(desktop): generate the commit message"));
        let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(words(&out), words(long), "no word lost or reordered");
        assert_eq!(
            out.lines().nth(1),
            Some(""),
            "blank line between subject and body"
        );
    }

    #[test]
    fn a_subject_with_no_spaces_is_hard_cut_not_looped_forever() {
        let out = clean_message(&"a".repeat(200));
        assert!(out.lines().next().unwrap().chars().count() <= SUBJECT_MAX);
        assert_eq!(out.replace(['\n', ' '], "").len(), 200);
    }

    #[test]
    fn multibyte_subjects_are_cut_on_char_boundaries() {
        let s = format!("fix: {}", "ação ".repeat(30));
        let out = clean_message(&s);
        assert!(out.lines().next().unwrap().chars().count() <= SUBJECT_MAX);
    }

    // The flags come from each CLI's own --help (verified on omp 18.11, claude 2.1, codex 0.162): prompt on stdin,
    // no tools, no session saved. A wrong flag turns into a hanging or file-editing agent, so they are pinned here.
    #[test]
    fn each_provider_runs_non_interactively_with_no_tools_and_no_saved_session() {
        let has = |argv: &[String], f: &str| argv.iter().any(|a| a == f);
        let omp = provider_argv("omp").unwrap();
        assert_eq!(omp[0], "omp");
        for f in ["-p", "--no-session", "--no-tools"] {
            assert!(has(omp, f), "omp lacks {f}: {omp:?}");
        }
        let claude = provider_argv("claude").unwrap();
        assert_eq!(claude[0], "claude");
        assert!(has(claude, "-p") && has(claude, "--no-session-persistence"));
        let at = claude.iter().position(|a| a == "--tools").expect("--tools");
        assert_eq!(
            claude[at + 1],
            "",
            "an empty --tools list disables every tool"
        );
        let codex = provider_argv("codex").unwrap();
        assert_eq!(&codex[..2], ["codex", "exec"]);
        for f in ["--ephemeral", "--skip-git-repo-check", "-"] {
            assert!(has(codex, f), "codex lacks {f}: {codex:?}");
        }
        let at = codex.iter().position(|a| a == "-s").expect("-s");
        assert_eq!(codex[at + 1], "read-only");
        assert_eq!(provider_argv("nope"), None);
    }

    // OpenCode is installed and launchable but has no verified tools-off one-shot run: the diff must never be
    // handed to it, whatever the setting says. (Incident class: the picker offered agents resolve_provider ignored.)
    #[test]
    fn an_agent_without_a_one_shot_run_is_never_a_commit_provider() {
        assert_eq!(
            provider_ids().collect::<Vec<_>>(),
            ["omp", "claude", "codex"]
        );
        assert_eq!(provider_argv("opencode"), None);
        let all = ["omp", "claude", "codex", "opencode"];
        assert_eq!(
            resolve_provider(Some("opencode"), Some("omp"), &all),
            Provider::None
        );
        assert_eq!(
            resolve_provider(None, Some("opencode"), &["opencode", "codex"]),
            Provider::Agent("codex")
        );
        assert_eq!(resolve_provider(None, None, &["opencode"]), Provider::None);
    }

    // ----- provider selection -----

    #[test]
    fn provider_resolution_honours_off_explicit_default_and_fallback() {
        let all = ["omp", "claude", "codex"];
        assert_eq!(
            resolve_provider(Some("off"), Some("omp"), &all),
            Provider::Off
        );
        assert_eq!(
            resolve_provider(Some("claude"), Some("omp"), &all),
            Provider::Agent("claude")
        );
        // Explicit but not installed: setup hint, NOT a silent switch to another agent that would get the diff.
        assert_eq!(
            resolve_provider(Some("claude"), Some("omp"), &["omp"]),
            Provider::None
        );
        assert_eq!(
            resolve_provider(Some("nope"), Some("omp"), &all),
            Provider::None
        );
        // Auto: the default agent when installed, else the first installed, else nothing.
        assert_eq!(
            resolve_provider(None, Some("codex"), &all),
            Provider::Agent("codex")
        );
        assert_eq!(
            resolve_provider(Some("auto"), Some("blank"), &["claude"]),
            Provider::Agent("claude")
        );
        assert_eq!(
            resolve_provider(None, Some("omp"), &["codex"]),
            Provider::Agent("codex")
        );
        assert_eq!(resolve_provider(None, None, &[]), Provider::None);
    }

    // ----- the run, against a fake provider: a real child process, the real streaming and cancel code -----

    #[cfg(unix)]
    /// A fake provider script: prints `body` after reading the whole prompt, then exits `code`.
    fn fake(dir: &Path, script: &str) -> Vec<String> {
        let p = dir.join("fake.sh");
        std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        vec!["sh".into(), p.to_string_lossy().into_owned()]
    }

    #[cfg(unix)]
    fn run(argv: Vec<String>, prompt: &str) -> (Event, Vec<Event>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let events = std::sync::Mutex::new(Vec::new());
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let done = rt.block_on(run_provider(
            &argv,
            Path::new("/tmp"),
            &[],
            prompt,
            rx,
            |e| events.lock().unwrap().push(e),
        ));
        (done, events.into_inner().unwrap())
    }

    #[cfg(unix)]
    #[test]
    fn a_provider_reply_is_cleaned_and_the_prompt_reaches_its_stdin() {
        let d = tempfile::tempdir().unwrap();
        // Echo the prompt's first line into the message: proves stdin delivery; the fence proves cleaning.
        let argv = fake(
            d.path(),
            "read first; printf '```\\nfix: got %s\\n```\\n' \"$first\"; cat >/dev/null",
        );
        let (done, chunks) = run(argv, "PROMPT-LINE\nrest of the prompt\n");
        assert_eq!(
            done,
            Event::Done {
                message: "fix: got PROMPT-LINE".into()
            }
        );
        assert!(
            matches!(chunks.last(), Some(Event::Chunk { text }) if text == "fix: got PROMPT-LINE"),
            "{chunks:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn output_streams_in_pieces_each_chunk_the_whole_message_so_far() {
        let d = tempfile::tempdir().unwrap();
        let argv = fake(
            d.path(),
            "cat >/dev/null; printf 'feat: one'; sleep 0.3; printf ' two'; sleep 0.3; printf ' three\\n'",
        );
        let (done, chunks) = run(argv, "x");
        assert_eq!(
            done,
            Event::Done {
                message: "feat: one two three".into()
            }
        );
        let texts: Vec<_> = chunks
            .iter()
            .map(|e| match e {
                Event::Chunk { text } => text.clone(),
                o => panic!("{o:?}"),
            })
            .collect();
        assert!(texts.len() >= 3, "streamed in pieces: {texts:?}");
        assert_eq!(texts[0], "feat: one");
        assert!(
            texts.windows(2).all(|w| w[1].starts_with(&w[0])),
            "each chunk extends the previous: {texts:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_provider_reports_its_own_reason() {
        let d = tempfile::tempdir().unwrap();
        let (done, _) = run(
            fake(
                d.path(),
                "cat >/dev/null; echo 'quota exceeded' >&2; exit 3",
            ),
            "x",
        );
        match done {
            Event::Error { message } => assert!(
                message.contains("quota exceeded") && message.contains("exited"),
                "{message}"
            ),
            o => panic!("{o:?}"),
        }
        // claude reports auth failures on stdout with a non-zero exit.
        let (done, _) = run(
            fake(
                d.path(),
                "cat >/dev/null; echo 'Failed to authenticate'; exit 1",
            ),
            "x",
        );
        assert!(
            matches!(done, Event::Error { message } if message.contains("Failed to authenticate"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_empty_reply_and_a_missing_binary_are_errors_not_empty_messages() {
        let d = tempfile::tempdir().unwrap();
        let (done, _) = run(fake(d.path(), "cat >/dev/null"), "x");
        assert!(matches!(done, Event::Error { .. }), "{done:?}");
        let (done, _) = run(vec!["/definitely/not/a/binary".into()], "x");
        assert!(matches!(done, Event::Error { message } if message.contains("not/a/binary")));
        let (done, _) = run(vec![], "x");
        assert!(matches!(done, Event::Error { .. }));
    }

    // A provider that exits without reading stdin (a rejected flag) must not hang or panic the writer.
    #[cfg(unix)]
    #[test]
    fn a_provider_that_never_reads_the_prompt_does_not_hang() {
        let d = tempfile::tempdir().unwrap();
        let big = "x".repeat(2_000_000);
        let (done, _) = run(fake(d.path(), "echo 'unknown flag' >&2; exit 2"), &big);
        assert!(matches!(done, Event::Error { message } if message.contains("unknown flag")));
    }

    // Cancel mid-stream: the child is killed (not left running, not waited for) and the result is Cancelled, no
    // message. The provider lives for 30 s and says so in `finished` if it gets to the end: a cancel that waited for
    // it would return `Done` and leave that file, however fast or slow the machine is; no stopwatch is involved.
    #[cfg(unix)]
    #[test]
    fn cancel_kills_the_provider_mid_stream() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("finished");
        let pidfile = d.path().join("pid");
        let argv = fake(
            d.path(),
            &format!(
                "cat >/dev/null; echo $$ > {pid}; printf 'feat: start'; sleep 30; touch {}",
                marker.display(),
                pid = pidfile.display()
            ),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let done = rt.block_on(async {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let s2 = seen.clone();
            let run = tokio::spawn({
                let argv = argv.clone();
                async move {
                    run_provider(&argv, Path::new("/tmp"), &[], "x", rx, move |e| {
                        if matches!(e, Event::Chunk { .. }) {
                            s2.store(true, std::sync::atomic::Ordering::SeqCst)
                        }
                    })
                    .await
                }
            });
            while !seen.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            let _ = tx.send(());
            tokio::time::timeout(crate::scaling::HANG, run)
                .await
                .expect("cancel hung")
                .unwrap()
        });
        assert_eq!(done, Event::Cancelled);
        assert!(!marker.exists(), "cancel waited for the provider to finish");
        let pid: u32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(
            !crate::sys::pid_alive(pid),
            "the provider kept running after cancel"
        );
    }

    // Contract with the frontend: the events serialize with a `kind` tag in snake_case; the committed fixture is the
    // REAL serialization that `commitMessage.test.ts` parses with the app's own code.
    #[test]
    fn events_match_the_frontend_fixture() {
        let events = vec![
            Event::Start {
                agent: "omp".into(),
                scope: "staged",
                digest: DigestInfo {
                    summarized: 1,
                    withheld: 2,
                    truncated: 3,
                    omitted: 0,
                },
            },
            Event::Chunk {
                text: "feat: add".into(),
            },
            Event::Done {
                message: "feat: add x".into(),
            },
            Event::Error {
                message: "boom".into(),
            },
            Event::Cancelled,
        ];
        let value = serde_json::to_value(&events).unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/commit-msg-events.json");
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, value,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // Contract with the frontend: the codes of `commit_error` are the whole `message` of an error event, and
    // `commitErrorText` (desktop) words each one. The empty-message code is what the real run below produces.
    #[cfg(unix)]
    #[test]
    fn error_codes_match_the_frontend_fixture() {
        let d = tempfile::tempdir().unwrap();
        let (done, _) = run(fake(d.path(), "cat >/dev/null"), "x");
        assert!(
            matches!(&done, Event::Error { message } if message == commit_error::EMPTY_MESSAGE),
            "{done:?}"
        );
        let value = serde_json::to_value(commit_error::ALL).unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/commit-msg-errors.json");
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, value,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // Incident class: "nothing staged → describe all changes" silently skipped brand-new files, because `git diff HEAD`
    // never lists untracked ones. The real `git diff --no-index /dev/null <path>` must split under the NEW path.
    #[test]
    fn a_new_untracked_file_diffed_against_dev_null_splits_under_its_own_path() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "."]);
        std::fs::write(d.path().join("a.txt"), "a\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/new file.rs"), "fn brand_new() {}\n").unwrap();
        let diff = new_file_diff(d.path(), "src/new file.rs");
        let files = split_files(&diff);
        assert_eq!(files.len(), 1, "{diff}");
        assert_eq!(files[0].0, "src/new file.rs");
        let dg = digest(&diff, "all");
        assert!(dg.text.contains("+fn brand_new() {}"), "{}", dg.text);
    }

    // The secret filter must also catch an UNTRACKED secret in the fallback path, where a file never staged is the
    // most likely place for a stray `.env` to be.
    #[test]
    fn an_untracked_env_file_in_the_all_changes_fallback_is_withheld() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "."]);
        std::fs::write(d.path().join(".env"), "TOKEN=abc123\n").unwrap();
        let dg = digest(&new_file_diff(d.path(), ".env"), "all");
        assert_eq!(dg.withheld, [".env"]);
        assert!(!dg.text.contains("abc123"));
    }

    /// The real product diff of an untracked file (`--no-index` exits 1 when the files differ: the normal case).
    fn new_file_diff(dir: &Path, path: &str) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(new_file_args(path))
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    /// The user's own git config that renames the diff prefixes: `c/ w/ i/` (`1/ 2/` for `--no-index`) and none.
    const PREFIX_CONFIGS: [(&str, &str); 2] =
        [("diff.mnemonicPrefix", "true"), ("diff.noprefix", "true")];

    /// A repo with one commit and `key=value` in ITS OWN config, whatever the machine's config says.
    fn repo_with(key: &str, value: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "."]);
        git(d.path(), &["config", key, value]);
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/a.rs"), "fn a() {}\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        d
    }

    // Regression (#414): with `diff.mnemonicPrefix` or `diff.noprefix` in the user's git config the digest misread
    // every path, so a `.env` was not withheld and its content went to the commit-message agent. These run the real
    // args the app uses (the constants above), not a hand-written copy.
    #[test]
    fn the_secret_filter_holds_whatever_prefix_the_users_git_config_asks_for() {
        for (key, value) in PREFIX_CONFIGS {
            // 1. Staged.
            let d = repo_with(key, value);
            std::fs::write(d.path().join(".env"), "TOKEN=staged-secret\n").unwrap();
            std::fs::write(d.path().join("src/ok.rs"), "fn ok() {}\n").unwrap();
            git(d.path(), &["add", "-A"]);
            let diff = git(d.path(), &STAGED_CHANGES);
            let paths: Vec<_> = split_files(&diff).into_iter().map(|(p, _)| p).collect();
            assert_eq!(paths, [".env", "src/ok.rs"], "{key}: {diff}");
            let dg = digest(&diff, "staged");
            assert_eq!(dg.withheld, [".env"], "{key}");
            assert!(dg.text.contains("fn ok()"), "{key}: {}", dg.text);
            assert!(!dg.text.contains("staged-secret"), "{key}: {}", dg.text);

            // 2. Nothing staged: tracked changes, then an untracked file through `--no-index`.
            let d = repo_with(key, value);
            std::fs::write(d.path().join("src/a.rs"), "fn a() { changed() }\n").unwrap();
            std::fs::write(d.path().join(".env"), "TOKEN=untracked-secret\n").unwrap();
            let mut all = git(d.path(), &ALL_CHANGES);
            all.push_str(&new_file_diff(d.path(), ".env"));
            let paths: Vec<_> = split_files(&all).into_iter().map(|(p, _)| p).collect();
            assert_eq!(paths, ["src/a.rs", ".env"], "{key}: {all}");
            let dg = digest(&all, "all");
            assert_eq!(dg.withheld, [".env"], "{key}");
            assert!(dg.text.contains("changed()"), "{key}: {}", dg.text);
            assert!(!dg.text.contains("untracked-secret"), "{key}: {}", dg.text);
        }
    }

    // Regression (#414): the Changes view parses the same output by path; the prefix must not rename its files.
    #[test]
    fn the_changes_view_names_files_whatever_prefix_the_users_git_config_asks_for() {
        for (key, value) in PREFIX_CONFIGS {
            let d = repo_with(key, value);
            std::fs::write(d.path().join("src/a.rs"), "fn a() { changed() }\n").unwrap();
            let unstaged = parse(&git(d.path(), &REVIEW_DIFF), false);
            assert_eq!(unstaged.len(), 1, "{key}");
            assert_eq!(unstaged[0].path, "src/a.rs", "{key}");
            assert_eq!(unstaged[0].status, "modified", "{key}");

            git(d.path(), &["mv", "src/a.rs", "src/b c.rs"]);
            let staged_args = [&REVIEW_DIFF[..], &["--cached"]].concat();
            let staged = parse(&git(d.path(), &staged_args), true);
            assert_eq!(staged.len(), 1, "{key}");
            assert_eq!(staged[0].path, "src/b c.rs", "{key}");
            assert_eq!(staged[0].status, "renamed", "{key}");
            assert_eq!(staged[0].old_path.as_deref(), Some("src/a.rs"), "{key}");
        }
    }

    // Regression (#414): with `diff.external` (difftastic and friends) git prints an untracked file's `--no-index`
    // diff with NO `diff --git` header, so its content was filed under the PREVIOUS file's section and a `.env`
    // reached the commit-message agent. The driver here prints the new file's raw content, like one that ignores
    // the header.
    #[cfg(unix)]
    #[test]
    fn an_external_diff_driver_in_the_users_git_config_cannot_hide_a_secret_from_the_filter() {
        let d = repo_with("diff.external", "cat \"$5\" #");
        std::fs::write(d.path().join("src/a.rs"), "fn a() { changed() }\n").unwrap();
        std::fs::write(d.path().join(".env"), "TOKEN=external-secret\n").unwrap();
        let mut all = git(d.path(), &ALL_CHANGES);
        all.push_str(&new_file_diff(d.path(), ".env"));
        let paths: Vec<_> = split_files(&all).into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, ["src/a.rs", ".env"], "{all}");
        let dg = digest(&all, "all");
        assert_eq!(dg.withheld, [".env"], "{all}");
        assert!(dg.text.contains("changed()"), "{}", dg.text);
        assert!(!dg.text.contains("external-secret"), "{}", dg.text);

        // The staged diff is read by the same rules.
        git(d.path(), &["add", "-A"]);
        let staged = git(d.path(), &STAGED_CHANGES);
        let dg = digest(&staged, "staged");
        assert_eq!(dg.withheld, [".env"], "{staged}");
        assert!(!dg.text.contains("external-secret"), "{}", dg.text);
    }

    // Regression (#414): moving a secret out of `.env` (`git mv .env notes.txt`) names the new path in the header
    // and shows the old lines as context; the section is the secret's content, so the old path counts too.
    #[test]
    fn a_file_renamed_out_of_a_secret_name_is_withheld() {
        let d = repo_with("diff.noprefix", "false");
        std::fs::write(
            d.path().join(".env"),
            "TOKEN=renamed-secret\nHOST=localhost\nPORT=8080\n",
        )
        .unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "env"]);
        git(d.path(), &["mv", ".env", "notes.txt"]);
        std::fs::write(
            d.path().join("notes.txt"),
            "TOKEN=renamed-secret\nHOST=localhost\nPORT=9090\n",
        )
        .unwrap();
        git(d.path(), &["add", "-A"]);
        let diff = git(d.path(), &STAGED_CHANGES);
        assert!(diff.contains("rename from .env"), "{diff}");
        let dg = digest(&diff, "staged");
        assert_eq!(dg.withheld, ["notes.txt"], "{diff}");
        assert!(!dg.text.contains("renamed-secret"), "{}", dg.text);
    }
}

#[cfg(test)]
mod status_json_tests {
    use super::*;
    use std::cell::Cell;
    use std::path::Path;

    /// `git -C dir args`, no ambient config; a non-zero exit is `Err(stderr)` like hostd's `git`.
    fn git_in(dir: &Path, args: &[&str]) -> Result<String, String> {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=a@b", "-c", "user.name=n"])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "protocol.file.allow=always",
            ])
            .args(args)
            .output()
            .unwrap();
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).into_owned())
        }
    }

    fn ok(dir: &Path, args: &[&str]) {
        git_in(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    }

    fn commit(dir: &Path, file: &str, body: &str) {
        std::fs::write(dir.join(file), body).unwrap();
        ok(dir, &["add", "--", file]);
        ok(dir, &["commit", "-qm", file]);
    }

    /// `status_json` over the real git; returns the value and how many git processes it spawned.
    fn run(dir: &Path) -> (Value, u32) {
        let spawns = Cell::new(0);
        let git = |a: &[&str]| {
            spawns.set(spawns.get() + 1);
            let r = git_in(dir, a);
            async move { r }
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let v = rt.block_on(status_json(dir, &git)).unwrap();
        (v, spawns.get())
    }

    /// main: c1 (pushed to origin) + c2 (local); `dev` = c1 + two commits, `branch.main.base = dev`.
    fn tracked_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let (origin, work) = (t.path().join("origin.git"), t.path().join("work"));
        std::fs::create_dir_all(&work).unwrap();
        ok(
            t.path(),
            &["init", "-q", "--bare", origin.to_str().unwrap()],
        );
        ok(&work, &["init", "-q", "."]);
        commit(&work, "a.txt", "1\n");
        ok(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        ok(&work, &["push", "-qu", "origin", "main"]);
        ok(&work, &["branch", "dev"]);
        ok(&work, &["checkout", "-q", "dev"]);
        commit(&work, "d1.txt", "x");
        commit(&work, "d2.txt", "x");
        ok(&work, &["checkout", "-q", "main"]);
        commit(&work, "c2.txt", "x");
        ok(&work, &["config", "branch.main.base", "dev"]);
        std::fs::write(work.join("a.txt"), "changed\n").unwrap();
        std::fs::write(work.join("new.txt"), "").unwrap();
        (t, work)
    }

    // Incident #307: every fs event ran status + `branch --show-current` + config + rev-list + rev-parse. The branch is
    // already in the status header and the git dir is a file read, so a refresh is status + config (+ rev-list when the
    // branch has a base): the same payload for fewer processes.
    #[test]
    fn a_tracked_branch_with_a_base_is_three_spawns_and_the_same_payload() {
        let (_t, work) = tracked_repo();
        let (v, spawns) = run(&work);
        assert_eq!(v["branch"], "main");
        assert_eq!(v["upstream"], "origin/main");
        assert_eq!(v["ab"], "+1 -0");
        assert_eq!(v["base"], "dev");
        assert_eq!(v["vs_base"], json!({"ahead": 1, "behind": 2}));
        assert_eq!(v["operation"], Value::Null);
        let by = |p: &str| {
            v["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["path"] == p)
                .cloned()
        };
        assert_eq!(by("a.txt").unwrap()["y"], "M");
        assert_eq!(by("new.txt").unwrap()["x"], "?");
        assert_eq!(spawns, 3, "status, config, rev-list");
    }

    #[test]
    fn a_branch_without_a_base_is_two_spawns() {
        let (_t, work) = tracked_repo();
        ok(&work, &["config", "--unset", "branch.main.base"]);
        let (v, spawns) = run(&work);
        assert_eq!(
            (v["base"].as_str(), v["vs_base"].is_null()),
            (Some("HEAD"), true)
        );
        assert_eq!(spawns, 2, "status, config");
    }

    #[test]
    fn a_detached_head_has_no_branch_base_or_upstream_and_is_one_spawn() {
        let (_t, work) = tracked_repo();
        ok(&work, &["checkout", "-q", "--detach"]);
        let (v, spawns) = run(&work);
        assert_eq!(v["branch"], "(detached)");
        assert_eq!((v["upstream"].is_null(), v["ab"].is_null()), (true, true));
        assert_eq!(v["base"], "HEAD");
        assert_eq!(spawns, 1, "nothing to ask but status");
    }

    #[test]
    fn an_initial_commit_still_names_its_branch() {
        let t = tempfile::tempdir().unwrap();
        ok(t.path(), &["init", "-q", "."]);
        std::fs::write(t.path().join("f"), "").unwrap();
        let (v, _) = run(t.path());
        assert_eq!(v["branch"], "main");
        assert_eq!((v["upstream"].is_null(), v["ab"].is_null()), (true, true));
        assert_eq!(v["base"], "HEAD");
        assert_eq!(v["entries"], json!([{"path": "f", "x": "?", "y": "?"}]));
    }

    /// A conflicting merge, in the checkout and in a linked worktree (where `.git` is a file).
    #[test]
    fn a_merge_in_progress_is_reported_in_a_checkout_and_in_a_linked_worktree() {
        let t = tempfile::tempdir().unwrap();
        let main = t.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        ok(&main, &["init", "-q", "."]);
        commit(&main, "f.txt", "base\n");
        ok(&main, &["branch", "other"]);
        let wt = t.path().join("wt");
        ok(
            &main,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feat"],
        );
        commit(&main, "f.txt", "main\n");
        ok(&main, &["checkout", "-q", "other"]);
        commit(&main, "f.txt", "other\n");
        ok(&main, &["checkout", "-q", "main"]);
        assert!(git_in(&main, &["merge", "other"]).is_err(), "must conflict");
        assert_eq!(run(&main).0["operation"], "merge");
        commit(&wt, "f.txt", "feat\n");
        assert!(git_in(&wt, &["merge", "other"]).is_err(), "must conflict");
        assert!(wt.join(".git").is_file());
        let (v, _) = run(&wt);
        assert_eq!(
            (v["operation"].as_str(), v["branch"].as_str()),
            (Some("merge"), Some("feat"))
        );
        assert_eq!(v["entries"][0]["conflict"], true);
    }

    // Contract with the frontend: the values of `OPERATIONS` and `FILE_STATUSES` are what `operationText` and
    // `statusText` (desktop) word. Every value the REAL producers make below is in the lists; the lists are the
    // fixture `__fixtures__/git-review-values.json` the desktop test reads.
    #[test]
    fn operation_and_file_status_values_match_the_frontend_fixture() {
        let t = tempfile::tempdir().unwrap();
        let main = t.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        ok(&main, &["init", "-q", "."]);
        commit(&main, "f.txt", "base\n");
        commit(&main, "gone.txt", "x\n");
        commit(&main, "moved.txt", "same content\nline two\nline three\n");
        ok(&main, &["branch", "other"]);
        commit(&main, "f.txt", "main\n");
        ok(&main, &["checkout", "-q", "other"]);
        commit(&main, "f.txt", "other\n");
        // A conflicting rebase and a conflicting merge, as real git leaves them.
        assert!(git_in(&main, &["rebase", "main"]).is_err(), "must conflict");
        let mut seen = vec![operation(&main).expect("a rebase is in progress")];
        ok(&main, &["rebase", "--abort"]);
        ok(&main, &["checkout", "-q", "main"]);
        assert!(git_in(&main, &["merge", "other"]).is_err(), "must conflict");
        seen.push(operation(&main).expect("a merge is in progress"));
        ok(&main, &["merge", "--abort"]);
        seen.sort();
        assert_eq!(seen, OPERATIONS.to_vec());

        // A modified, an added, a deleted and a renamed file, in one real `git diff -M`.
        std::fs::write(main.join("f.txt"), "edited\n").unwrap();
        std::fs::write(main.join("added.txt"), "new\n").unwrap();
        ok(&main, &["rm", "-q", "gone.txt"]);
        ok(&main, &["mv", "moved.txt", "renamed.txt"]);
        ok(&main, &["add", "-A"]);
        let diff = git_in(&main, &["diff", "--cached", "--no-color", "-U3", "-M"]).unwrap();
        let mut statuses: Vec<&str> = parse(&diff, true).iter().map(|f| f.status).collect();
        statuses.sort();
        assert_eq!(statuses, ["added", "deleted", "modified", "renamed"]);
        assert!(statuses.iter().all(|s| FILE_STATUSES.contains(s)));

        let value = json!({"operations": OPERATIONS, "statuses": FILE_STATUSES});
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/git-review-values.json");
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, value,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // The parser behind it, on the verbatim headers of `git status --porcelain=v2 -z --branch`.
    #[test]
    fn porcelain_v2_branch_headers() {
        let p = |s: &str| {
            let (b, u, ab, _) = parse_status(s);
            (b, u, ab)
        };
        let s = |x: &str| Some(x.to_string());
        assert_eq!(
            p(
                "# branch.oid e494ad1\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +3 -2\0"
            ),
            (s("main"), s("origin/main"), s("+3 -2"))
        );
        assert_eq!(
            p("# branch.oid e494ad1\0# branch.head feat/x\0"),
            (s("feat/x"), None, None)
        );
        assert_eq!(
            p("# branch.oid e494ad1\0# branch.head (detached)\0"),
            (s("(detached)"), None, None)
        );
        assert_eq!(
            p("# branch.oid (initial)\0# branch.head main\0"),
            (s("main"), None, None)
        );
        // Upstream set but gone: git prints the name and no `branch.ab`.
        assert_eq!(
            p("# branch.oid e494ad1\0# branch.head main\0# branch.upstream origin/gone\0"),
            (s("main"), s("origin/gone"), None)
        );
    }
}
