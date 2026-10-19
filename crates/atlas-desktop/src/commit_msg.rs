//! AI commit message from the staged diff. The diff is collected with real git (through hostd `exec`),
//! reduced under a budget (lockfiles/binaries as names, secret-looking files left out, long files cut with a
//! note), put in a prompt with the repo's own conventions, and sent to the agent CLI the user picked, run
//! non-interactively. The reply streams back as events; nothing is ever committed here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use atlas_core::sys::CommandExt as _;

use crate::host::HostHub;
use crate::worktree::git;

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
        if is_secret(&path) {
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

/// How an agent CLI is run to answer one prompt and exit: `argv`, prompt on stdin, no tools, no saved session.
/// Flags verified against each CLI's `--help` (omp 18.11, claude 2.1, codex 0.162).
pub fn provider_argv(agent: &str) -> Option<Vec<&'static str>> {
    Some(match agent {
        "omp" => vec![
            "omp",
            "-p",
            "--no-session",
            "--no-tools",
            "--no-skills",
            "--no-rules",
            "--no-extensions",
            "--no-lsp",
            "--no-title",
        ],
        "claude" => vec!["claude", "-p", "--no-session-persistence", "--tools", ""],
        "codex" => vec![
            "codex",
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "-s",
            "read-only",
            "--color",
            "never",
            "-",
        ],
        _ => return None,
    })
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
    let known = |id: &str| {
        installed
            .iter()
            .copied()
            .find(|i| *i == id)
            .and_then(|i| provider_argv(i).map(|a| a[0]))
    };
    let pick = |id: &str| ["omp", "claude", "codex"].into_iter().find(|k| *k == id);
    match setting {
        Some("off") => Provider::Off,
        Some(id) if id != "auto" && !id.is_empty() => {
            match pick(id).filter(|k| known(k).is_some()) {
                Some(k) => Provider::Agent(k),
                None => Provider::None,
            }
        }
        _ => default_agent
            .and_then(pick)
            .filter(|k| known(k).is_some())
            .or_else(|| {
                ["omp", "claude", "codex"]
                    .into_iter()
                    .find(|k| known(k).is_some())
            })
            .map_or(Provider::None, Provider::Agent),
    }
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
            message: "the provider returned no message".into(),
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

// ---------- Tauri commands ----------

/// Running generations by request id; firing the sender cancels the run.
#[derive(Default)]
pub struct Runs(Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>);

fn setting(key: &str) -> Option<String> {
    crate::shell_env::load_settings()
        .extra
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn installed() -> Vec<&'static str> {
    ["omp", "claude", "codex"]
        .into_iter()
        .filter(|a| {
            crate::agent::registry::get(a)
                .and_then(|d| crate::agent::registry::which(d.bin))
                .is_some()
        })
        .collect()
}

fn provider_now() -> Provider {
    resolve_provider(
        setting("gitCommitAgent").as_deref(),
        setting("defaultAgent").as_deref(),
        &installed(),
    )
}

/// What the commit box needs to know before it shows the button: `{state: "ready"|"off"|"none", agent?}`.
#[tauri::command]
pub fn commit_msg_provider() -> Value {
    match provider_now() {
        Provider::Agent(a) => json!({"state": "ready", "agent": a}),
        Provider::Off => json!({"state": "off"}),
        Provider::None => json!({"state": "none"}),
    }
}

/// Subjects of the last commits, newest first (style to follow); empty on a repo with no commits.
async fn recent_subjects(hub: &HostHub, dir: &std::path::Path) -> Vec<String> {
    git(hub, dir, &["log", "-n", "12", "--no-merges", "--format=%s"])
        .await
        .map(|o| o.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Git args for the "nothing is staged" fallback: tracked changes and, through intent-to-add, new files.
pub const ALL_CHANGES: [&str; 5] = ["diff", "HEAD", "--no-color", "-M", "--no-ext-diff"];

/// The staged diff, or every uncommitted change when nothing is staged: `(diff, scope)`.
async fn collect_diff(
    hub: &HostHub,
    dir: &std::path::Path,
) -> Result<(String, &'static str), String> {
    let staged = git(
        hub,
        dir,
        &["diff", "--cached", "--no-color", "-M", "--no-ext-diff"],
    )
    .await?;
    if !staged.trim().is_empty() {
        return Ok((staged, "staged"));
    }
    // New files are not in `git diff HEAD`; list them and diff each against /dev/null so they count as additions
    // without touching the index.
    let mut all = git(hub, dir, &ALL_CHANGES).await.unwrap_or_default();
    let untracked = git(
        hub,
        dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )
    .await
    .unwrap_or_default();
    for path in untracked.split('\0').filter(|p| !p.is_empty()) {
        // `--no-index` exits 1 when files differ, which `git()` reports as an error; the diff is in the error-free
        // path only when it succeeds, so use the raw form below.
        if let Some(section) = new_file_diff(hub, dir, path).await {
            all.push_str(&section);
        }
    }
    Ok((all, "all"))
}

/// `git diff --no-index /dev/null <path>`: exit 1 means "differs", the normal case here.
async fn new_file_diff(hub: &HostHub, dir: &std::path::Path, path: &str) -> Option<String> {
    let dir_s = dir.to_string_lossy();
    let o = crate::worktree::exec(
        hub,
        dir,
        &[
            "git",
            "-C",
            &dir_s,
            "diff",
            "--no-index",
            "--no-color",
            "--",
            "/dev/null",
            path,
        ],
        60_000,
    )
    .await
    .ok()?;
    (matches!(o.code, Some(0) | Some(1)) && !o.stdout.is_empty()).then_some(o.stdout)
}

/// Start generating. Returns the request id at once; progress arrives as `commit-msg://<id>` events
/// ([`Event`]), the last one being `done`, `error` or `cancelled`. Nothing is committed.
#[tauri::command]
pub async fn commit_msg_generate(
    app: AppHandle,
    hub: State<'_, Arc<HostHub>>,
    runs: State<'_, Runs>,
    id: String,
    instruction: Option<String>,
) -> Result<String, String> {
    let entry = crate::registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown worktree {id}"))?;
    if entry.is_remote() {
        return Err("Generating a commit message is not available on SSH hosts yet: the agent would have to run there.".into());
    }
    let agent = match provider_now() {
        Provider::Agent(a) => a,
        Provider::Off => return Err("AI commit messages are turned off in Settings → Git".into()),
        Provider::None => return Err("no_provider".into()),
    };
    let dir = PathBuf::from(&entry.root);
    let (diff, scope) = collect_diff(&hub, &dir).await?;
    if diff.trim().is_empty() {
        return Err("Nothing to describe: no staged or uncommitted changes".into());
    }
    let d = digest(&diff, scope);
    if d.text.trim().is_empty() && d.summarized.is_empty() {
        return Err(
            "Every changed file looks like a secret, so none was sent. Write the message yourself."
                .into(),
        );
    }
    let docs: Vec<(&str, String)> = ["CONTRIBUTING.md", "AGENTS.md"]
        .into_iter()
        .filter_map(|n| std::fs::read_to_string(dir.join(n)).ok().map(|t| (n, t)))
        .collect();
    let prompt = build_prompt(
        &commit_rules(&docs),
        &recent_subjects(&hub, &dir).await,
        instruction.as_deref().unwrap_or(""),
        &d,
    );
    let mut argv: Vec<String> = provider_argv(agent)
        .expect("known agent")
        .into_iter()
        .map(str::to_string)
        .collect();
    // The agent's own binary path (login-shell PATH), not whatever the app inherited.
    if let Some(bin) =
        crate::agent::registry::get(agent).and_then(|a| crate::agent::registry::which(a.bin))
    {
        argv[0] = bin.to_string_lossy().into_owned();
    }
    // Settings → Agents extra arguments (e.g. --model) apply here too, after the fixed flags.
    if let Some(extra) = crate::shell_env::load_settings()
        .extra
        .get("agentArgs")
        .and_then(|m| m.get(agent))
        .and_then(Value::as_str)
    {
        argv.extend(extra.split_whitespace().map(str::to_string));
    }
    let env = crate::shell_env::vars_async().await;
    let request = atlas_host::paths::new_id();
    let (tx, rx) = tokio::sync::oneshot::channel();
    runs.0.lock().insert(request.clone(), tx);
    let topic = format!("commit-msg://{request}");
    let _ = app.emit(
        &topic,
        Event::Start {
            agent: agent.to_string(),
            scope,
            digest: (&d).into(),
        },
    );
    let (app2, request2) = (app.clone(), request.clone());
    tauri::async_runtime::spawn(async move {
        let emit_app = app2.clone();
        let emit_topic = topic.clone();
        let last = run_provider(&argv, &dir, &env, &prompt, rx, move |e| {
            let _ = emit_app.emit(&emit_topic, e);
        })
        .await;
        app2.state::<Runs>().0.lock().remove(&request2);
        let _ = app2.emit(&topic, last);
    });
    Ok(request)
}

/// Stop a running generation; a finished or unknown id is fine.
#[tauri::command]
pub fn commit_msg_cancel(runs: State<'_, Runs>, request: String) {
    if let Some(tx) = runs.0.lock().remove(&request) {
        let _ = tx.send(());
    }
}

#[cfg(test)]
mod tests {
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
        let diff = git(d.path(), &["diff", "--cached", "--no-color"]);
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
        let diff = git(d.path(), &["diff", "--cached", "--no-color", "-M"]);
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
        let omp = provider_argv("omp").unwrap();
        assert_eq!(omp[0], "omp");
        for f in ["-p", "--no-session", "--no-tools"] {
            assert!(omp.contains(&f), "omp lacks {f}: {omp:?}");
        }
        let claude = provider_argv("claude").unwrap();
        assert_eq!(claude[0], "claude");
        assert!(claude.contains(&"-p") && claude.contains(&"--no-session-persistence"));
        let at = claude
            .iter()
            .position(|a| *a == "--tools")
            .expect("--tools");
        assert_eq!(
            claude[at + 1],
            "",
            "an empty --tools list disables every tool"
        );
        let codex = provider_argv("codex").unwrap();
        assert_eq!(&codex[..2], ["codex", "exec"]);
        for f in ["--ephemeral", "--skip-git-repo-check", "-"] {
            assert!(codex.contains(&f), "codex lacks {f}: {codex:?}");
        }
        let at = codex.iter().position(|a| *a == "-s").expect("-s");
        assert_eq!(codex[at + 1], "read-only");
        assert_eq!(provider_argv("nope"), None);
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

    // Cancel mid-stream: the child is killed (not left running) and the result is Cancelled, no message.
    #[cfg(unix)]
    #[test]
    fn cancel_kills_the_provider_mid_stream() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("finished");
        let argv = fake(
            d.path(),
            &format!(
                "cat >/dev/null; printf 'feat: start'; sleep 30; touch {}",
                marker.display()
            ),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let started = std::time::Instant::now();
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
            run.await.unwrap()
        });
        assert_eq!(done, Event::Cancelled);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "cancel must not wait for the 30 s sleep"
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!marker.exists(), "the provider kept running after cancel");
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
        let o = Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args([
                "diff",
                "--no-index",
                "--no-color",
                "--",
                "/dev/null",
                "src/new file.rs",
            ])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(1),
            "--no-index exits 1 when the files differ"
        );
        let diff = String::from_utf8_lossy(&o.stdout).into_owned();
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
        let o = Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args([
                "diff",
                "--no-index",
                "--no-color",
                "--",
                "/dev/null",
                ".env",
            ])
            .output()
            .unwrap();
        let dg = digest(&String::from_utf8_lossy(&o.stdout), "all");
        assert_eq!(dg.withheld, [".env"]);
        assert!(!dg.text.contains("abc123"));
    }
}
