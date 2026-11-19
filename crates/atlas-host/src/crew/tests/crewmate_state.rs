//! What `crewmate_list` says a crewmate is doing: `idle`, `working`, `needs_you`. It is the one answer every client
//! shows (the desktop's sidebar and header, the phone's deck), so it comes from the tasks' own state machine.
//! Incident: #524 (a crewmate parked on a refused card, or on a question, listed as `working` for as long as the card
//! was open, and nothing told the owner it was his turn).

use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use super::guards::{crewmate_on, project, send, weakening_agent};
use crate::HostClient;
use crate::crew::handlers::crewmates::crewmate_state;
use crate::crew::store::{CrewStore, Task, TaskAction, TaskState, TriggerKind};

async fn listed(client: &HostClient, id: &str) -> Value {
    let v = client.call("crewmate_list", json!({})).await.unwrap();
    v["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .unwrap()
}

/// Wait until `crewmate_list` reports `state` for the crewmate (a state change is stored a moment after it happens).
async fn becomes(client: &HostClient, id: &str, state: &str) -> Value {
    let start = std::time::Instant::now();
    loop {
        let c = listed(client, id).await;
        if c["state"] == state {
            return c;
        }
        assert!(
            start.elapsed() < atlas_core::scaling::HANG,
            "never became {state}: {c}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[test]
fn a_crewmate_that_waits_for_its_owner_is_never_working() {
    // (pending approval, parked task, runner holds a task) -> state
    for (approval, parked, running, want) in [
        (false, false, false, "idle"),
        (false, false, true, "working"),
        (false, true, true, "needs_you"),
        (false, true, false, "needs_you"),
        (true, false, true, "needs_you"),
        (true, true, true, "needs_you"),
    ] {
        assert_eq!(
            crewmate_state(approval, parked, running),
            want,
            "approval {approval}, parked {parked}, running {running}"
        );
    }
}

fn every_card() -> Vec<TaskAction> {
    vec![
        TaskAction::ConnectGithub { reason: "r".into() },
        TaskAction::ProvideSecret {
            name: "K".into(),
            label: "K".into(),
            reason: "r".into(),
        },
        TaskAction::Input {
            label: "l".into(),
            reason: "r".into(),
            placeholder: None,
        },
        TaskAction::Choose {
            question: "q".into(),
            options: vec!["A".into(), "B".into()],
        },
        TaskAction::SignInEngine {
            engine: "omp".into(),
        },
        TaskAction::AddTrigger {
            trigger: TriggerKind::Schedule {
                cron: "0 9 * * *".into(),
            },
            prompt: "p".into(),
            reason: "r".into(),
        },
        TaskAction::Refused {
            reasons: vec!["x".into()],
        },
        TaskAction::ProveUi { reason: "r".into() },
    ]
}

// The state does not depend on the kind of card: the issue's Revisor (an `input` card) showed "Needs you" and its
// Gerente (a `refused` one) "working…", so one kind had been handled and the others not.
#[tokio::test(flavor = "multi_thread")]
async fn every_kind_of_card_makes_its_crewmate_need_you() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let store = CrewStore::new(&dir);
    for action in every_card() {
        let kind = serde_json::to_value(&action).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string();
        let id = create_crewmate(&client, &format!("Card-{kind}")).await;
        // Parked on disk, with no runner: what a card is after a restart, before its runner has picked it up.
        let mut t = Task::new("test", "p");
        t.state = TaskState::WaitingAction;
        t.action = Some(action);
        store.append_task(&id, &t).unwrap();
        let c = listed(&client, &id).await;
        assert_eq!(c["state"], "needs_you", "{kind}: {c}");
        assert_eq!(c["waiting_task"]["id"], t.id.as_str(), "{kind}: {c}");
    }
    client.call("shutdown", json!({})).await.ok();
}

// The incident's own path, end to end on a real runner: a guard refuses the work, the crewmate needs its owner while
// the card is open (and can still be stopped: its task is still the one the runner holds), and is idle once the
// owner has decided.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_task_parks_the_crewmate_as_needs_you_until_the_owner_decides() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    use_agent_bin(&weakening_agent(tmp.path()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "fix the cart").await;
    wait_task_state(
        &client,
        &id,
        &task,
        "waiting_action",
        atlas_core::scaling::HANG,
    )
    .await;

    let parked = listed(&client, &id).await;
    assert_eq!(parked["state"], "needs_you", "{parked}");
    assert_eq!(parked["waiting_task"]["action"]["kind"], "refused");
    assert_eq!(
        parked["running_task"]["id"],
        task.as_str(),
        "the owner can still stop it"
    );
    assert_eq!(
        parked["queued"], 0,
        "nothing queued: this is the card holding the crewmate"
    );

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "override"}),
        )
        .await
        .unwrap();
    wait_task_state(&client, &id, &task, "done", atlas_core::scaling::HANG).await;
    let after = becomes(&client, &id, "idle").await;
    assert!(
        after.get("waiting_task").is_none() && after.get("running_task").is_none(),
        "{after}"
    );
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The whole sequence of a question: working, then needs_you on the card, working again on the answer, idle at the
// end; and a cancelled card frees the crewmate.
#[tokio::test(flavor = "multi_thread")]
async fn a_question_walks_the_crewmate_through_working_needs_you_working_idle() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let ask = "Which?\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Which?\",\"options\":[\"A\",\"B\"]}";
    // Slow enough to be seen working before it parks.
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(2).done(ask).script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Walker").await;
    assert_eq!(listed(&client, &id).await["state"], "idle");
    let task = send(&client, &id, "pick").await;
    becomes(&client, &id, "working").await;
    let parked = becomes(&client, &id, "needs_you").await;
    assert_eq!(parked["waiting_task"]["action"]["kind"], "choose");

    // The answer's run is slow too, so the crewmate is seen working on it.
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(3).done("went with it").script(),
    ));
    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "A"}),
        )
        .await
        .unwrap();
    let working = becomes(&client, &id, "working").await;
    assert!(
        working.get("waiting_task").is_none(),
        "answered: no card any more: {working}"
    );
    wait_task_state(&client, &id, &task, "done", atlas_core::scaling::HANG).await;
    becomes(&client, &id, "idle").await;

    // A second question, cancelled instead of answered: the crewmate is free again.
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done(ask).script(),
    ));
    let second = send(&client, &id, "again").await;
    becomes(&client, &id, "needs_you").await;
    client
        .call("crewmate_cancel", json!({"id": id, "task": second}))
        .await
        .unwrap();
    becomes(&client, &id, "idle").await;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
