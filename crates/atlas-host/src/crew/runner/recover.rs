//! What hostd does with the tasks a restart (an upgrade, a reboot, a crash) interrupted. Nothing a
//! user started is lost:
//!
//! - `queued` tasks queue again, in order.
//! - a `running` task goes back to the HEAD of its crewmate's queue with the same id, marked
//!   `resumed` and counted in `restarts`, and says so in a step. A conversational task continues its
//!   agent session (`resume_prompt`); a terminal one runs again. After `MAX_RESTARTS` interruptions
//!   it fails instead, so a task that takes the daemon down cannot loop.
//! - a `waiting_action` task stays waiting; the answer still reaches it (`recovered_wait`).
//! - an `ask_user` question belonged to an agent process that is gone: it is closed with a step.
//! - a paused crew starts nothing: `enqueue` holds the queue until `crew_resume`.

use super::{enqueue, enqueue_front};
use crate::crew::now_s;
use crate::crew::store::{Crewmate, Step, Task, TaskState};
use crate::server::Hub;

/// How many restarts a task survives. The restart after that fails it.
pub(crate) const MAX_RESTARTS: u32 = 2;

/// The error of a task its `restarts`-th interruption exhausted the cap for.
fn cap_error(restarts: u32) -> String {
    format!(
        "interrupted by {restarts} server restarts in a row; not resumed again (it may be what stops the server)"
    )
}

/// The prompt a conversational task continues its session with: the agent's own context is in the
/// session, so this only says what happened and repeats the task.
pub(super) fn resume_prompt(task_prompt: &str) -> String {
    format!(
        "The server restarted and your previous run of the task \"{task_prompt}\" was interrupted. \
         Check what is already done (files, git status) and continue from there; do not start over."
    )
}

/// Whether `task` was waiting for the user when the daemon stopped (the runner picks up the wait).
pub(super) fn recovered_wait(task: &Task) -> bool {
    task.state == TaskState::WaitingAction && task.action.is_some()
}

fn step(text: &str) -> Step {
    Step {
        ts: now_s(),
        text: text.to_string(),
        group: None,
    }
}

/// Apply the restart to a task that was `running`: the task as it goes back to the queue, or the
/// failed one when it was interrupted too often.
fn interrupted(mut t: Task) -> Task {
    t.restarts += 1;
    if t.restarts > MAX_RESTARTS {
        t.state = TaskState::Failed;
        t.action = None;
        t.ask = None;
        t.error = Some(cap_error(t.restarts));
        t.finished_at = Some(now_s());
        return t;
    }
    if t.ask.take().is_some() {
        t.steps
            .push(step("The question was dropped: the server restarted"));
    }
    t.steps
        .push(step("The server restarted; running this task again"));
    t.state = TaskState::Queued;
    t.resumed = true;
    t.finished_at = None;
    t
}

/// The task as a restart leaves it (for the contract fixture, which must come from the real code).
#[cfg(test)]
pub(crate) fn interrupted_for_fixture(t: Task) -> Task {
    interrupted(t)
}

/// Bring back the in-flight tasks of every crewmate. Runs once, when hostd starts.
pub(crate) fn recover(hub: &Hub) {
    let store = &hub.crew.store;
    for crewmate in &store.list_crewmates() {
        let tasks = store.read_tasks(&crewmate.id, crate::crew::store::default_task_limit());
        // Oldest first, so queued tasks keep their order; interrupted ones go to the head, last
        // one first, so the oldest of them ends up first.
        let mut resumed: Vec<Task> = vec![];
        for t in tasks.into_iter().rev() {
            match t.state {
                TaskState::Queued => enqueue(hub, store, crewmate, t),
                TaskState::Running => resumed.push(interrupted_and_stored(hub, crewmate, t)),
                TaskState::WaitingAction => enqueue_front(hub, crewmate, t),
                _ => {}
            }
        }
        for t in resumed
            .into_iter()
            .rev()
            .filter(|t| t.state == TaskState::Queued)
        {
            enqueue_front(hub, crewmate, t);
        }
    }
}

fn interrupted_and_stored(hub: &Hub, crewmate: &Crewmate, t: Task) -> Task {
    let t = interrupted(t);
    if let Err(e) = hub.crew.store.append_task(&crewmate.id, &t) {
        // i18n-ignore: log
        eprintln!(
            "atlas crew: crewmate {} task {}: cannot record the restart interruption: {e}",
            crewmate.id, t.id
        );
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(restarts: u32) -> Task {
        let mut t = Task::new("test", "do it");
        t.state = TaskState::Running;
        t.restarts = restarts;
        t
    }

    // #199: the first and second restart resume, the third fails it; the cap is not off by one.
    #[test]
    fn a_task_is_resumed_until_the_cap_then_fails() {
        for (before, state) in [
            (0, TaskState::Queued),
            (1, TaskState::Queued),
            (2, TaskState::Failed),
        ] {
            let t = interrupted(running(before));
            assert_eq!(t.state, state, "after {before} earlier restarts");
            assert_eq!(t.restarts, before + 1);
        }
        let failed = interrupted(running(MAX_RESTARTS));
        assert!(failed.error.unwrap().contains('3'));
    }

    #[test]
    fn a_resumed_task_is_marked_and_explains_itself() {
        let t = interrupted(running(0));
        assert!(t.resumed);
        assert!(t.steps.iter().any(|s| s.text.contains("restarted")));
    }
}
