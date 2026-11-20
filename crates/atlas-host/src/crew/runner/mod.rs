//! One-at-a-time FIFO task runner per crewmate.
//!
//! Each crewmate has a `CrewmateRunner` with an in-memory queue and one background worker task. The
//! worker pops tasks, runs each one (a conversational engine turn, or the agent in a terminal), waits
//! for it to finish (or times out after 2 h) and updates the task state.
//!
//! | file           | what it owns                                                      |
//! |----------------|-------------------------------------------------------------------|
//! | `mod.rs`       | the queue, `enqueue`/`cancel`, the worker loop                    |
//! | `recover`      | what a hostd restart does to tasks in flight                      |
//! | `conversation` | a task of a crewmate with an engine                               |
//! | `pty`          | a task whose agent runs in a terminal                             |
//! | `prompt`       | working directory, instructions, agent env files, hook endpoint   |
//! | `action`       | `ATLAS_ACTION` lines: parse, park, wait for the answer            |
//! | `github`       | GitHub login a project needs                                      |
//! | `reporting`    | a GitHub task's reply comment: posted, edited, finished           |
//! | `slots`        | how many agents run at once on the server                          |
//! | `outcome`      | waiting for the agent, recording and announcing the result        |
//! | `guard`, `tape`| refusing work that weakens tests, strays, invents or overclaims   |
//! | `summary`      | short replies                                                     |
//! | `testing`      | test-only overrides                                               |

mod action;
mod conversation;
mod gate;
mod github;
mod guard;
pub(crate) mod isolation;
mod outcome;
mod prompt;
mod pty;
mod recover;
mod reporting;
mod slots;
pub(crate) use reporting::Live as ReportingLive;
#[cfg(test)]
pub(crate) use reporting::branch_to_report;
pub(crate) use slots::AgentSlots;
#[cfg(all(test, unix))]
pub(crate) use slots::BUSY as SERVER_BUSY;
// Only the unix-only crew tests (tests/prepare.rs) set it: a plain cfg(test) re-export is unused on Windows.
#[cfg(all(test, unix))]
pub(crate) use slots::TEST_MAX_AGENTS;
mod summary;
mod tape;
#[cfg(test)]
mod testing;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use conversation::run_one_conversational;
#[cfg(test)]
pub(crate) use guard::strip_unproven as guard_strip_unproven;
pub(crate) use guard::{OVERRIDE as REFUSAL_OVERRIDE, RETRY as REFUSAL_RETRY};
use outcome::{audit_task, update_task};
// Only the Unix-only `crew::tests::permissions` reaches it from outside; on Windows it would be an unused import.
#[cfg(all(test, unix))]
pub(crate) use pty::launch_argv;
use pty::run_one_pty;
#[cfg(test)]
pub(crate) use recover::interrupted_for_fixture;
pub(crate) use recover::recover;
pub(crate) use tape::{Tape, observe as observe_tape};

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
    TEST_PAUSE_BEFORE_SPAWN, set_test_agent_bin,
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

    /// Drop the runner of a deleted crewmate once its worker has ended (`run_worker` expects its
    /// entry for as long as it lives); a worker still alive ends by itself with an empty queue.
    pub fn forget(&self, crewmate_id: &str) {
        let mut map = self.0.lock();
        if let Some(r) = map.get_mut(crewmate_id) {
            r.queue.clear();
            if !r.worker_active {
                map.remove(crewmate_id);
            }
        }
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

/// Append `task` to the crewmate's queue and start the worker if needed. While the crew is paused
/// the task waits in the queue (a new task still queues) and no worker starts.
pub(crate) fn enqueue(hub: &Hub, store: &CrewStore, crewmate: &Crewmate, task: Task) {
    let _ = store; // store already written by caller before enqueue
    push(hub, crewmate, task, false);
}

/// Like `enqueue`, but the task goes to the HEAD of the queue: a task a restart interrupted runs
/// before the ones the user sent after it (`recover`).
pub(crate) fn enqueue_front(hub: &Hub, crewmate: &Crewmate, task: Task) {
    push(hub, crewmate, task, true);
}

fn push(hub: &Hub, crewmate: &Crewmate, task: Task, front: bool) {
    let crewmate_id = crewmate.id.clone();
    audit_task(hub, &crewmate_id, "task_queued", &task);
    let notify;
    let should_spawn;
    {
        let mut map = hub.crew.runners.0.lock();
        let runner = map
            .entry(crewmate_id.clone())
            .or_insert_with(CrewmateRunner::new);
        if front {
            runner.queue.push_front(task);
        } else {
            runner.queue.push_back(task);
        }
        notify = runner.notify.clone();
        should_spawn = !hub.crew.is_paused() && !runner.worker_active;
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
}

/// `crew_pause`: cancel every running task with `reason` as its error. Queued tasks stay queued,
/// and the workers stop at their next pop (`run_worker` checks the switch).
pub(crate) fn cancel_all(hub: &Hub, reason: &str) {
    let running: Vec<(String, String)> = {
        let map = hub.crew.runners.0.lock();
        map.iter()
            .filter_map(|(c, r)| r.running.clone().map(|t| (c.clone(), t)))
            .collect()
    };
    for (crewmate_id, task_id) in running {
        if let Err(e) = cancel_with(hub, &hub.crew.store, &crewmate_id, &task_id, Some(reason)) {
            // i18n-ignore: log
            eprintln!("atlas crew: pause: cannot cancel task {task_id} of {crewmate_id}: {e}");
        }
    }
}

/// `crew_resume`: start a worker for every crewmate that has queued tasks and none running.
pub(crate) fn resume_all(hub: &Hub) {
    let Some(hub_arc) = hub.crew.hub_arc() else {
        return;
    };
    let waiting: Vec<String> = {
        let mut map = hub.crew.runners.0.lock();
        map.iter_mut()
            .filter(|(_, r)| !r.queue.is_empty() && !r.worker_active)
            .map(|(id, r)| {
                r.worker_active = true;
                id.clone()
            })
            .collect()
    };
    for id in waiting {
        // A fresh definition: it may have changed while the crew was paused.
        match hub.crew.store.load_crewmate(&id) {
            Ok(crewmate) => {
                let hub_arc = hub_arc.clone();
                tokio::spawn(async move { run_worker(hub_arc, crewmate).await });
            }
            Err(e) => {
                // i18n-ignore: log
                eprintln!(
                    "atlas crew: resume: crewmate {id} unreadable, its queue stays held: {e}"
                );
                if let Some(r) = hub.crew.runners.0.lock().get_mut(&id) {
                    r.worker_active = false;
                }
            }
        }
    }
}

/// Cancel a task: remove from queue if pending, kill its pty or end its engine turn if running.
pub(crate) fn cancel(
    hub: &Hub,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
) -> Result<(), String> {
    cancel_with(hub, store, crewmate_id, task_id, None)
}

fn cancel_with(
    hub: &Hub,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    reason: Option<&str>,
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
        if let Some(reason) = reason {
            t.error = Some(reason.to_string());
        }
        audit_task(hub, crewmate_id, "task_finished", t);
    });

    // Kill the pty if the task is currently running; an engine turn has no pty, its handle ends the
    // turn's process tree instead.
    if is_running {
        if let Some(cancel) = hub.crew.turn_cancels.lock().get(task_id) {
            cancel.fire();
        }
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
            // Checked under the same lock `enqueue` and `resume_all` use, so a pause between two
            // tasks can never be missed: the worker ends and the queue stays for `resume_all`.
            if hub.crew.is_paused() {
                runner.worker_active = false;
                return;
            }
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

async fn run_one(hub: &Hub, crewmate: &Crewmate, mut task: Task) {
    // A task from GitHub answers there from the start, so a failure before the agent even runs
    // (project checkout, GitHub login) still edits a comment instead of leaving the issue silent.
    reporting::start(hub, &crewmate.id, &mut task).await;
    // Isolation is installed but its helper is not there: never run the crewmate as the crew user instead.
    if let Some(why) = hub.crew.store.isolation_fault() {
        outcome::finish_task(hub, crewmate, &mut task, Err(why.to_string()), vec![]);
        crate::crew::triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }
    if let Some(engine_cfg) = &crewmate.engine {
        run_one_conversational(hub, crewmate, engine_cfg, task).await;
    } else {
        run_one_pty(hub, crewmate, task).await;
    }
}
