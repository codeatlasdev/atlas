//! AI commit message from the staged diff. The diff is collected with real git (through hostd `exec`),
//! reduced under a budget (lockfiles/binaries as names, secret-looking files left out, long files cut with a
//! note), put in a prompt with the repo's own conventions, and sent to the agent CLI the user picked, run
//! non-interactively. The reply streams back as events; nothing is ever committed here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use atlas_core::git_review::{
    ALL_CHANGES, Event, Provider, STAGED_CHANGES, build_prompt, commit_error, commit_rules, digest,
    new_file_args, provider_argv, provider_ids, resolve_provider, run_provider,
};

use crate::host::HostHub;
use crate::worktree::git;

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
    provider_ids()
        .filter(|a| {
            atlas_core::agents::get(a)
                .and_then(|d| crate::agent::registry::which(&d.bin))
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

/// The staged diff, or every uncommitted change when nothing is staged: `(diff, scope)`.
async fn collect_diff(
    hub: &HostHub,
    dir: &std::path::Path,
) -> Result<(String, &'static str), String> {
    let staged = git(hub, dir, &STAGED_CHANGES).await?;
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
        &[&["git", "-C", &dir_s][..], &new_file_args(path)].concat(),
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
        return Err(commit_error::SSH_HOST.into());
    }
    let agent = match provider_now() {
        Provider::Agent(a) => a,
        Provider::Off => return Err(commit_error::PROVIDER_OFF.into()),
        Provider::None => return Err("no_provider".into()),
    };
    let dir = PathBuf::from(&entry.root);
    let (diff, scope) = collect_diff(&hub, &dir).await?;
    if diff.trim().is_empty() {
        return Err(commit_error::NO_CHANGES.into());
    }
    let d = digest(&diff, scope);
    if d.text.trim().is_empty() && d.summarized.is_empty() {
        return Err(commit_error::ALL_SECRET.into());
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
    let mut argv: Vec<String> = provider_argv(agent).expect("known agent").to_vec();
    // The agent's own binary path (login-shell PATH), not whatever the app inherited.
    if let Some(bin) =
        atlas_core::agents::get(agent).and_then(|a| crate::agent::registry::which(&a.bin))
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
