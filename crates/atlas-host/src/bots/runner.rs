//! One-at-a-time FIFO task runner per bot.
//!
//! Each bot has a `BotRunner` with an in-memory queue and one background worker
//! task. The worker pops tasks, spawns a PTY, waits for the agent to finish (or
//! times out after 2 h), and updates the task state.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;

use crate::bots::store::{Approval, Bot, BotStore, EngineConfig, Step, Task, TaskState};
use crate::paths;
use crate::protocol::{Mode, PtyCreate, Scope};
use crate::pty::Session;
use crate::server::Hub;

/// 2-hour hard timeout per bot task.
const BOT_TASK_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Test-only: serialise tests that need to override the spawned agent binary.
/// Hold via `.lock()` for the entire test to prevent races between parallel tests.
#[cfg(all(test, unix))]
pub(crate) static TEST_BIN_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

/// Test-only: absolute path to a fake agent binary, overrides the agent def's bin.
#[cfg(test)]
static TEST_AGENT_BIN: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// Set (or clear) the per-test agent binary override.  Always acquire
/// `TEST_BIN_LOCK` first to prevent concurrent tests from racing.
#[cfg(all(test, unix))]
pub(crate) fn set_test_agent_bin(bin: Option<&str>) {
    *TEST_AGENT_BIN.lock() = bin.map(str::to_string);
}

const ATLAS_ACTION_PREFIX: &str = "ATLAS_ACTION: ";

/// Test-only: if set, forces the GitHub auth pre-check to return Some(reason).
#[cfg(test)]
pub(crate) static TEST_FORCE_GITHUB_AUTH: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Test-only: override the result of is_github_logged_in().
#[cfg(test)]
pub(crate) static TEST_GITHUB_LOGGED_IN: parking_lot::Mutex<Option<bool>> =
    parking_lot::Mutex::new(None);

/// Test-only: absolute path to a fake summarizer binary.
#[cfg(test)]
pub(crate) static TEST_SUMMARIZER_BIN: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Set (or clear) the per-test summarizer binary override. Its tests run a shell
/// script as the fake summarizer, so they (and this) are unix only.
#[cfg(all(test, unix))]
pub(crate) fn set_test_summarizer_bin(bin: Option<&str>) {
    *TEST_SUMMARIZER_BIN.lock() = bin.map(str::to_string);
}

// ─── per-bot runner state ─────────────────────────────────────────────────────

struct BotRunner {
    queue: VecDeque<Task>,
    /// task_id of the currently executing task.
    running: Option<String>,
    notify: Arc<tokio::sync::Notify>,
    /// True while a tokio worker task is alive for this bot.
    worker_active: bool,
}

impl BotRunner {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            running: None,
            notify: Arc::new(tokio::sync::Notify::new()),
            worker_active: false,
        }
    }
}

// ─── RunnerMap ────────────────────────────────────────────────────────────────

pub struct RunnerMap(Mutex<HashMap<String, BotRunner>>);

impl RunnerMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Return the task_id currently running for `bot_id`, if any.
    pub fn running_task(&self, bot_id: &str) -> Option<String> {
        self.0.lock().get(bot_id)?.running.clone()
    }

    /// Count of tasks waiting in the queue for `bot_id` (not including the running one).
    pub fn queue_len(&self, bot_id: &str) -> usize {
        self.0
            .lock()
            .get(bot_id)
            .map(|r| r.queue.len())
            .unwrap_or(0)
    }
}

impl Default for RunnerMap {
    fn default() -> Self {
        Self::new()
    }
}

// ─── public API ──────────────────────────────────────────────────────────────

/// Append `task` to the bot's queue and start the worker if needed.
pub(crate) fn enqueue(hub: &Hub, store: &BotStore, bot: &Bot, task: Task) {
    let bot_id = bot.id.clone();
    let notify;
    let should_spawn;
    {
        let mut map = hub.bots.runners.0.lock();
        let runner = map.entry(bot_id.clone()).or_insert_with(BotRunner::new);
        runner.queue.push_back(task);
        notify = runner.notify.clone();
        should_spawn = !runner.worker_active;
        if should_spawn {
            runner.worker_active = true;
        }
    }
    notify.notify_one();
    let hub_arc_for_spawn = should_spawn.then(|| hub.bots.hub_arc()).flatten();
    if let Some(hub_arc) = hub_arc_for_spawn {
        let bot = bot.clone();
        tokio::spawn(async move {
            run_worker(hub_arc, bot).await;
        });
    }
    let _ = store; // store already written by caller before enqueue
}

/// Cancel a task: remove from queue if pending, kill pty if running.
pub(crate) fn cancel(
    hub: &Hub,
    store: &BotStore,
    bot_id: &str,
    task_id: &str,
) -> Result<(), String> {
    let is_running;
    {
        let mut map = hub.bots.runners.0.lock();
        if let Some(runner) = map.get_mut(bot_id) {
            // Remove from queue if present.
            let before = runner.queue.len();
            runner.queue.retain(|t| t.id != task_id);
            let removed_from_queue = runner.queue.len() < before;
            is_running = runner.running.as_deref() == Some(task_id);
            if !removed_from_queue && !is_running {
                return Err(format!("task {task_id} not found for bot {bot_id}"));
            }
        } else {
            return Err(format!("no runner for bot {bot_id}"));
        }
    }

    // Update task state to Cancelled in the store.
    update_task(hub, store, bot_id, task_id, |t| {
        t.state = TaskState::Cancelled;
        t.finished_at = Some(now_s());
    });

    // Kill the pty if the task is currently running.
    if is_running {
        let pane_key = format!("bot:{bot_id}:{task_id}");
        let sessions: Vec<Arc<Session>> = hub.sessions.lock().values().cloned().collect();
        if let Some(s) = sessions
            .into_iter()
            .find(|s| s.meta.scope.pane_key.as_deref() == Some(&pane_key) && s.exited().is_none())
        {
            let s = s.clone();
            tokio::spawn(async move { s.kill().await });
        }
    }

    // Wake any action or github-connect waiter for this task.
    hub.bots.notify_github_connected(); // wakes github_connect waiters (they check cancel state)
    hub.bots.cancel_action(task_id); // drops action waiter sender → receiver gets Err
    Ok(())
}

// ─── ATLAS_ACTION parsing and waiting ────────────────────────────────────────

enum WaitResult {
    Connected,
    Cancelled,
    Timeout,
}
enum AnswerResult {
    Answer(String),
    Cancelled,
    Timeout,
}

/// Parse the last `ATLAS_ACTION: {…}` line in `text`.
/// Unknown kinds fall back to `Input`.
pub(crate) fn parse_atlas_action(text: &str) -> Option<crate::bots::store::TaskAction> {
    use crate::bots::store::TaskAction;
    let line = text
        .lines()
        .rev()
        .find(|l| l.trim().starts_with(ATLAS_ACTION_PREFIX))?;
    let json_str = line.trim().strip_prefix(ATLAS_ACTION_PREFIX)?;
    if let Ok(action) = serde_json::from_str::<TaskAction>(json_str) {
        return Some(action);
    }
    // Unknown kind → fall back to Input
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let label = v["label"].as_str().unwrap_or("Response needed").to_string();
    let reason = v["reason"].as_str().unwrap_or("").to_string();
    let placeholder = v["placeholder"].as_str().map(str::to_string);
    Some(TaskAction::Input {
        label,
        reason,
        placeholder,
    })
}

/// The reply as people should read it: without the `ATLAS_ACTION:` line, which is for Atlas (it becomes the card).
pub(crate) fn visible_reply(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim().starts_with(ATLAS_ACTION_PREFIX))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// The agent's own title for what it is doing, fit for the live card. omp first titles a session with the start of
/// our instructions ("You are carlos. …") and decorates real titles with an emoji ("🏪 STORE: …"); neither belongs on
/// the card.
pub(crate) fn clean_title(raw: &str) -> Option<String> {
    let t = raw
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim();
    (!t.is_empty() && !t.starts_with("You are ")).then(|| t.to_string())
}

/// What the chat shows as the user's own message after an action: what they picked or typed. Secrets, GitHub and
/// sign-ins resolve with a system note ("The secret X is now set") that is not something the user said.
pub(crate) fn user_answer(
    action: &crate::bots::store::TaskAction,
    resolved: &str,
) -> Option<String> {
    use crate::bots::store::TaskAction;
    matches!(action, TaskAction::Choose { .. } | TaskAction::Input { .. })
        .then(|| resolved.to_string())
}

/// The next turn after the user answered an action. A terminal agent starts fresh each run, so it gets its own last
/// reply and the answer; re-sending only the original prompt made it ask the same question again.
pub(crate) fn continuation_prompt(base: &str, reply: &str, answer: &str) -> String {
    format!(
        "{base}\n\n\
         You already replied, and asked the user a question:\n\
         ---\n{said}\n---\n\
         The user answered: {answer}\n\
         Continue from that answer. Do not ask it again.",
        said = visible_reply(reply),
    )
}

fn default_cheap_model(engine_kind: &str) -> &'static str {
    match engine_kind {
        "claude" => "claude-haiku-4-5",
        _ => "cc/claude-haiku-5-5",
    }
}

async fn summarize(
    text: &str,
    engine_kind: &str,
    model: &str,
    agent_env: &std::collections::BTreeMap<String, String>,
    bot_dir: &std::path::Path,
) -> Option<String> {
    use std::process::Stdio;
    use tokio::time::timeout;
    const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(25);
    let system = "Same language as the reply. \
        Keep every file path, command, number, error and every question or decision asked of the user. \
        Say what was done, what changed, what is left, what the user must do. \
        No new facts, no preamble, no emojis. At most 6 short lines or bullets.";
    let prompt = format!("{system}\n\n---\n{text}");

    #[cfg(test)]
    {
        let bin_opt = TEST_SUMMARIZER_BIN.lock().clone();
        if let Some(bin) = bin_opt {
            let out = timeout(
                TIMEOUT,
                tokio::process::Command::new(bin)
                    .arg(&prompt)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output(),
            )
            .await
            .ok()?
            .ok()?;
            if !out.status.success() {
                return None;
            }
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() || s.len() >= text.len() {
                return None;
            }
            return Some(s);
        }
    }

    let mut cmd = match engine_kind {
        "omp" => {
            let mut c = tokio::process::Command::new("omp");
            // A one-shot rewrite: no tools, LSP, extensions, skills or rules to load.
            c.args(["-p", &prompt, "--model", model])
                .args([
                    "--no-tools",
                    "--no-lsp",
                    "--no-extensions",
                    "--no-skills",
                    "--no-rules",
                ])
                .args(crate::bots::engine::omp::overlay_args(bot_dir));
            c
        }
        "claude" => {
            let mut c = tokio::process::Command::new("claude");
            c.args(["-p", &prompt, "--model", model, "--output-format", "text"]);
            c
        }
        _ => return None,
    };
    for (k, v) in agent_env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    // Every failure is logged: a silent fallback hid for days that every summary timed out (omp waited 25 s on D-Bus).
    let out = match timeout(TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!("atlas bots: summary: could not start {engine_kind}: {e}");
            return None;
        }
        Err(_) => {
            eprintln!(
                "atlas bots: summary: {engine_kind} took over {}s, sending the full reply",
                TIMEOUT.as_secs()
            );
            return None;
        }
    };
    if !out.status.success() {
        eprintln!(
            "atlas bots: summary: {engine_kind} exited with {}",
            out.status
        );
        return None;
    }
    let summary = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if summary.is_empty() || summary.len() >= text.len() {
        return None;
    }
    Some(summary)
}

async fn maybe_summarize(
    hub: &Hub,
    bot: &Bot,
    outcome: Result<String, String>,
    agent_env: &std::collections::BTreeMap<String, String>,
) -> (Result<String, String>, Option<String>, bool) {
    const THRESHOLD: usize = 600;
    let text = match &outcome {
        Ok(t) => t.clone(),
        Err(_) => return (outcome, None, false),
    };
    let visible = visible_reply(&text);
    if visible.len() <= THRESHOLD {
        return (outcome, None, false);
    }
    let crew = hub.bots.store.load_crew_settings();
    if !crew.short_replies {
        return (outcome, None, false);
    }
    if bot.short_replies == Some(false) {
        return (outcome, None, false);
    }
    let engine_kind = bot
        .engine
        .as_ref()
        .map(|e| e.kind.as_str())
        .unwrap_or(&bot.agent);
    let model = crew
        .short_replies_model
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_cheap_model(engine_kind));
    let bot_dir = hub.bots.store.bot_dir(&bot.id);
    match summarize(&visible, engine_kind, model, agent_env, &bot_dir).await {
        Some(summary) => (Ok(summary), Some(visible), true),
        None => (outcome, None, false),
    }
}

/// Returns Some(reason) if bot has a github.com Git project AND gh is not logged in.
async fn needs_github_auth(bot: &crate::bots::store::Bot) -> Option<String> {
    use crate::bots::store::ProjectRef;
    #[cfg(test)]
    if let Some(r) = TEST_FORCE_GITHUB_AUTH.lock().clone() {
        return Some(r);
    }
    let url = match bot.project.as_ref()? {
        ProjectRef::Git { url, .. } => url.clone(),
        ProjectRef::Path { .. } => return None,
    };
    if !url.contains("github.com") {
        return None;
    }
    if is_github_logged_in().await {
        return None;
    }
    let parts: Vec<&str> = url
        .trim_end_matches(".git")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let slug = if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        url.clone()
    };
    Some(format!("to clone {slug}"))
}

async fn is_github_logged_in() -> bool {
    #[cfg(test)]
    if let Some(v) = *TEST_GITHUB_LOGGED_IN.lock() {
        return v;
    }
    tokio::process::Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn wait_for_github_connect(hub: &Arc<Hub>, bot_id: &str, task_id: &str) -> WaitResult {
    if is_github_logged_in().await {
        return WaitResult::Connected;
    }
    let deadline = tokio::time::Instant::now() + BOT_TASK_TIMEOUT;
    loop {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        hub.bots.github_connect_waiters.lock().push(tx);
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return WaitResult::Timeout;
        }
        let _ = tokio::time::timeout(remaining.min(Duration::from_secs(30)), rx).await;
        let tasks = hub.bots.store.read_tasks(bot_id, 10_000);
        if tasks
            .iter()
            .any(|t| t.id == task_id && t.state == crate::bots::store::TaskState::Cancelled)
        {
            return WaitResult::Cancelled;
        }
        if is_github_logged_in().await {
            return WaitResult::Connected;
        }
    }
}

async fn wait_for_action_answer(hub: &Arc<Hub>, bot_id: &str, task_id: &str) -> AnswerResult {
    let rx = hub.bots.wait_action(task_id.to_string());
    let deadline = tokio::time::sleep(BOT_TASK_TIMEOUT);
    tokio::pin!(deadline);
    tokio::pin!(rx);
    loop {
        let cancel_check = tokio::time::sleep(Duration::from_secs(30));
        tokio::select! {
            biased;
            _ = &mut deadline => return AnswerResult::Timeout,
            result = &mut rx => match result {
                Ok(value) => return AnswerResult::Answer(value),
                Err(_) => {
                    let tasks = hub.bots.store.read_tasks(bot_id, 10_000);
                    if tasks.iter().any(|t| t.id == task_id && t.state == crate::bots::store::TaskState::Cancelled) {
                        return AnswerResult::Cancelled;
                    }
                    return AnswerResult::Timeout;
                }
            },
            _ = cancel_check => {
                let tasks = hub.bots.store.read_tasks(bot_id, 10_000);
                if tasks.iter().any(|t| t.id == task_id && t.state == crate::bots::store::TaskState::Cancelled) {
                    return AnswerResult::Cancelled;
                }
            }
        }
    }
}

/// Park `task` as WaitingAction with `action`, store, and broadcast.
fn park_task(
    hub: &Hub,
    bot: &crate::bots::store::Bot,
    task: &mut Task,
    action: crate::bots::store::TaskAction,
) {
    task.state = TaskState::WaitingAction;
    task.action = Some(action);
    let _ = hub.bots.store.append_task(&bot.id, task);
    broadcast_task(hub, &bot.id, task);
}

/// Wait for the action to be resolved; return the follow-up message or None (cancelled/timeout).
async fn resolve_action(
    hub: &Arc<Hub>,
    bot_id: &str,
    task_id: &str,
    action: &crate::bots::store::TaskAction,
) -> Option<String> {
    use crate::bots::store::TaskAction;
    match action {
        TaskAction::ConnectGithub { .. } => {
            match wait_for_github_connect(hub, bot_id, task_id).await {
                WaitResult::Connected => Some("GitHub is connected now. Continue.".to_string()),
                _ => None,
            }
        }
        TaskAction::ProvideSecret { .. } => {
            match wait_for_action_answer(hub, bot_id, task_id).await {
                AnswerResult::Answer(received_name) => {
                    Some(format!("The secret {received_name} is now set. Continue."))
                }
                _ => None,
            }
        }
        TaskAction::Input { .. } => match wait_for_action_answer(hub, bot_id, task_id).await {
            AnswerResult::Answer(text) => Some(text),
            _ => None,
        },
        TaskAction::Choose { .. } => match wait_for_action_answer(hub, bot_id, task_id).await {
            AnswerResult::Answer(choice) => Some(choice),
            _ => None,
        },
        TaskAction::SignInEngine { engine } => {
            let engine = engine.clone();
            match wait_for_action_answer(hub, bot_id, task_id).await {
                AnswerResult::Answer(_) => {
                    Some(format!("The sign-in for {engine} is complete. Continue."))
                }
                _ => None,
            }
        }
        TaskAction::AddTrigger { trigger, .. } => {
            // Validate the trigger before waiting; if invalid, resolve immediately so
            // the agent can correct and retry (no user card is shown for a bad proposal).
            if let Err(e) = crate::bots::triggers::validate_trigger(&crate::bots::store::Trigger {
                id: String::new(),
                kind: trigger.clone(),
                enabled: true,
                prompt: String::new(),
                last_fired: None,
                tz: None,
                next_fire: None,
            }) {
                return Some(format!(
                    "The automation was invalid: {e}. Please fix it and try again."
                ));
            }
            match wait_for_action_answer(hub, bot_id, task_id).await {
                AnswerResult::Answer(v) if v == "add" => {
                    Some("The user added the automation.".to_string())
                }
                AnswerResult::Answer(_) => Some("The user did not add the automation.".to_string()),
                _ => None,
            }
        }
    }
}

// ─── worker loop ─────────────────────────────────────────────────────────────

async fn run_worker(hub: Arc<Hub>, bot: Bot) {
    loop {
        // Pop next task from the queue.
        let task = {
            let mut map = hub.bots.runners.0.lock();
            let runner = map.get_mut(&bot.id).expect("runner entry always present");
            match runner.queue.pop_front() {
                Some(t) => {
                    runner.running = Some(t.id.clone());
                    t
                }
                None => {
                    runner.worker_active = false;
                    return;
                }
            }
        };

        let task_id = task.id.clone();
        run_one(&hub, &bot, task).await;

        // Clear running.
        {
            let mut map = hub.bots.runners.0.lock();
            if let Some(runner) = map.get_mut(&bot.id) {
                runner.running = None;
            }
        }

        // If the queue is now empty, wait for a notification before checking again.
        let notify = {
            let map = hub.bots.runners.0.lock();
            map.get(&bot.id)
                .filter(|r| r.queue.is_empty())
                .map(|r| r.notify.clone())
        };
        if let Some(n) = notify {
            n.notified().await;
            // Re-check: there may be work now.
        }
        let _ = task_id;
    }
}

async fn run_one(hub: &Hub, bot: &Bot, task: Task) {
    if let Some(engine_cfg) = &bot.engine {
        run_one_conversational(hub, bot, engine_cfg, task).await;
    } else {
        run_one_pty(hub, bot, task).await;
    }
}

/// Where the bot's agent works: its project (cloned or fetched now, so after any GitHub login it waited for), else its
/// own folder. Both runners use this; the terminal one used to skip it and start every agent in hostd's directory.
async fn prepare_work_dir(hub: &Hub, bot: &Bot, task: &mut Task) -> Result<String, String> {
    let dir = if let Some(proj) = &bot.project {
        // Not checked out yet (a first task, or one sent while the background clone runs): say so on the task.
        let work = hub.bots.dir.join("bots").join(&bot.id).join("work");
        if let crate::bots::store::ProjectRef::Git { url, .. } = proj
            && !work.join(".git").exists()
        {
            task.preparing = Some(
                url.trim_end_matches(".git")
                    .trim_start_matches("https://github.com/")
                    .to_string(),
            );
            let _ = hub.bots.store.append_task(&bot.id, task);
            broadcast_task(hub, &bot.id, task);
        }
        let p = crate::bots::project::prepare(proj, &bot.id, &hub.bots.dir).await;
        if task.preparing.take().is_some() {
            let _ = hub.bots.store.append_task(&bot.id, task);
            broadcast_task(hub, &bot.id, task);
        }
        let p = p.map_err(|e| format!("project prepare: {e}"))?;
        let _ = crate::bots::project::fetch(&p).await; // non-fatal
        p.to_string_lossy().to_string()
    } else {
        bot.workdir.clone().unwrap_or_else(|| {
            hub.bots
                .store
                .bot_dir(&bot.id)
                .join("work")
                .to_string_lossy()
                .to_string()
        })
    };
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir)
}

/// The instructions every agent gets. It says where it runs: the bot lives on its own server, so the user's computer
/// (its paths, mounts, branches) is out of reach; without this an agent kept asking to "mount the Mac folder".
fn task_prompt(
    bot: &Bot,
    work_dir: &str,
    memory: &std::path::Path,
    prompt: &str,
    tz: Option<&str>,
) -> String {
    let project = match &bot.project {
        Some(crate::bots::store::ProjectRef::Git { url, branch }) => format!(
            "Your project is {url}{b}, already checked out in {work_dir}, your working directory. Work there.\n",
            b = branch
                .as_deref()
                .map(|b| format!(" (branch {b})"))
                .unwrap_or_default(),
        ),
        Some(crate::bots::store::ProjectRef::Path { .. }) => {
            format!("Your project is in {work_dir}, your working directory. Work there.\n")
        }
        None => format!("Your working directory is {work_dir}.\n"),
    };
    let tz_line = if let Some(tz_str) = tz {
        if let Ok(zone) = tz_str.parse::<chrono_tz::Tz>() {
            let local = chrono::Utc::now().with_timezone(&zone);
            format!(
                "The user's timezone is {tz_str}. Current local time there: {}.\n",
                local.format("%Y-%m-%d %H:%M %Z")
            )
        } else {
            format!("The user's timezone is {tz_str}.\n")
        }
    } else {
        String::new()
    };
    format!(
        "You are {name}. {role}\n\
         You run on your own server, not on the user's computer. You cannot see the user's machine, its files or \
         what is checked out there, and you never ask for it to be mounted or shared.\n\
         {project}\
         Your memory file is {mem}. Read it first. When you learn something durable \
         about this work or this user, append it there.\n\
         {tz_line}\
         Never use emojis in replies.\n\
         If you need the user to do something (connect to GitHub, provide a secret, answer a \
         question, make a choice, sign in to a service, or set up an automation), do not refuse \
         and do not ask for keys or tokens in prose. Instead, end your turn with exactly one line:\n\
         ATLAS_ACTION: {{\"kind\":\"<kind>\", ...}}\n\
         Supported kinds: connect_github {{reason}}, provide_secret {{name, label, reason}}, \
         input {{label, reason, placeholder?}}, choose {{question, options:[...]}}, \
         sign_in_engine {{engine}}, \
         add_trigger {{trigger: {{type, ...}}, prompt, reason}} \
         (trigger types: schedule {{cron}}, interval {{every_secs}}, github {{repo, events}}, \
         file {{path, glob?}}, bot {{from_bot, on}}, service {{project_dir, service, on}}; \
         cron is in the user's local time when tz is known).\n\
         Task: {prompt}",
        name = bot.name,
        role = bot.role,
        mem = memory.display(),
    )
}

async fn run_one_conversational(hub: &Hub, bot: &Bot, engine_cfg: &EngineConfig, mut task: Task) {
    let pane_key = format!("bot:{}:{}", bot.id, task.id);
    let memory_path = hub.bots.store.memory_path(&bot.id);

    // Pre-task: if this is a GitHub project and gh is not logged in, park and wait.
    if let Some(reason) = needs_github_auth(bot).await
        && let Some(hub_arc) = hub.bots.hub_arc()
    {
        let action = crate::bots::store::TaskAction::ConnectGithub { reason };
        park_task(hub, bot, &mut task, action.clone());
        match wait_for_github_connect(&hub_arc, &bot.id, &task.id).await {
            WaitResult::Connected => {
                task.action = None;
            }
            WaitResult::Cancelled => {
                crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
                return;
            }
            WaitResult::Timeout => {
                finish_task(
                    hub,
                    bot,
                    &mut task,
                    Err("timed out waiting for GitHub connection".into()),
                    vec![],
                );
                crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
                return;
            }
        }
    }

    // After the GitHub wait: a private clone only works once gh is logged in.
    let work_dir = match prepare_work_dir(hub, bot, &mut task).await {
        Ok(d) => d,
        Err(e) => {
            finish_task(hub, bot, &mut task, Err(e), vec![]);
            crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
            return;
        }
    };
    let full_prompt = task_prompt(
        bot,
        &work_dir,
        &memory_path,
        &task.prompt,
        task.tz.as_deref(),
    );

    // Load prior session and bot secrets (secrets.json).
    let session = hub.bots.store.load_session(&bot.id);
    let mut extra_env = hub.bots.store.secret_env(&bot.id);

    // Also inject per-agent env overrides from <dir>/agent-env/<agent>.env —
    // the same file the PTY runner loads.  This lets a custom provider key
    // stored by the provider-setup UI reach the conversational engines
    // (omp, claude, codex, opencode) without requiring a per-bot secret.
    // bot secrets win: entry() only inserts when the key is absent.
    let agent_env_file = hub.dir.join("agent-env").join(format!("{}.env", bot.agent));
    if let Ok(data) = std::fs::read_to_string(&agent_env_file) {
        for line in data.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                extra_env
                    .entry(k.trim().to_string())
                    .or_insert_with(|| v.trim_matches('"').to_string());
            }
        }
    }

    // Hook endpoint from desktop env file.
    let (hook_port, hook_token) = load_hook_endpoint(&hub.dir);

    // Mark running.
    task.state = TaskState::Running;
    task.started_at = Some(now_s());
    task.pane = Some(pane_key.clone());
    let _ = hub.bots.store.append_task(&bot.id, &task);
    broadcast_task(hub, &bot.id, &task);

    let yolo = bot.approval == Approval::Auto;

    // For omp with approval==ask: write the extension file before engine::run_turn
    // (even if TEST_ENGINE_BIN bypasses the real omp, the file must exist for the test).
    if engine_cfg.kind == "omp" && !yolo {
        let ext_path = hub.bots.store.bot_dir(&bot.id).join("omp-approval.ts");
        let _ = crate::bots::engine::omp::write_approval_extension_pub(
            &ext_path,
            hook_port,
            &hook_token,
            &pane_key,
        );
    }

    let result = crate::bots::engine::run_turn(
        engine_cfg,
        session,
        &full_prompt,
        &hub.bots.store.bot_dir(&bot.id),
        &bot.id,
        hook_port,
        &hook_token,
        &pane_key,
        &extra_env,
        &work_dir,
        yolo,
    )
    .await;

    match result {
        Ok(turn) => {
            let _ = hub.bots.store.save_session(&bot.id, &turn.session);
            if let Some(u) = &turn.usage {
                let _ = hub
                    .bots
                    .store
                    .append_usage(&bot.id, &engine_cfg.kind, now_s(), u);
            }
            // Check for ATLAS_ACTION in the reply.
            if let Some(action) = parse_atlas_action(&turn.reply) {
                if let Some(hub_arc) = hub.bots.hub_arc() {
                    park_task(hub, bot, &mut task, action.clone());
                    match resolve_action(&hub_arc, &bot.id, &task.id, &action).await {
                        Some(follow_up) => {
                            // The action stays on the task (cards only show while waiting): with `answer` it lets the
                            // chat show what was asked and what the user picked.
                            task.answer = user_answer(&action, &follow_up);
                            // Reload env (may include newly written secrets).
                            let session2 = hub.bots.store.load_session(&bot.id);
                            let mut extra_env2 = hub.bots.store.secret_env(&bot.id);
                            let agent_env_file2 =
                                hub.dir.join("agent-env").join(format!("{}.env", bot.agent));
                            if let Ok(data) = std::fs::read_to_string(&agent_env_file2) {
                                for line in data.lines() {
                                    let line = line.trim();
                                    if line.starts_with('#') || line.is_empty() {
                                        continue;
                                    }
                                    if let Some((k, v)) = line.split_once('=') {
                                        extra_env2
                                            .entry(k.trim().to_string())
                                            .or_insert_with(|| v.trim_matches('"').to_string());
                                    }
                                }
                            }
                            let result2 = crate::bots::engine::run_turn(
                                engine_cfg,
                                session2,
                                &follow_up,
                                &hub.bots.store.bot_dir(&bot.id),
                                &bot.id,
                                hook_port,
                                &hook_token,
                                &pane_key,
                                &extra_env2,
                                &work_dir,
                                yolo,
                            )
                            .await;
                            match result2 {
                                Ok(turn2) => {
                                    let _ = hub.bots.store.save_session(&bot.id, &turn2.session);
                                    if let Some(u) = &turn2.usage {
                                        let _ = hub.bots.store.append_usage(
                                            &bot.id,
                                            &engine_cfg.kind,
                                            now_s(),
                                            u,
                                        );
                                    }
                                    task.reply = Some(turn2.reply.clone());
                                    task.usage = turn2.usage;
                                    let (final_outcome, reply_full, was_summarized) =
                                        maybe_summarize(hub, bot, Ok(turn2.reply), &extra_env2)
                                            .await;
                                    task.reply_full = reply_full;
                                    task.summarized = was_summarized;
                                    finish_task(hub, bot, &mut task, final_outcome, vec![]);
                                }
                                Err(e) => finish_task(hub, bot, &mut task, Err(e), vec![]),
                            }
                        }
                        None => {
                            let re_tasks = hub.bots.store.read_tasks(&bot.id, 10_000);
                            if !re_tasks
                                .iter()
                                .any(|t| t.id == task.id && t.state == TaskState::Cancelled)
                            {
                                finish_task(
                                    hub,
                                    bot,
                                    &mut task,
                                    Err("timed out waiting for action response".into()),
                                    vec![],
                                );
                            } else {
                                task.state = TaskState::Cancelled;
                            }
                        }
                    }
                } else {
                    task.reply = Some(turn.reply.clone());
                    task.usage = turn.usage;
                    let (final_outcome, reply_full, was_summarized) =
                        maybe_summarize(hub, bot, Ok(turn.reply), &extra_env).await;
                    task.reply_full = reply_full;
                    task.summarized = was_summarized;
                    finish_task(hub, bot, &mut task, final_outcome, vec![]);
                }
            } else {
                task.reply = Some(turn.reply.clone());
                task.usage = turn.usage;
                let (final_outcome, reply_full, was_summarized) =
                    maybe_summarize(hub, bot, Ok(turn.reply), &extra_env).await;
                task.reply_full = reply_full;
                task.summarized = was_summarized;
                finish_task(hub, bot, &mut task, final_outcome, vec![]);
            }
        }
        Err(e) => {
            finish_task(hub, bot, &mut task, Err(e), vec![]);
        }
    }
    crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
}

fn load_hook_endpoint(dir: &std::path::Path) -> (u16, String) {
    let env_path = dir.join("desktop").join("hook-endpoint.env");
    let content = std::fs::read_to_string(env_path).unwrap_or_default();
    let mut port = 0u16;
    let mut token = String::new();
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_PORT=") {
            port = v.parse().unwrap_or(0);
        }
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_TOKEN=") {
            token = v.to_string();
        }
    }
    (port, token)
}

async fn run_one_pty(hub: &Hub, bot: &Bot, mut task: Task) {
    let pane_key = format!("bot:{}:{}", bot.id, task.id);

    let memory_path = hub.bots.store.memory_path(&bot.id);

    // Pre-task: if this is a GitHub project and gh is not logged in, park and wait.
    if let Some(reason) = needs_github_auth(bot).await
        && let Some(hub_arc) = hub.bots.hub_arc()
    {
        let action = crate::bots::store::TaskAction::ConnectGithub { reason };
        park_task(hub, bot, &mut task, action.clone());
        match wait_for_github_connect(&hub_arc, &bot.id, &task.id).await {
            WaitResult::Connected => {
                task.action = None;
            }
            WaitResult::Cancelled => {
                crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
                return;
            }
            WaitResult::Timeout => {
                finish_task(
                    hub,
                    bot,
                    &mut task,
                    Err("timed out waiting for GitHub connection".into()),
                    vec![],
                );
                crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
                return;
            }
        }
    }

    // After the GitHub wait: a private clone only works once gh is logged in.
    let work_dir = match prepare_work_dir(hub, bot, &mut task).await {
        Ok(d) => d,
        Err(e) => {
            finish_task(hub, bot, &mut task, Err(e), vec![]);
            crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
            return;
        }
    };
    let full_prompt = task_prompt(
        bot,
        &work_dir,
        &memory_path,
        &task.prompt,
        task.tz.as_deref(),
    );

    // Load agent env for maybe_summarize (same source as run_pty_inner uses for PTY env).
    let mut pty_agent_env: std::collections::BTreeMap<String, String> = Default::default();
    let agent_env_file = hub.dir.join("agent-env").join(format!("{}.env", bot.agent));
    if let Ok(data) = std::fs::read_to_string(&agent_env_file) {
        for line in data.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                pty_agent_env.insert(k.trim().to_string(), v.trim_matches('"').to_string());
            }
        }
    }

    let (outcome, steps) =
        run_pty_inner(hub, bot, &mut task, &full_prompt, &pane_key, &work_dir).await;
    // Check for ATLAS_ACTION in a successful result.
    if let Ok(result_text) = &outcome
        && let Some(action) = parse_atlas_action(result_text)
        && let Some(hub_arc) = hub.bots.hub_arc()
    {
        // Show what the agent said above the card while it waits (the card replaces the machine line).
        task.result = Some(visible_reply(result_text)).filter(|s| !s.is_empty());
        park_task(hub, bot, &mut task, action.clone());
        let resolved = resolve_action(&hub_arc, &bot.id, &task.id, &action).await;
        if let Some(answer) = resolved {
            task.answer = user_answer(&action, &answer);
            let next = continuation_prompt(&full_prompt, result_text, &answer);
            let (outcome2, steps2) =
                run_pty_inner(hub, bot, &mut task, &next, &pane_key, &work_dir).await;
            let (final_outcome, reply_full, was_summarized) =
                maybe_summarize(hub, bot, outcome2, &pty_agent_env).await;
            task.reply_full = reply_full;
            task.summarized = was_summarized;
            finish_task(hub, bot, &mut task, final_outcome, steps2);
        } else {
            let re_tasks = hub.bots.store.read_tasks(&bot.id, 10_000);
            if !re_tasks
                .iter()
                .any(|t| t.id == task.id && t.state == TaskState::Cancelled)
            {
                finish_task(
                    hub,
                    bot,
                    &mut task,
                    Err("timed out waiting for action response".into()),
                    vec![],
                );
            } else {
                task.state = TaskState::Cancelled;
            }
        }
        crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
        return;
    }
    let (final_outcome, reply_full, was_summarized) =
        maybe_summarize(hub, bot, outcome, &pty_agent_env).await;
    task.reply_full = reply_full;
    task.summarized = was_summarized;
    finish_task(hub, bot, &mut task, final_outcome, steps);
    crate::bots::triggers::on_task_finished(hub, &bot.id, &task.id, task.state);
}

/// Spawn a PTY for the agent and wait for completion; returns (outcome, steps).
/// Sets task state to Running, stores and broadcasts the update.
async fn run_pty_inner(
    hub: &Hub,
    bot: &Bot,
    task: &mut Task,
    full_prompt: &str,
    pane_key: &str,
    work_dir: &str,
) -> (Result<String, String>, Vec<Step>) {
    // Resolve the agent definition.
    let Some(agent_def) = atlas_core::agents::get(&bot.agent) else {
        return (Err(format!("unknown agent '{}'", bot.agent)), vec![]);
    };

    // In tests a fake binary can be injected; production always uses the agent def.
    #[cfg(test)]
    let argv = match TEST_AGENT_BIN.lock().clone() {
        Some(bin) => vec![bin],
        None => agent_def.argv(bot.approval == Approval::Auto, Some(full_prompt)),
    };
    #[cfg(not(test))]
    let argv = {
        let mut argv = agent_def.argv(bot.approval == Approval::Auto, Some(full_prompt));
        if bot.agent == "omp" {
            let overlay = crate::bots::engine::omp::overlay_args(&hub.bots.store.bot_dir(&bot.id));
            argv.splice(1..1, overlay);
        }
        argv
    };

    // Build PtyCreate, started in the bot's project (prepare_work_dir), never in hostd's own directory.
    let cwd = work_dir.to_string();

    let mut req = PtyCreate {
        cwd,
        argv,
        env: {
            let mut m = std::collections::BTreeMap::new();
            m.insert("ATLAS_HOOK_DECISION_SECS".to_string(), "1800".to_string());
            m
        },
        cols: 120,
        rows: 40,
        scope: Scope {
            pane_key: Some(pane_key.to_string()),
            ..Default::default()
        },
        title: Some(format!("bot:{}", bot.name)),
        mode: Mode::Pty,
    };

    // Load per-agent env overrides from <dir>/agent-env/<agent>.env (KEY=VALUE lines).
    let agent_env_file = hub.dir.join("agent-env").join(format!("{}.env", bot.agent));
    if let Ok(data) = std::fs::read_to_string(&agent_env_file) {
        for line in data.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                req.env
                    .insert(k.trim().to_string(), v.trim_matches('"').to_string());
            }
        }
    }

    // Inject hook endpoint env vars (must come after bot env so ATLAS_* always wins).
    crate::server::hook_env(hub, &mut req);

    // Spawn the pty.
    let session = match Session::spawn(req, &paths::scrollback_dir(&hub.dir)) {
        Ok(s) => s,
        Err(e) => return (Err(format!("spawn failed: {e:#}")), vec![]),
    };

    let pty_id = session.id.clone();
    hub.sessions.lock().insert(pty_id.clone(), session.clone());

    // Mark task as running and record start time.
    task.state = TaskState::Running;
    task.started_at = Some(now_s());
    task.pane = Some(pane_key.to_string());
    let _ = hub.bots.store.append_task(&bot.id, task);
    broadcast_task(hub, &bot.id, task);

    // Wait for completion.
    let (outcome, steps) = wait_for_outcome(hub, &session, pane_key, &bot.id, task).await;

    // Clean up session.
    hub.sessions.lock().remove(&pty_id);

    (outcome, steps)
}

/// Poll agent status events and pty exit to determine the task outcome.
async fn wait_for_outcome(
    hub: &Hub,
    session: &Arc<Session>,
    pane_key: &str,
    bot_id: &str,
    task: &mut Task,
) -> (Result<String, String>, Vec<Step>) {
    let mut agent_rx = hub.agents.subscribe();
    let deadline = tokio::time::sleep(BOT_TASK_TIMEOUT);
    tokio::pin!(deadline);
    let mut steps: Vec<Step> = Vec::new();

    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => {
                session.kill().await;
                return (Err("timed out after 2h".to_string()), steps);
            }
            ev = agent_rx.recv() => {
                match ev {
                    Ok(v) if v["ev"] == "agent_status"
                        && v["pane_key"].as_str() == Some(pane_key) =>
                    {
                        // The task the agent set itself ("DANFE: Add DANFE and XML download to portal"), shown on
                        // the live card so the user sees what it is doing, not just "Working".
                        if let Some(title) = v["status"]["title"].as_str().and_then(clean_title)
                            && task.title.as_deref() != Some(title.as_str())
                        {
                            task.title = Some(title);
                            let _ = hub.bots.store.append_task(bot_id, task);
                            broadcast_task(hub, bot_id, task);
                        }
                        let state = v["status"]["state"].as_str().unwrap_or("");
                        if state == "done" {
                            let result = v["status"]["last_message"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            session.kill().await;
                            return (Ok(result), steps);
                        }
                    }
                    Ok(v) if v["ev"] == "agent_step"
                        && v["pane_key"].as_str() == Some(pane_key) =>
                    {
                        if steps.len() < 50 {
                            let tool_name = v["tool_name"].as_str().unwrap_or("?");
                            let target = v["tool_input"]
                                .as_object()
                                .and_then(|m| m.values().find_map(|v| v.as_str()))
                                .map(|s| s.chars().take(80).collect::<String>());
                            let text = match target {
                                Some(t) => format!("{tool_name} {t}"),
                                None => tool_name.to_string(),
                            };
                            steps.push(Step { ts: now_s(), text, group: task.title.clone() });
                            // Live, not only at the end: the working line shows the latest step while it runs.
                            task.steps = steps.clone();
                            let _ = hub.bots.store.append_task(bot_id, task);
                            broadcast_task(hub, bot_id, task);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return (Err("agent hub closed".to_string()), steps);
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if let Some(code) = session.exited() {
                    return (Err(format!(
                        "agent exited (code {})",
                        code.map_or_else(|| "?".to_string(), |c| c.to_string())
                    )), steps);
                }
            }
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn finish_task(
    hub: &Hub,
    bot: &Bot,
    task: &mut Task,
    outcome: Result<String, String>,
    steps: Vec<Step>,
) {
    task.finished_at = Some(now_s());
    task.steps = steps;
    match outcome {
        Ok(result) => {
            task.state = TaskState::Done;
            // Every path ends here, so the machine line never reaches what people read (desktop, phone, Telegram).
            task.result = Some(visible_reply(&result));
        }
        Err(e) => {
            // Don't overwrite Cancelled state.
            if task.state != TaskState::Cancelled {
                task.state = TaskState::Failed;
                task.error = Some(e);
            }
        }
    }
    let _ = hub.bots.store.append_task(&bot.id, task);
    broadcast_task(hub, &bot.id, task);

    // Telegram outbound — done and failed both get a message.
    let telegram_msg: Option<String> = match task.state {
        TaskState::Done => {
            let r = task.result.as_deref().unwrap_or("");
            Some(
                format!("{}: done\n\n{}", bot.name, r)
                    .chars()
                    .take(4000)
                    .collect(),
            )
        }
        TaskState::Failed => {
            let e = task.error.as_deref().unwrap_or("unknown");
            Some(
                format!("{}: failed: {}", bot.name, e)
                    .chars()
                    .take(4000)
                    .collect(),
            )
        }
        _ => None,
    };
    if let Some(msg) = telegram_msg {
        hub.bots.telegram.send_message(&msg);
    }

    // Push notification — only for task_done, skip phones already connected.
    if task.state != TaskState::Done {
        return;
    }
    let live = hub.mobile.live_node_ids();
    let tokens: Vec<_> = hub
        .bots
        .push_tokens
        .lock()
        .iter()
        .filter(|t| !live.contains(&t.node_id))
        .cloned()
        .collect();
    if !tokens.is_empty() {
        tokio::spawn(async move {
            let expired = crate::bots::push::notify(&tokens, "task_done").await;
            if !expired.is_empty() {
                eprintln!("atlas bots: push: {} expired token(s)", expired.len());
            }
        });
    }
}

fn broadcast_task(hub: &Hub, bot_id: &str, task: &Task) {
    hub.broadcast(json!({"ev": "bot_task", "bot_id": bot_id, "task": task}).to_string());
}

fn update_task(
    hub: &Hub,
    store: &BotStore,
    bot_id: &str,
    task_id: &str,
    f: impl FnOnce(&mut Task),
) {
    // Find the task in the store (last state).
    let tasks = store.read_tasks(bot_id, 10_000);
    if let Some(mut t) = tasks.into_iter().find(|t| t.id == task_id) {
        f(&mut t);
        let _ = store.append_task(bot_id, &t);
        broadcast_task(hub, bot_id, &t);
    }
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident: carlos offered "DANFE / Pix / …" as a choice, the user picked DANFE, and carlos asked the very same
    // question again. The terminal runner re-ran the ORIGINAL prompt after the answer, so the agent never saw it.
    #[test]
    fn the_follow_up_carries_the_answer_and_what_was_asked() {
        let reply = "Sugiro DANFE ou Pix.\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Por onde?\",\"options\":[\"DANFE\",\"Pix\"]}";
        let p = continuation_prompt("BASE", reply, "DANFE");
        assert!(p.starts_with("BASE"));
        assert!(
            p.contains("Sugiro DANFE ou Pix."),
            "the agent sees its own last reply"
        );
        assert!(p.contains("DANFE"), "and the user's answer");
        assert!(
            !p.contains("ATLAS_ACTION"),
            "not the machine line it already resolved"
        );
        assert!(p.to_lowercase().contains("do not ask it again"));
    }

    // The action line is for Atlas, not for people: it was shown raw at the end of the reply.
    #[test]
    fn the_reply_shown_to_the_user_drops_the_action_line() {
        let reply = "Posso fazer A ou B.\n\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"q\",\"options\":[\"A\"]}\n";
        assert_eq!(visible_reply(reply), "Posso fazer A ou B.");
        assert_eq!(visible_reply("Sem ação."), "Sem ação.");
    }

    // Real titles from omp on the test ship: the first one is the start of our prompt, the next carries an emoji.
    #[test]
    fn live_card_title_drops_the_prompt_echo_and_the_emoji() {
        assert_eq!(clean_title("You are carlos. Something else"), None);
        assert_eq!(
            clean_title("🏪 STORE: Resumir propósito do apps/store").as_deref(),
            Some("STORE: Resumir propósito do apps/store")
        );
        assert_eq!(
            clean_title("DANFE: Add DANFE and XML download").as_deref(),
            Some("DANFE: Add DANFE and XML download")
        );
        assert_eq!(clean_title("  "), None);
    }

    // Incident: the user picked "Baixar o DANFE" on a choice card and the conversation showed nothing; the pick must be
    // recorded as their message, while a secret's note must never pose as something they said.
    #[test]
    fn only_picks_and_typed_answers_become_the_users_message() {
        use crate::bots::store::TaskAction;
        let choose = TaskAction::Choose {
            question: "q".into(),
            options: vec!["DANFE".into()],
        };
        assert_eq!(user_answer(&choose, "DANFE").as_deref(), Some("DANFE"));
        let secret = TaskAction::ProvideSecret {
            name: "K".into(),
            label: "K".into(),
            reason: "r".into(),
        };
        assert_eq!(
            user_answer(&secret, "The secret K is now set. Continue."),
            None
        );
        assert_eq!(
            user_answer(
                &TaskAction::ConnectGithub { reason: "r".into() },
                "GitHub is connected now."
            ),
            None
        );
    }

    #[test]
    fn agent_env_file_is_injected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("agent-env")).unwrap();
        std::fs::write(
            dir.path().join("agent-env/omp.env"),
            "OPENROUTER_API_KEY=sk-test\n# comment\nFOO=bar\n",
        )
        .unwrap();
        // Simulate reading the file (just the parsing logic, no Hub needed)
        let mut env: std::collections::BTreeMap<String, String> = Default::default();
        let data = std::fs::read_to_string(dir.path().join("agent-env/omp.env")).unwrap();
        for line in data.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                env.insert(k.trim().to_string(), v.trim_matches('"').to_string());
            }
        }
        assert_eq!(env["OPENROUTER_API_KEY"], "sk-test");
        assert_eq!(env["FOO"], "bar");
        assert!(!env.contains_key("# comment"));
    }

    // Helper: write a shell script to a tempdir/bin/<name> and return its path.
    #[cfg(unix)]
    fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    // summarize() returns None when the summarizer binary exits non-zero.
    #[cfg(unix)]
    #[tokio::test]
    async fn summarizer_failure_falls_back_to_original() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "fail-sum", "exit 1");
        let env: std::collections::BTreeMap<String, String> = Default::default();
        // Hold lock to avoid racing with other tests that mutate TEST_SUMMARIZER_BIN.
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            "some long text",
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert!(result.is_none(), "non-zero exit must return None");
    }

    // summarize() returns None when the summarizer output is longer than the input.
    #[cfg(unix)]
    #[tokio::test]
    async fn summarizer_longer_output_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        // Emit a string longer than the input "hi" (2 chars).
        let bin = script(
            dir.path(),
            "long-sum",
            "printf 'this is much longer than hi'",
        );
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize("hi", "omp", "model", &env, std::path::Path::new("/tmp")).await;
        set_test_summarizer_bin(None);
        assert!(result.is_none(), "longer output must return None");
    }

    // summarize() passes rules + original text as argv[1] to the binary.
    #[cfg(unix)]
    #[tokio::test]
    async fn summarizer_prompt_contains_rules_and_original() {
        let dir = tempfile::tempdir().unwrap();
        let out_file = dir.path().join("prompt.txt");
        let out_path = out_file.to_string_lossy().into_owned();
        // Write argv[1] to a file, then emit a short summary.
        let bin = script(
            dir.path(),
            "record-sum",
            &format!("printf '%s' \"$1\" > {out_path}\nprintf 'ok'"),
        );
        let original = "A".repeat(700);
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            &original,
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert_eq!(result.as_deref(), Some("ok"));
        let prompt = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            prompt.contains("No new facts, no preamble, no emojis"),
            "prompt must contain the summarizer rules"
        );
        assert!(
            prompt.contains(&original),
            "prompt must contain the original text"
        );
    }

    // summarize() returns Some(summary) for a successful binary and shorter output.
    #[cfg(unix)]
    #[tokio::test]
    async fn summarize_succeeds_with_shorter_output() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "ok-sum", "printf 'short'");
        let long_text = "word ".repeat(200); // much longer than "short"
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            &long_text,
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert_eq!(result.as_deref(), Some("short"));
    }

    // CrewSettings: default has short_replies=true, explicit false parses correctly.
    #[test]
    fn crew_settings_parse_with_explicit_strings() {
        use crate::bots::store::CrewSettings;
        let default: CrewSettings = serde_json::from_str("{}").unwrap();
        assert!(default.short_replies, "default must be true");
        assert!(default.short_replies_model.is_none());

        let off: CrewSettings = serde_json::from_str(r#"{"short_replies": false}"#).unwrap();
        assert!(!off.short_replies);

        let with_model: CrewSettings =
            serde_json::from_str(r#"{"short_replies": true, "short_replies_model": "my-model"}"#)
                .unwrap();
        assert_eq!(with_model.short_replies_model.as_deref(), Some("my-model"));
    }

    // CrewSettings round-trip through the store.
    #[test]
    fn crew_settings_store_round_trip() {
        use crate::bots::store::{BotStore, CrewSettings};
        let dir = tempfile::tempdir().unwrap();
        let store = BotStore::new(dir.path());
        // Default: short_replies=true when no file exists.
        let loaded = store.load_crew_settings();
        assert!(loaded.short_replies);
        // Save false and reload.
        let settings = CrewSettings {
            short_replies: false,
            short_replies_model: None,
        };
        store.save_crew_settings(&settings).unwrap();
        let reloaded = store.load_crew_settings();
        assert!(!reloaded.short_replies);
    }
}
