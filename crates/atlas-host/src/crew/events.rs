//! The push events a crew server broadcasts to every client (desktop, phone, the Mac's relay).
//! The one place their JSON is built: clients read these keys, and the contract fixture
//! `apps/mobile/test/fixtures/crewmate_task_event.json` is made from [`task`].

use serde_json::json;

use crate::crew::Task;

/// `{"ev":"crewmate_task","crewmate_id":…}`: a task of this crewmate changed. With `Some(task)` the
/// event carries the task itself (`"task"`); without, clients refetch `crewmate_tasks`.
pub(crate) fn task(crewmate_id: &str, task: Option<&Task>) -> String {
    match task {
        Some(t) => json!({"ev": "crewmate_task", "crewmate_id": crewmate_id, "task": t}),
        None => json!({"ev": "crewmate_task", "crewmate_id": crewmate_id}),
    }
    .to_string()
}

/// `{"ev":"crewmate_updated","crewmate_id":…}`: the crewmate's definition changed.
pub(crate) fn crewmate_updated(crewmate_id: &str) -> String {
    json!({"ev": "crewmate_updated", "crewmate_id": crewmate_id}).to_string()
}

/// `{"ev":"crew_paused","paused":bool}`: the kill switch flipped (`crew_pause` / `crew_resume`).
/// The contract fixture `apps/desktop/src/__fixtures__/crew_paused_event.json` is made from this.
pub(crate) fn crew_paused(paused: bool) -> String {
    json!({"ev": "crew_paused", "paused": paused}).to_string()
}
