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

use crate::bots::store::{Approval, Bot, BotStore, Task, TaskState};
use crate::paths;
use crate::protocol::{Mode, PtyCreate, Scope};
use crate::pty::Session;
use crate::server::Hub;

/// 2-hour hard timeout per bot task.
const BOT_TASK_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Test-only: serialise tests that need to override the spawned agent binary.
/// Hold via `.lock()` for the entire test to prevent races between parallel tests.
#[cfg(test)]
pub(crate) static TEST_BIN_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

/// Test-only: absolute path to a fake agent binary, overrides the agent def's bin.
#[cfg(test)]
static TEST_AGENT_BIN: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// Set (or clear) the per-test agent binary override.  Always acquire
/// `TEST_BIN_LOCK` first to prevent concurrent tests from racing.
#[cfg(test)]
pub(crate) fn set_test_agent_bin(bin: Option<&str>) {
    *TEST_AGENT_BIN.lock() = bin.map(str::to_string);
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

    Ok(())
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

async fn run_one(hub: &Hub, bot: &Bot, mut task: Task) {
    let pane_key = format!("bot:{}:{}", bot.id, task.id);

    // Build the full prompt.
    let memory_path = hub.bots.store.memory_path(&bot.id);
    let full_prompt = format!(
        "You are {name}. {role}\n\
         Your memory file is {mem}. Read it first. When you learn something durable \
         about this work or this user, append it there.\n\
         Task: {prompt}",
        name = bot.name,
        role = bot.role,
        mem = memory_path.display(),
        prompt = task.prompt,
    );

    // Resolve the agent definition.
    let Some(agent_def) = atlas_core::agents::get(&bot.agent) else {
        finish_task(
            hub,
            bot,
            &mut task,
            Err(format!("unknown agent '{}'", bot.agent)),
        );
        return;
    };

    // In tests a fake binary can be injected; production always uses the agent def.
    #[cfg(test)]
    let argv = match TEST_AGENT_BIN.lock().clone() {
        Some(bin) => vec![bin],
        None => agent_def.argv(bot.approval == Approval::Auto, Some(&full_prompt)),
    };
    #[cfg(not(test))]
    let argv = agent_def.argv(bot.approval == Approval::Auto, Some(&full_prompt));

    // Build PtyCreate.
    let cwd = bot
        .workdir
        .clone()
        .unwrap_or_else(|| hub.dir.to_string_lossy().to_string());

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
            pane_key: Some(pane_key.clone()),
            ..Default::default()
        },
        title: Some(format!("bot:{}", bot.name)),
        mode: Mode::Pty,
    };

    // Inject hook endpoint env vars (must come after bot env so ATLAS_* always wins).
    crate::server::hook_env(hub, &mut req);

    // Spawn the pty.
    let session = match Session::spawn(req, &paths::scrollback_dir(&hub.dir)) {
        Ok(s) => s,
        Err(e) => {
            finish_task(hub, bot, &mut task, Err(format!("spawn failed: {e:#}")));
            return;
        }
    };

    let pty_id = session.id.clone();
    hub.sessions.lock().insert(pty_id.clone(), session.clone());

    // Mark task as running.
    task.state = TaskState::Running;
    task.pane = Some(pane_key.clone());
    let _ = hub.bots.store.append_task(&bot.id, &task);
    broadcast_task(hub, &bot.id, &task);

    // Wait for completion.
    let outcome = wait_for_outcome(hub, &session, &pane_key).await;

    // Clean up session.
    hub.sessions.lock().remove(&pty_id);

    finish_task(hub, bot, &mut task, outcome);
}

/// Poll agent status events and pty exit to determine the task outcome.
async fn wait_for_outcome(
    hub: &Hub,
    session: &Arc<Session>,
    pane_key: &str,
) -> Result<String, String> {
    let mut agent_rx = hub.agents.subscribe();
    let deadline = tokio::time::sleep(BOT_TASK_TIMEOUT);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => {
                session.kill().await;
                return Err("timed out after 2h".to_string());
            }
            ev = agent_rx.recv() => {
                match ev {
                    Ok(v) if v["ev"] == "agent_status"
                        && v["pane_key"].as_str() == Some(pane_key) =>
                    {
                        let state = v["status"]["state"].as_str().unwrap_or("");
                        if state == "done" {
                            let result = v["status"]["last_message"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            session.kill().await;
                            return Ok(result);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("agent hub closed".to_string());
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if let Some(code) = session.exited() {
                    return Err(format!(
                        "agent exited (code {})",
                        code.map_or_else(|| "?".to_string(), |c| c.to_string())
                    ));
                }
            }
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn finish_task(hub: &Hub, bot: &Bot, task: &mut Task, outcome: Result<String, String>) {
    task.finished_at = Some(now_s());
    match outcome {
        Ok(result) => {
            task.state = TaskState::Done;
            task.result = Some(result);
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
