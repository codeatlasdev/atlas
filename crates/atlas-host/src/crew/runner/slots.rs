//! How many agents the crew server runs at once (`max_agents` in crew settings). A task takes a slot
//! right before its agent starts and gives it back when the agent ends, so a task that only waits
//! for a person (a GitHub login, an action card, a refusal) never holds one, and a task that cannot
//! get one shows "server busy" instead of a bare "Queued" (#296).

use std::sync::Arc;
use std::time::Duration;

use super::outcome::{broadcast_task, persist, was_cancelled};
use crate::crew::store::{Crewmate, Task, TaskState};
use crate::server::Hub;

/// What a task shows while every slot is taken.
const BUSY: &str = "server busy";

/// How often a task waiting for a slot looks at the store for a cancel.
const CANCEL_POLL: Duration = Duration::from_millis(500);

/// Test-only: the limit the crew starts with, whatever `crew.json` says.
#[cfg(test)]
pub(crate) static TEST_MAX_AGENTS: parking_lot::Mutex<Option<usize>> =
    parking_lot::Mutex::new(None);

struct Counts {
    running: usize,
    limit: usize,
}

/// The slots of the server. The limit can change while agents run: a larger one takes effect at
/// once, a smaller one as agents end (nothing is killed).
pub(crate) struct AgentSlots {
    counts: parking_lot::Mutex<Counts>,
    freed: tokio::sync::Notify,
}

/// A held slot, given back when dropped (the agent ended, or the task with it).
pub(crate) struct Slot(Arc<AgentSlots>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.counts.lock().running -= 1;
        self.0.freed.notify_waiters();
    }
}

impl AgentSlots {
    pub(crate) fn new(limit: usize) -> Arc<Self> {
        #[cfg(test)]
        let limit = TEST_MAX_AGENTS.lock().unwrap_or(limit);
        Arc::new(Self {
            counts: parking_lot::Mutex::new(Counts {
                running: 0,
                limit: limit.max(1),
            }),
            freed: tokio::sync::Notify::new(),
        })
    }

    /// Apply a new limit from the settings.
    pub(crate) fn set_limit(&self, limit: usize) {
        self.counts.lock().limit = limit.max(1);
        self.freed.notify_waiters();
    }

    fn try_take(self: &Arc<Self>) -> Option<Slot> {
        let mut c = self.counts.lock();
        (c.running < c.limit).then(|| {
            c.running += 1;
            Slot(self.clone())
        })
    }
}

/// A slot for `task`'s agent, waiting for one when all are taken (the task says "server busy"
/// meanwhile). `None` when the store says the task was cancelled, before or while it waited: the
/// store is the arbiter, so an agent is never started for a cancelled task.
pub(super) async fn take(hub: &Hub, crewmate: &Crewmate, task: &mut Task) -> Option<Slot> {
    let slots = &hub.crew.agent_slots;
    let mut told = false;
    loop {
        // Registered before the check, so a slot freed in between wakes this wait.
        let freed = slots.freed.notified();
        tokio::pin!(freed);
        freed.as_mut().enable();
        if was_cancelled(hub, &crewmate.id, &task.id) {
            clear_busy(hub, crewmate, task, told);
            return None;
        }
        if let Some(slot) = slots.try_take() {
            clear_busy(hub, crewmate, task, told);
            return Some(slot);
        }
        if !told {
            told = true;
            task.state = TaskState::Queued;
            task.waiting_for = Some(BUSY.to_string());
            let _ = persist(hub, &crewmate.id, task);
            broadcast_task(hub, &crewmate.id, task);
        }
        tokio::select! {
            () = &mut freed => {}
            () = tokio::time::sleep(CANCEL_POLL) => {}
        }
    }
}

fn clear_busy(hub: &Hub, crewmate: &Crewmate, task: &mut Task, told: bool) {
    if told {
        task.waiting_for = None;
        let _ = persist(hub, &crewmate.id, task);
        broadcast_task(hub, &crewmate.id, task);
    }
}
