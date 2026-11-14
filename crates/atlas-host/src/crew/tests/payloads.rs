//! Payloads the apps are tested against that are pure functions of a value: the summarized task
//! and the `crewmate_task` push event. No process is spawned, so unlike the other areas these also
//! run on Windows.

use serde_json::{Value, json};

use crate::crew::events;
use crate::crew::store::{Task, TaskState};

#[test]
fn crewmate_task_summarized_fixture_is_real_payload() {
    let mut task = Task::new("desktop", "list the files");
    task.id = "task-sum-1".to_string();
    task.ts = 1700000000;
    task.state = TaskState::Done;
    task.result = Some("Listed 3 files in src/.".to_string());
    task.reply_full = Some(
        "Found the following main files:\n\
         - foo.rs: main module\n\
         - bar.rs: utilities\n\
         - baz.rs: tests\n\n\
         All files are in the src/ directory. No changes were made."
            .to_string(),
    );
    task.summarized = true;
    let json = serde_json::to_string_pretty(&task).expect("serialize task");
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/crewmate_task_summarized.json");
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(fixture_path.parent().unwrap()).ok();
        std::fs::write(&fixture_path, &json).expect("write fixture");
        return;
    }
    if !fixture_path.exists() {
        panic!(
            "fixture missing — run \
             UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_summarized_fixture_is_real_payload \
             to generate it"
        );
    }
    let stored = std::fs::read_to_string(&fixture_path).expect("read fixture");
    let stored_v: serde_json::Value = serde_json::from_str(&stored).expect("parse stored");
    let live_v: serde_json::Value = serde_json::from_str(&json).expect("parse live");
    assert_eq!(
        stored_v, live_v,
        "fixture out of date — run UPDATE_FIXTURES=1 ..."
    );
}

// Incident: the phone read `e['bot']['id']` while hostd sends a flat `crewmate_id`, so its task list
// never refreshed on a real push. The fixture is the event as `events::task` builds it for the
// runner's broadcast (the same function every emitter calls); the mobile test reads this file.
#[test]
fn crewmate_task_event_fixture_is_real_payload() {
    let mut task = Task::new("desktop", "review the PR");
    task.id = "task-event-1".to_string();
    task.ts = 1700000000;
    task.state = TaskState::Running;
    task.started_at = Some(1700000005);
    task.pane = Some("crew:crewmate-working:task-event-1".to_string());
    let live: Value = serde_json::from_str(&events::task("crewmate-working", Some(&task))).unwrap();

    // The keys clients read: a flat id (never a nested crewmate object), the task inside.
    assert_eq!(live["ev"], "crewmate_task");
    assert_eq!(live["crewmate_id"], "crewmate-working");
    assert_eq!(live["task"]["id"], "task-event-1");
    assert!(live.get("crewmate").is_none() && live.get("bot").is_none());

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/crewmate_task_event.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&live).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&fixture_path).expect(
        "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_event_fixture_is_real_payload",
    ))
    .unwrap();
    assert_eq!(
        stored, live,
        "crewmate_task_event.json changed: rerun with UPDATE_FIXTURES=1"
    );

    // A push without a task (a task was queued by a trigger, telegram or the scheduler) is just the id.
    let bare: Value = serde_json::from_str(&events::task("crewmate-working", None)).unwrap();
    assert_eq!(
        bare,
        json!({"ev": "crewmate_task", "crewmate_id": "crewmate-working"})
    );
}
