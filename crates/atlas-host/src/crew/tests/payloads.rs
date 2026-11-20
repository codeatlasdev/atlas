//! Payloads the apps are tested against that are pure functions of a value: the summarized task
//! and the `crewmate_task` push event. No process is spawned, so unlike the other areas these also
//! run on Windows.

use serde_json::{Value, json};

use crate::crew::events;
use crate::crew::store::{GithubOrigin, GithubOriginKind, Task, TaskState, Trigger};

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

// Issue #204: a task from GitHub carries its origin as structured fields, and a GitHub automation
// carries `handle`/`assignee` next to `authors`/`allow_auto`. Both clients read this file; the
// editors must hand every one of those fields back unchanged when an automation is edited, else
// saving it silently resets `authors` and `allow_auto` to their defaults.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host github_origin_fixture_is_real_payload
#[test]
fn github_origin_fixture_is_real_payload() {
    let mut task = Task::new("trigger", "Handle this: [UNTRUSTED CONTENT ...]");
    task.id = "task-gh-1".to_string();
    task.ts = 1700000000;
    task.state = TaskState::Running;
    task.no_auto = true;
    task.trigger_id = Some("gh".to_string());
    task.origin = Some(GithubOrigin {
        repo: "o/r".into(),
        kind: GithubOriginKind::PrReviewComment,
        number: Some(12),
        comment_id: Some(200),
        branch: Some("feat/parser".into()),
        reply_comment_id: Some(9001),
        requester: None,
    });
    let trigger: Trigger = serde_json::from_value(json!({
        "id": "gh", "enabled": true, "prompt": "Handle this: {{event}}",
        "kind": {"type": "github", "repo": "o/r",
                 "events": ["mention", "issue_assigned", "pr_review_comment", "ci_failed"],
                 "authors": ["owner", "member"], "allow_auto": true,
                 "handle": "nami", "assignee": "nami-bot"}
    }))
    .unwrap();
    let live = json!({"task": task, "trigger": trigger});

    // The keys the clients read.
    assert_eq!(live["task"]["origin"]["kind"], "pr_review_comment");
    assert_eq!(live["task"]["origin"]["reply_comment_id"], 9001);
    assert_eq!(live["trigger"]["kind"]["handle"], "nami");
    assert_eq!(
        live["trigger"]["kind"]["authors"],
        json!(["owner", "member"])
    );

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/github_origin.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&live).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&fixture_path).expect(
        "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host github_origin_fixture_is_real_payload",
    ))
    .unwrap();
    assert_eq!(
        stored, live,
        "github_origin.json changed: rerun with UPDATE_FIXTURES=1"
    );
}

// #199: a task a hostd restart interrupted goes back to the queue marked `resumed` with its `restarts`
// count; the desktop and the phone read those keys. The fixture is the task exactly as the restart
// recovery (`runner::recover`) leaves it and the serializer writes it.
#[test]
fn crewmate_task_resumed_fixture_is_real_payload() {
    let mut task = Task::new("desktop", "ship the release");
    task.id = "task-resumed-1".to_string();
    task.ts = 1700000000;
    task.state = TaskState::Running;
    task.started_at = Some(1700000005);
    task.pane = Some("crew:crewmate-working:task-resumed-1".to_string());
    let mut task = crate::crew::runner::interrupted_for_fixture(task);
    for step in &mut task.steps {
        step.ts = 1700000010; // the recovery stamps the clock; a fixture must not move
    }
    assert!(task.resumed && task.restarts == 1, "{task:?}");
    let json = serde_json::to_string_pretty(&task).expect("serialize task");
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/crewmate_task_resumed.json");
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(fixture_path.parent().unwrap()).ok();
        std::fs::write(&fixture_path, &json).expect("write fixture");
        return;
    }
    let stored = std::fs::read_to_string(&fixture_path).unwrap_or_else(|_| {
        panic!(
            "fixture missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host \
             crewmate_task_resumed_fixture_is_real_payload to generate it"
        )
    });
    let stored_v: Value = serde_json::from_str(&stored).expect("parse stored");
    let live_v: Value = serde_json::from_str(&json).expect("parse live");
    assert_eq!(
        stored_v, live_v,
        "fixture out of date: run UPDATE_FIXTURES=1 ..."
    );
}

// #209: a task with images. The phone and the desktop render the thread from these keys and fetch the bytes
// with `crewmate_attachment`. The records come from `attachments::check` (the code that makes them for real),
// the task and its push event from the real serializers, the reply from `attachments::reply`.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_attachments_fixtures_are_real_payloads
#[test]
fn crewmate_task_attachments_fixtures_are_real_payloads() {
    use crate::crew::attachments::{self, Incoming};
    const PNG: &[u8] =
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";
    const JPG: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
    ];

    let mut task = Task::new("mobile", "the header is cut off on this screen");
    task.id = "task-img-1".to_string();
    task.ts = 1700000000;
    task.state = TaskState::Queued;
    let files = attachments::check(
        &task.id,
        &[
            Incoming {
                name: "IMG_0412.PNG".into(),
                bytes: PNG.to_vec(),
            },
            Incoming {
                name: "marked.jpg".into(),
                bytes: JPG.to_vec(),
            },
        ],
    )
    .unwrap();
    task.attachments = files.iter().map(|(a, _)| a.clone()).collect();

    let live = json!({
        "task": task,
        "event": serde_json::from_str::<Value>(&events::task("crewmate-working", Some(&task))).unwrap(),
        "attachment": attachments::reply(&files[0].0, &files[0].1, 0, None),
    });
    // The keys the clients read.
    assert_eq!(live["task"]["attachments"][0]["name"], "IMG_0412.png");
    assert_eq!(live["task"]["attachments"][1]["mime"], "image/jpeg");
    assert_eq!(
        live["event"]["task"]["attachments"][0]["bytes_len"],
        PNG.len()
    );
    assert_eq!(live["attachment"]["last"], true);
    assert!(
        live["task"]["attachments"][0].get("data_b64").is_none(),
        "a task carries records, never bytes"
    );

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = [
        root.join("apps/mobile/test/fixtures/crewmate_task_attachments.json"),
        root.join("apps/desktop/src/__fixtures__/crewmate_task_attachments.json"),
    ];
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        for p in &paths {
            std::fs::write(p, serde_json::to_string_pretty(&live).unwrap() + "\n").unwrap();
        }
    }
    for p in &paths {
        let stored: Value = serde_json::from_str(&std::fs::read_to_string(p).expect(
            "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_attachments_fixtures_are_real_payloads",
        ))
        .unwrap();
        assert_eq!(
            stored,
            live,
            "{} changed: rerun with UPDATE_FIXTURES=1",
            p.display()
        );
    }
}

// #210/#212: a `ui` task parked on `prove_ui` with the agent's one capture and the owner's own image, the
// card both apps render. Records come from `attachments::add` (agent and owner sources), the task and its
// event from the real serializers.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_prove_ui_fixtures_are_real_payloads
#[test]
fn crewmate_task_prove_ui_fixtures_are_real_payloads() {
    use crate::crew::attachments::{self, Incoming, SOURCE_AGENT};
    use crate::crew::store::{CrewStore, TaskAction};
    const PNG: &[u8] =
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";

    let tmp = tempfile::tempdir().unwrap();
    let store = CrewStore::new(tmp.path());
    let mut task = Task::new("desktop", "restyle the header");
    task.id = "task-ui-1".to_string();
    task.ts = 1700000000;
    task.ui = true;
    let img = |name: &str| Incoming {
        name: name.into(),
        bytes: PNG.to_vec(),
    };
    attachments::add(&store, "c", &mut task, &[img("mock.png")], "").unwrap();
    attachments::add(
        &store,
        "c",
        &mut task,
        &[img("chromium-390-light.png")],
        SOURCE_AGENT,
    )
    .unwrap();
    task.state = TaskState::WaitingAction;
    task.result = Some("Restyled the header.".to_string());
    task.action = Some(TaskAction::ProveUi {
        reason: "Could not prove it: this is a UI task and the agent attached 1 of 2 captures."
            .to_string(),
    });

    let live = json!({
        "task": task,
        "event": serde_json::from_str::<Value>(&events::task("crewmate-working", Some(&task))).unwrap(),
    });
    // The keys the clients read.
    assert_eq!(live["task"]["ui"], true);
    assert_eq!(live["task"]["action"]["kind"], "prove_ui");
    assert_eq!(live["task"]["attachments"][1]["source"], "agent");
    assert!(
        live["task"]["attachments"][0].get("source").is_none(),
        "an owner's image carries no source"
    );

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = [
        root.join("apps/mobile/test/fixtures/crewmate_task_prove_ui.json"),
        root.join("apps/desktop/src/__fixtures__/crewmate_task_prove_ui.json"),
    ];
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        for p in &paths {
            std::fs::write(p, serde_json::to_string_pretty(&live).unwrap() + "\n").unwrap();
        }
    }
    for p in &paths {
        let stored: Value = serde_json::from_str(&std::fs::read_to_string(p).expect(
            "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_prove_ui_fixtures_are_real_payloads",
        ))
        .unwrap();
        assert_eq!(
            stored,
            live,
            "{} changed: rerun with UPDATE_FIXTURES=1",
            p.display()
        );
    }
}
