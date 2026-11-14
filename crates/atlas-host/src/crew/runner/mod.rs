//! One-at-a-time FIFO task runner per crewmate.
//!
//! Each crewmate has a `CrewmateRunner` with an in-memory queue and one background worker task. The
//! worker pops tasks, runs each one (a conversational engine turn, or the agent in a terminal), waits
//! for it to finish (or times out after 2 h) and updates the task state.
//!
//! | file           | what it owns                                                      |
//! |----------------|-------------------------------------------------------------------|
//! | `mod.rs`       | the queue, `enqueue`/`cancel`, the worker loop                    |
//! | `conversation` | a task of a crewmate with an engine                               |
//! | `pty`          | a task whose agent runs in a terminal                             |
//! | `prompt`       | working directory, instructions, agent env files, hook endpoint   |
//! | `action`       | `ATLAS_ACTION` lines: parse, park, wait for the answer            |
//! | `github`       | GitHub login a project needs                                      |
//! | `outcome`      | waiting for the agent, recording and announcing the result        |
//! | `summary`      | short replies                                                     |
//! | `testing`      | test-only overrides                                               |

mod action;
mod conversation;
mod github;
mod outcome;
mod prompt;
mod pty;
mod summary;
#[cfg(test)]
mod testing;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use conversation::run_one_conversational;
use outcome::update_task;
use pty::run_one_pty;

use crate::crew::store::{CrewStore, Crewmate, Task, TaskState};
use crate::crew::{now_s, pane};
use crate::pty::Session;
use crate::server::Hub;

// Re-exported for the crew integration tests, which spawn `/bin/sh` fake agents (unix only).
#[cfg(all(test, unix))]
pub(crate) use action::parse_atlas_action;
#[cfg(all(test, unix))]
pub(crate) use testing::{
    TEST_BIN_LOCK, TEST_FORCE_GITHUB_AUTH, TEST_GITHUB_LOGGED_IN, TEST_PAUSE_AFTER_SPAWN,
    set_test_agent_bin,
};

/// 2-hour hard timeout per task.
const TASK_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

// ─── per-crewmate runner state ───────────────────────────────────────────────

struct CrewmateRunner {
    queue: VecDeque<Task>,
    /// task_id of the currently executing task.
    running: Option<String>,
    notify: Arc<tokio::sync::Notify>,
    /// True while a tokio worker task is alive for this crewmate.
    worker_active: bool,
}

impl CrewmateRunner {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            running: None,
            notify: Arc::new(tokio::sync::Notify::new()),
            worker_active: false,
        }
    }
}

// ─── RunnerMap ───────────────────────────────────────────────────────────────

pub struct RunnerMap(Mutex<HashMap<String, CrewmateRunner>>);

impl RunnerMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Return the task_id currently running for `crewmate_id`, if any.
    pub fn running_task(&self, crewmate_id: &str) -> Option<String> {
        self.0.lock().get(crewmate_id)?.running.clone()
    }

    /// Count of tasks waiting in the queue for `crewmate_id` (not including the running one).
    pub fn queue_len(&self, crewmate_id: &str) -> usize {
        self.0
            .lock()
            .get(crewmate_id)
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

/// Append `task` to the crewmate's queue and start the worker if needed.
pub(crate) fn enqueue(hub: &Hub, store: &CrewStore, crewmate: &Crewmate, task: Task) {
    let crewmate_id = crewmate.id.clone();
    let notify;
    let should_spawn;
    {
        let mut map = hub.crew.runners.0.lock();
        let runner = map
            .entry(crewmate_id.clone())
            .or_insert_with(CrewmateRunner::new);
        runner.queue.push_back(task);
        notify = runner.notify.clone();
        should_spawn = !runner.worker_active;
        if should_spawn {
            runner.worker_active = true;
        }
    }
    notify.notify_one();
    let hub_arc_for_spawn = should_spawn.then(|| hub.crew.hub_arc()).flatten();
    if let Some(hub_arc) = hub_arc_for_spawn {
        let crewmate = crewmate.clone();
        tokio::spawn(async move {
            run_worker(hub_arc, crewmate).await;
        });
    }
    let _ = store; // store already written by caller before enqueue
}

/// Cancel a task: remove from queue if pending, kill pty if running.
pub(crate) fn cancel(
    hub: &Hub,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
) -> Result<(), String> {
    let is_running;
    {
        let mut map = hub.crew.runners.0.lock();
        if let Some(runner) = map.get_mut(crewmate_id) {
            // Remove from queue if present.
            let before = runner.queue.len();
            runner.queue.retain(|t| t.id != task_id);
            let removed_from_queue = runner.queue.len() < before;
            is_running = runner.running.as_deref() == Some(task_id);
            if !removed_from_queue && !is_running {
                return Err(format!(
                    "task {task_id} not found for crewmate {crewmate_id}"
                ));
            }
        } else {
            return Err(format!("no runner for crewmate {crewmate_id}"));
        }
    }

    // Update task state to Cancelled in the store.
    update_task(hub, store, crewmate_id, task_id, |t| {
        t.state = TaskState::Cancelled;
        t.finished_at = Some(now_s());
    });

    // Kill the pty if the task is currently running.
    if is_running {
        let pane_key = pane::key(crewmate_id, task_id);
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
    hub.crew.notify_github_connected(); // wakes github_connect waiters (they check cancel state)
    hub.crew.cancel_action(task_id); // drops action waiter sender → receiver gets Err
    Ok(())
}

// ─── worker loop ─────────────────────────────────────────────────────────────

async fn run_worker(hub: Arc<Hub>, crewmate: Crewmate) {
    loop {
        // Pop next task from the queue.
        let task = {
            let mut map = hub.crew.runners.0.lock();
            let runner = map
                .get_mut(&crewmate.id)
                .expect("runner entry always present");
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
        run_one(&hub, &crewmate, task).await;

        // Clear running.
        {
            let mut map = hub.crew.runners.0.lock();
            if let Some(runner) = map.get_mut(&crewmate.id) {
                runner.running = None;
            }
        }

        // If the queue is now empty, wait for a notification before checking again.
        let notify = {
            let map = hub.crew.runners.0.lock();
            map.get(&crewmate.id)
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

async fn run_one(hub: &Hub, crewmate: &Crewmate, task: Task) {
    if let Some(engine_cfg) = &crewmate.engine {
        run_one_conversational(hub, crewmate, engine_cfg, task).await;
    } else {
        run_one_pty(hub, crewmate, task).await;
    }
}
