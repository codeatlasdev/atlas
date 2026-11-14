//! Creating and describing a crewmate: validation, the look, the caps of a crew server, secrets
//! (never in a reply) and the `crewmate_list` contract the mobile app is tested against.

use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use crate::HostClient;
use crate::server::ServeOpts;

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_create_omp_ask_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    // omp+ask must now succeed: the omp engine supports ask approvals.
    let result = client
        .call(
            "crewmate_create",
            json!({"name": "OmpMate", "role": "x", "agent": "omp", "approval": "ask"}),
        )
        .await;
    match result {
        Err(e) => {
            // Only fail if the error is the old omp+ask restriction message.
            assert!(
                !e.to_string().contains("omp crewmates run with approval"),
                "omp+ask should be allowed, got old restriction error: {e}"
            );
        }
        Ok(v) => {
            // Successful creation: must have an id.
            assert!(
                v.get("error").is_none() && v["id"].is_string(),
                "expected successful crewmate creation, got: {v}"
            );
        }
    }

    client.call("shutdown", json!({})).await.ok();
}

// ── Test 5: invalid cron rejected ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_create_ignores_unknown_env_field() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    // Attempt to inject env via crewmate_create — the field is not in the protocol.
    // serde ignores unknown fields by default, so this must succeed.
    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "EnvMate",
                "role": "test",
                "agent": "claude",
                "approval": "auto",
                "env": {"PATH": "/injected/bin:/usr/bin"}
            }),
        )
        .await
        .expect("crewmate_create with unknown env field should succeed (field ignored)");

    // The crewmate was created; the env key must not appear in the returned object.
    assert!(
        crewmate.get("env").is_none() || crewmate["env"].is_null(),
        "env must not be stored in the crewmate object: {crewmate}"
    );
    // And it must not appear when the crewmate is fetched.
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    let found = list["crewmates"]
        .as_array()
        .and_then(|bs| bs.iter().find(|b| b["name"] == "EnvMate"))
        .cloned()
        .unwrap();
    assert!(
        found.get("env").is_none() || found["env"].is_null(),
        "env must not appear in crewmate_list: {found}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── Test 8: crewmate_list state mapping ───────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn caps_without_atlas_crew_host() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-nocaps");
    // Start WITHOUT crew_host.
    let d = dir.clone();
    tokio::spawn(async move {
        let _ = crate::server::serve_with(&d, ServeOpts::default()).await;
    });
    let mut client = None;
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(&dir, "test").await {
            client = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let client = client.expect("hostd did not start");

    let info = client.call("info", json!({})).await.unwrap();
    let caps = info["caps"].as_array().unwrap();
    assert!(
        !caps.iter().any(|c| c == "crew"),
        "caps must not contain 'crewmates' without crew_host: {caps:?}"
    );

    let result = client
        .call(
            "crewmate_create",
            json!({"name": "x", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await;
    let is_err = match &result {
        Err(_) => true,
        Ok(v) => v.get("error").is_some() || !v["ok"].as_bool().unwrap_or(true),
    };
    assert!(
        is_err,
        "crewmate_create must error without crew_host, got: {result:?}"
    );

    client.call("shutdown", json!({})).await.ok();
}

// ── Test 12: crewmate pane approval held/not held ──────────────────────────────

// ── Test 13: crewmate_list JSON contract — fixture for apps/mobile ────────────
//
// Calls crewmate_list on a REAL daemon.  Sequence:
//   1. Crewmate B runs instant-finish agent  → idle, last present
//   2. Crewmate A runs hang agent            → working, running_task present
//   3. crewmate_list is called               → real reply captured → fixture written
//
// pending_approval is NOT included: holding it requires a desktop subscriber
// to be wired into the test daemon, which is racy.  The field shape is
// already covered by Test 12 (AgentHub::ask) and by the Dart test's null check.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_list_json_contract
#[tokio::test(flavor = "multi_thread")]
async fn crewmate_list_json_contract() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-contract");

    // ── Phase 1: Crewmate B — instant-finish agent (produces "last") ──────────
    // Uses the same script pattern as existing Test 1 and Test 9.
    let stop_script = Hooks::new().done("nightly done").script();
    let stop_bin = fake_agent(tmp.path(), "claude", &stop_script);
    use_agent_bin(&stop_bin);

    let client = daemon(&dir).await;

    // ── Crewmate C: opencode engine (idle) — produces approval_supported:false ─
    let crewmate_c = client
        .call(
            "crewmate_create",
            json!({
                "name": "Deployer", "role": "Deploys releases",
                "agent": "opencode", "approval": "auto",
                "engine": {"kind": "opencode"}
            }),
        )
        .await
        .expect("crewmate_create C");
    let crewmate_c_id = crewmate_c["id"].as_str().unwrap().to_string();

    let crewmate_b = client
        .call(
            "crewmate_create",
            json!({
                "name": "Nightly", "role": "Nightly checks",
                "agent": "claude", "approval": "auto"
            }),
        )
        .await
        .expect("crewmate_create B");
    let crewmate_b_id = crewmate_b["id"].as_str().unwrap().to_string();

    let vb = client
        .call(
            "crewmate_task",
            json!({"id": crewmate_b_id, "prompt": "run checks", "source": "routine"}),
        )
        .await
        .expect("crewmate_task B");
    let task_b_id = vb["task"]["id"].as_str().unwrap().to_string();
    // Crewmate B completes; now crewmate_list will show last.
    wait_task_state(
        &client,
        &crewmate_b_id,
        &task_b_id,
        "done",
        Duration::from_secs(15),
    )
    .await;

    // ── Phase 2: Crewmate A — hanging agent (produces running_task) ───────────
    let hang_bin = fake_agent(tmp.path(), "claude", "#!/bin/sh\nsleep 60\nexit 0\n");
    use_agent_bin(&hang_bin);

    let crewmate_a = client
        .call(
            "crewmate_create",
            json!({
                "name": "Reviewer", "role": "Reviews PRs",
                "agent": "claude", "approval": "ask"
            }),
        )
        .await
        .expect("crewmate_create A");
    let crewmate_a_id = crewmate_a["id"].as_str().unwrap().to_string();

    let va = client
        .call(
            "crewmate_task",
            json!({"id": crewmate_a_id, "prompt": "review PR", "source": "test"}),
        )
        .await
        .expect("crewmate_task A");
    let task_a_id = va["task"]["id"].as_str().unwrap().to_string();

    // Wait until crewmate_list sees crewmate_a as "working".
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let list = client.call("crewmate_list", json!({})).await.unwrap();
        if list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            // The runner reports "working" a beat before the task log records `running`; CI caught
            // the fixture with last.state "queued" (PR #106, Rust (Linux)). Wait for both.
            .any(|b| {
                b["id"] == crewmate_a_id
                    && b["state"] == "working"
                    && b["last"]["state"] == "running"
            })
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "crewmate A never reached working state"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // ── Add triggers to crewmate B for the trigger-parsing contract ────────────
    client
        .call(
            "crewmate_triggers_set",
            json!({
                "id": crewmate_b_id,
                "triggers": [
                    {
                        "id": "t-sched",
                        "kind": {"type": "schedule", "cron": "0 9 * * 1-5"},
                        "enabled": true,
                        "prompt": "Review open PRs",
                        "tz": "America/Sao_Paulo"
                    },
                    {
                        "id": "t-interval",
                        "kind": {"type": "interval", "every_secs": 3600},
                        "enabled": false,
                        "prompt": "Check deployments"
                    }
                ]
            }),
        )
        .await
        .expect("crewmate_triggers_set B");

    // ── Phase 3: capture real crewmate_list ────────────────────────────────────
    let list = client
        .call("crewmate_list", json!({}))
        .await
        .expect("crewmate_list");
    let crewmates = list["crewmates"].as_array().expect("crewmates array");

    let a = crewmates
        .iter()
        .find(|b| b["id"] == crewmate_a_id)
        .expect("crewmate A in list");
    let b = crewmates
        .iter()
        .find(|b| b["id"] == crewmate_b_id)
        .expect("crewmate B in list");
    let c = crewmates
        .iter()
        .find(|b| b["id"] == crewmate_c_id)
        .expect("crewmate C in list");

    // ── Assert the REAL shape ─────────────────────────────────────────────
    // working crewmate: running_task is an OBJECT {id, pane}, not a string
    assert_eq!(a["state"], "working", "crewmate A must be working: {a}");
    let rt = &a["running_task"];
    assert!(
        rt.is_object(),
        "running_task must be {{id,pane}}, got: {rt}"
    );
    assert_eq!(rt["id"].as_str().unwrap(), task_a_id, "running_task.id");
    assert_eq!(
        rt["pane"].as_str().unwrap(),
        &crate::crew::pane::key(&crewmate_a_id, &task_a_id),
        "running_task.pane format"
    );

    // idle crewmate with done task: last is present
    assert_eq!(b["state"], "idle", "crewmate B must be idle: {b}");
    assert!(!b["last"].is_null(), "crewmate B must have last: {b}");
    assert!(
        !b["last"]["text"].as_str().unwrap_or("").is_empty(),
        "last.text must not be empty: {b}"
    );
    assert!(b["last"]["ts"].is_number(), "last.ts must be a number: {b}");
    assert!(
        b["running_task"].is_null(),
        "idle crewmate must have no running_task: {b}"
    );

    // opencode crewmate: approval_supported must be false
    assert_eq!(c["state"], "idle", "crewmate C must be idle: {c}");
    assert_eq!(
        c["approval_supported"].as_bool(),
        Some(false),
        "opencode crewmate must have approval_supported:false in crewmate_list: {c}"
    );
    assert_eq!(
        c["engine"]["kind"].as_str(),
        Some("opencode"),
        "crewmate C must have engine.kind=opencode: {c}"
    );

    // The top-level `paused` of the real reply (the kill switch; off by default).
    assert_eq!(
        list["paused"],
        json!(false),
        "crewmate_list has paused: {list}"
    );

    // ── Build deterministic fixture ───────────────────────────────────────
    // Normalise volatile fields (random UUIDs, real timestamps) so the
    // stored fixture has a stable diff.
    let mut a_fix = a.clone();
    let mut b_fix = b.clone();
    a_fix["id"] = json!("crewmate-working");
    a_fix["running_task"]["id"] = json!("task-working");
    a_fix["running_task"]["pane"] = json!("crew:crewmate-working:task-working");
    a_fix["queued"] = json!(0);
    if !a_fix["last"].is_null() {
        a_fix["last"]["ts"] = json!(1700000001u64);
    }
    b_fix["id"] = json!("crewmate-done");
    b_fix["last"]["ts"] = json!(1700000000u64);
    b_fix["queued"] = json!(0);
    // Normalise trigger IDs, last_fired (server baseline, volatile), and next_fire.
    if let Some(triggers) = b_fix["triggers"].as_array_mut() {
        for t in triggers.iter_mut() {
            if let Some(obj) = t.as_object_mut() {
                obj.remove("last_fired");
            }
            if t["kind"]["type"] == "schedule" {
                t["id"] = json!("t-sched");
                t["next_fire"] = json!(1700100000u64);
            } else if t["kind"]["type"] == "interval" {
                t["id"] = json!("t-interval");
                t["next_fire"] = json!(1700103600u64);
            }
        }
    }
    let mut c_fix = c.clone();
    c_fix["id"] = json!("crewmate-opencode");
    c_fix["queued"] = json!(0);

    let fixture = json!({ "crewmates": [a_fix, b_fix, c_fix], "paused": list["paused"] });

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/crewmate_list.json");

    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).unwrap() + "\n",
        )
        .unwrap();
    }

    let stored: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path).expect(
            "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_list_json_contract",
        ),
    )
    .unwrap();

    // Structural invariant: stored fixture has the same object shape as the live reply.
    let sb = stored["crewmates"].as_array().unwrap();
    let sw = sb
        .iter()
        .find(|b| b["state"] == "working")
        .expect("working crewmate in fixture");
    assert!(
        sw["running_task"].is_object(),
        "fixture running_task must be an object: {sw}"
    );
    assert!(
        sw["running_task"]["id"].is_string(),
        "running_task.id must be string"
    );
    assert!(
        sw["running_task"]["pane"].is_string(),
        "running_task.pane must be string"
    );

    // Structural invariant: stored fixture must include approval_supported:false for opencode.
    let sc = sb
        .iter()
        .find(|b| b["engine"]["kind"] == "opencode")
        .expect("opencode crewmate must be in fixture");
    assert_eq!(
        sc["approval_supported"].as_bool(),
        Some(false),
        "fixture opencode crewmate must have approval_supported:false: {sc}"
    );
    // Structural invariant: stored fixture has triggers on crewmate-done.
    let sd = sb
        .iter()
        .find(|b| b["id"] == "crewmate-done")
        .expect("crewmate-done must be in fixture");
    let striggers = sd["triggers"]
        .as_array()
        .expect("crewmate-done must have triggers array");
    assert!(
        !striggers.is_empty(),
        "crewmate-done fixture must have at least one trigger"
    );
    let sched = striggers
        .iter()
        .find(|t| t["kind"]["type"] == "schedule")
        .expect("crewmate-done must have a schedule trigger");
    assert!(sched["id"].is_string(), "trigger id must be string");
    assert_eq!(sched["kind"]["cron"].as_str(), Some("0 9 * * 1-5"));
    assert_eq!(sched["tz"].as_str(), Some("America/Sao_Paulo"));
    assert!(
        sched["next_fire"].is_number(),
        "schedule trigger must have next_fire"
    );

    assert_eq!(
        stored, fixture,
        "crewmate_list.json changed: rerun with UPDATE_FIXTURES=1"
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The avatar look the user picks in the editor must survive create, update
// and a daemon restart, and reach every client through crewmate_list.
#[tokio::test(flavor = "multi_thread")]
async fn crewmate_look_persists_and_rejects_bad_values() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let look = json!({"color": "#38bdf8", "eyes": "visor", "hat": "wizard"});
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Looky", "role": "", "agent": "claude", "approval": "auto", "look": look}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let listed = |v: &Value| {
        v["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id.as_str())
            .unwrap()["look"]
            .clone()
    };
    assert_eq!(
        listed(&client.call("crewmate_list", json!({})).await.unwrap()),
        look
    );

    // A patch without `look` keeps it; a patch with one replaces it.
    client
        .call("crewmate_update", json!({"id": id, "name": "Looky2"}))
        .await
        .unwrap();
    assert_eq!(
        listed(&client.call("crewmate_list", json!({})).await.unwrap()),
        look
    );
    let look2 = json!({"color": "#22c55e", "eyes": "round", "hat": "none"});
    client
        .call("crewmate_update", json!({"id": id, "look": look2}))
        .await
        .unwrap();

    // Colours end up in SVG fill attributes on every client: only #rrggbb.
    for bad in [
        json!({"color": "url(#x)", "eyes": "round", "hat": "none"}),
        json!({"color": "#abc", "eyes": "round", "hat": "none"}),
        json!({"color": "#22c55e", "eyes": "Round Eyes", "hat": "none"}),
        json!({"color": "#22c55e", "eyes": "round", "hat": "x".repeat(40)}),
    ] {
        let r = client
            .call("crewmate_update", json!({"id": id, "look": bad}))
            .await;
        assert!(r.is_err(), "accepted bad look {bad}");
    }

    client.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let client = daemon(&dir).await;
    assert_eq!(
        listed(&client.call("crewmate_list", json!({})).await.unwrap()),
        look2
    );
    client.call("shutdown", json!({})).await.ok();
}

// Incident (found while documenting Atlas Crew): the desktop editor sends `tool`, `outfit` and `face`
// next to color/eyes/hat, but hostd kept only the first three and rejected hyphenated ids, so
// "round-glasses" could never be saved and a tool or outfit vanished on the next `crewmate_list`.
// The values are the real catalogue of apps/desktop/src/lib/crew/look.ts.
#[tokio::test(flavor = "multi_thread")]
async fn every_look_field_the_editor_sends_survives_update_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Looky").await;
    let list_look = || async {
        let v = client.call("crewmate_list", json!({})).await.unwrap();
        v["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id.as_str())
            .unwrap()["look"]
            .clone()
    };

    // Every face, tool, outfit and hat the editor offers, hyphen included.
    for (hat, tool, outfit, face) in [
        ("straw", "spyglass", "scarf", "round-glasses"),
        ("headphones", "megaphone", "satchel", "eyepatch"),
        ("none", "none", "none", "none"),
    ] {
        let look = json!({
            "color": "#3d5bff", "eyes": "visor",
            "hat": hat, "tool": tool, "outfit": outfit, "face": face,
        });
        client
            .call("crewmate_update", json!({"id": id, "look": look}))
            .await
            .unwrap_or_else(|e| panic!("look {look} refused: {e}"));
        assert_eq!(list_look().await, look, "look changed on the way back");
    }

    // An older client sends only color/eyes/hat: still accepted, and nothing else appears.
    let old = json!({"color": "#22c55e", "eyes": "round", "hat": "none"});
    client
        .call("crewmate_update", json!({"id": id, "look": old}))
        .await
        .unwrap();
    assert_eq!(list_look().await, old);

    // The new fields land in SVG ids on every client: same shape rule as the old ones.
    for bad in [
        json!({"color": "#22c55e", "eyes": "round", "hat": "none", "face": "Round Glasses"}),
        json!({"color": "#22c55e", "eyes": "round", "hat": "none", "tool": "x".repeat(40)}),
        json!({"color": "#22c55e", "eyes": "round", "hat": "none", "outfit": "a/b"}),
        json!({"color": "#22c55e", "eyes": "round", "hat": "none", "face": ""}),
    ] {
        let r = client
            .call("crewmate_update", json!({"id": id, "look": bad}))
            .await;
        assert!(r.is_err(), "accepted bad look {bad}");
    }
    client.call("shutdown", json!({})).await.ok();
}

// ── straw hat is a valid look hat value ───────────────────────────────────
#[test]
fn look_accepts_straw_hat() {
    use crate::crew::store::CrewmateLook;
    let look = CrewmateLook {
        color: "#e8c36a".into(),
        eyes: "round".into(),
        hat: "straw".into(),
        tool: None,
        outfit: None,
        face: None,
    };
    assert!(
        look.validate().is_ok(),
        "straw hat must pass look validation"
    );
}

// ── Test 15: mobile required_scope rejects CrewmateSecretSet ─────────────────

#[test]
fn phone_cannot_call_crewmate_secret_set() {
    use crate::mobile;
    use crate::protocol::HostCmd;
    let cmd = HostCmd::CrewmateSecretSet {
        id: "x".into(),
        key: "k".into(),
        value: "v".into(),
    };
    let scope = mobile::required_scope(&cmd);
    assert!(
        scope.is_err(),
        "CrewmateSecretSet must be unavailable on mobile, got: {scope:?}"
    );
}

// ── Test 16: trigger validation (pure unit) ───────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_secret_set_creates_0600_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name":"SecMate","role":"r","agent":"claude","approval":"auto"}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap();
    client
        .call(
            "crewmate_secret_set",
            json!({"id":id,"key":"MY_KEY","value":"s3cr3t"}),
        )
        .await
        .expect("crewmate_secret_set failed");

    let secrets_path = dir.join("crew").join(id).join("secrets.json");
    assert!(secrets_path.exists(), "secrets.json must exist");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&secrets_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "secrets.json must be 0600, got {:o}",
            mode & 0o777
        );
    }

    // The secret value must not appear anywhere in crewmate_list.
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    assert!(
        !list.to_string().contains("s3cr3t"),
        "secret must not appear in crewmate_list: {list}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── Test 19: crewmate_triggers_set validates bad triggers ─────────────────────
