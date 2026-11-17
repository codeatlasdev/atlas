//! A cron saved under croner 2 that croner 4 refuses (#540): a server upgraded across that change has
//! automations on disk it never wrote with this parser. They must load, show up as invalid, never fire, leave
//! the others alone, and stay on disk as the user saved them; the `?` spellings croner 2 read as `*` keep
//! running as the schedule they always were.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin};
use crate::HostClient;
use crate::crew::store::{CrewStore, Routine, Trigger, TriggerKind};

/// A cron croner 2 read as `5` in the minute field; croner 4 refuses the sign.
const PLUS: &str = "+5 * * * *";
/// `?` in the minute field and `?/2` in the hour field: `*` and `*/2` to croner 2, an error to croner 4 as is.
const QUESTION: &str = "? ?/2 * * *";

async fn stop(client: HostClient) {
    client.call("shutdown", json!({})).await.ok();
    drop(client);
    tokio::time::sleep(Duration::from_millis(600)).await;
}

fn routine(id: &str, cron: &str) -> Routine {
    Routine {
        id: id.into(),
        cron: cron.into(),
        prompt: format!("run {id}"),
        enabled: true,
        last_run: None,
    }
}

fn trigger(id: &str, cron: &str) -> Trigger {
    Trigger {
        id: id.into(),
        kind: TriggerKind::Schedule { cron: cron.into() },
        enabled: true,
        prompt: format!("run {id}"),
        last_fired: None,
        tz: None,
        next_fire: None,
    }
}

async fn crewmate_json(client: &HostClient, id: &str) -> Value {
    let v = client.call("crewmate_list", json!({})).await.unwrap();
    v["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .expect("the crewmate is listed")
}

fn by_id<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap()
}

/// `(id, cron)` of every routine and of every trigger, as the store has them.
type Crons = (Vec<(String, String)>, Vec<(String, String)>);

fn stored_crons(dir: &Path, id: &str) -> Crons {
    let c = CrewStore::new(dir).load_crewmate(id).unwrap();
    let routines = c
        .routines
        .iter()
        .map(|r| (r.id.clone(), r.cron.clone()))
        .collect();
    let triggers = c
        .triggers
        .iter()
        .map(|t| match &t.kind {
            TriggerKind::Schedule { cron } => (t.id.clone(), cron.clone()),
            _ => unreachable!(),
        })
        .collect();
    (routines, triggers)
}

#[tokio::test(flavor = "multi_thread")]
async fn automations_an_older_parser_accepted_load_list_and_run_without_hurting_the_others() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_agent_bin(&fake_agent(tmp.path(), "claude", "exit 0"));

    // A crewmate as the previous build left it: good, `?`, and refused crons, as routines and as triggers.
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "OldCrons").await;
    stop(client).await;
    let store = CrewStore::new(&dir);
    let mut crewmate = store.load_crewmate(&id).unwrap();
    crewmate.routines = vec![
        routine("r-ok", "0 * * * *"),
        routine("r-question", QUESTION),
        routine("r-plus", PLUS),
    ];
    crewmate.triggers = vec![
        trigger("t-ok", "0 9 * * *"),
        trigger("t-question", QUESTION),
        trigger("t-plus", PLUS),
    ];
    store.save_crewmate(&crewmate).unwrap();
    let before = stored_crons(&dir, &id);

    // hostd starts, lists all six, and marks only the refused ones, with the parser's words.
    let client = daemon(&dir).await;
    let c = crewmate_json(&client, &id).await;
    assert_eq!(c["routines"].as_array().unwrap().len(), 3);
    assert_eq!(c["triggers"].as_array().unwrap().len(), 3);
    for (list, ok, question, plus) in [
        (&c["routines"], "r-ok", "r-question", "r-plus"),
        (&c["triggers"], "t-ok", "t-question", "t-plus"),
    ] {
        assert!(by_id(list, ok).get("invalid").is_none());
        assert!(by_id(list, question).get("invalid").is_none(), "{question}");
        let why = by_id(list, plus)["invalid"].as_str().expect(plus);
        assert!(why.starts_with("invalid cron '+5 * * * *': "), "{why}");
    }
    for t in ["t-ok", "t-question"] {
        assert!(
            by_id(&c["triggers"], t)["next_fire"].is_u64(),
            "{t} has a next fire"
        );
    }
    assert!(by_id(&c["triggers"], "t-plus")["next_fire"].is_null());

    // The reply is the fixture of the phone's parser (`apps/mobile/test/crewmate_list_invalid_cron_test.dart`).
    // Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host automations_an_older_parser_accepted
    let mut fix = c.clone();
    fix["id"] = json!("crewmate-oldcrons");
    // The routines that fired at start queued tasks and the fake agent is running one: not part of the contract.
    let fields = fix.as_object_mut().unwrap();
    fields.remove("last");
    fields.remove("running_task");
    fix["state"] = json!("idle");
    fix["queued"] = json!(0);
    for r in fix["routines"].as_array_mut().unwrap() {
        r["last_run"] = Value::Null; // the scheduler may already have fired it
    }
    for t in fix["triggers"].as_array_mut().unwrap() {
        t.as_object_mut().unwrap().remove("last_fired");
        if t["next_fire"].is_u64() {
            t["next_fire"] = json!(1_700_100_000u64);
        }
    }
    let fixture = json!({ "crewmates": [fix], "paused": false });
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/crewmate_list_invalid_cron.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&fixture).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect(
        "fixture missing: UPDATE_FIXTURES=1 cargo test -p atlas-host automations_an_older_parser_accepted",
    ))
    .unwrap();
    assert_eq!(
        stored, fixture,
        "the fixture of apps/mobile is stale: regenerate it"
    );

    // The scheduler ran at start: the parsable routines fired (last_run set), the refused one never does.
    let mut fired = Vec::new();
    for _ in 0..100 {
        fired = CrewStore::new(&dir).load_crewmate(&id).unwrap().routines;
        if fired.iter().filter(|r| r.last_run.is_some()).count() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let ran = |name: &str| {
        fired
            .iter()
            .find(|r| r.id == name)
            .unwrap()
            .last_run
            .is_some()
    };
    assert!(
        ran("r-ok") && ran("r-question"),
        "the others run: {fired:?}"
    );
    assert!(!ran("r-plus"), "a refused cron never fires");

    // Editing the others goes through with the refused ones echoed back unchanged; changing a refused one is
    // validated like any new cron, and fixing it works.
    let routines = |plus: &str, ok_enabled: bool| {
        json!([
            {"id": "r-ok", "cron": "0 * * * *", "prompt": "run r-ok", "enabled": ok_enabled},
            {"id": "r-question", "cron": QUESTION, "prompt": "run r-question", "enabled": true},
            {"id": "r-plus", "cron": plus, "prompt": "run r-plus", "enabled": true},
        ])
    };
    let triggers = |plus: &str, ok_enabled: bool| {
        json!([
            {"id": "t-ok", "kind": {"type": "schedule", "cron": "0 9 * * *"}, "enabled": ok_enabled, "prompt": "run t-ok"},
            {"id": "t-question", "kind": {"type": "schedule", "cron": QUESTION}, "enabled": true, "prompt": "run t-question"},
            {"id": "t-plus", "kind": {"type": "schedule", "cron": plus}, "enabled": true, "prompt": "run t-plus"},
        ])
    };
    client
        .call(
            "crewmate_routine_set",
            json!({"id": id, "routines": routines(PLUS, false)}),
        )
        .await
        .expect("a refused routine does not block editing the others");
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": triggers(PLUS, false)}),
        )
        .await
        .expect("a refused trigger does not block editing the others");
    assert_eq!(stored_crons(&dir, &id), before, "every cron stays as saved");

    let e = client
        .call(
            "crewmate_routine_set",
            json!({"id": id, "routines": routines("+6 * * * *", true)}),
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("invalid cron '+6 * * * *'"), "{e}");
    let e = client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": triggers("+6 * * * *", true)}),
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("invalid cron '+6 * * * *'"), "{e}");
    assert_eq!(
        stored_crons(&dir, &id),
        before,
        "a refused edit changes nothing"
    );

    client
        .call(
            "crewmate_routine_set",
            json!({"id": id, "routines": routines("5 * * * *", true)}),
        )
        .await
        .unwrap();
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": triggers("5 * * * *", true)}),
        )
        .await
        .unwrap();
    let c = crewmate_json(&client, &id).await;
    assert!(by_id(&c["routines"], "r-plus").get("invalid").is_none());
    assert!(by_id(&c["triggers"], "t-plus").get("invalid").is_none());
    assert!(by_id(&c["triggers"], "t-plus")["next_fire"].is_u64());
    client.call("shutdown", json!({})).await.ok();
}
