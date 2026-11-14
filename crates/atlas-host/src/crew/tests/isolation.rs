//! Crewmates as their own Linux users (issue #200), through a real hostd, a real spawn helper and a real
//! provisioning service (`spawn::testing`): only `useradd`/`userdel`/`nft` and the user lookup are
//! replaced, because the `atlas-cm-*` accounts do not exist on a developer's machine. The tests that need
//! real accounts are in `spawn::exec` and `spawn::provision`, `#[ignore]`d and run by the root CI job.

use std::time::Duration;

use serde_json::json;

use super::fixture::{daemon_with, lock_agent_bin, wait_task_state};
use crate::crew::spawn::testing::{Rig, rig};

async fn create(client: &crate::HostClient, name: &str) -> String {
    let v = client
        .call(
            "crewmate_create",
            json!({"name": name, "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .expect("crewmate_create failed");
    v["id"].as_str().unwrap().to_string()
}

fn user_of(id: &str) -> String {
    crate::crew::spawn::validate::user_for(id).unwrap()
}

fn slug_dir(r: &Rig, id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(r.setup().root).join(user_of(id).trim_start_matches("atlas-cm-"))
}

// Creating a crewmate creates its Linux user and directories first; nothing is saved without them.
#[tokio::test(flavor = "multi_thread")]
async fn creating_a_crewmate_provisions_its_user_and_directories() {
    let r = rig();
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    let user = user_of(&id);
    let log = r.prov_log.lock().clone();
    assert!(
        log.iter()
            .any(|l| l.starts_with("useradd") && l.ends_with(&user)),
        "no useradd for {user}: {log:?}"
    );
    let dir = slug_dir(&r, &id);
    for d in ["home", "work", "crew-runtime"] {
        assert!(dir.join(d).is_dir(), "{d} missing in {}", dir.display());
    }
    assert!(dir.join("memory.md").is_file());
    client.call("shutdown", json!({})).await.ok();
}

// A server where the account cannot be made (out of system uids) refuses the crewmate with the reason
// and keeps nothing: no definition that would fail every task.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_provision_creates_no_crewmate_and_says_why() {
    let r = rig();
    *r.fail_useradd.lock() = Some("out of system user ids".into());
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let e = client
        .call(
            "crewmate_create",
            json!({"name": "Ana", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("out of system user ids"), "{e}");
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    assert!(list["crewmates"].as_array().unwrap().is_empty());
    client.call("shutdown", json!({})).await.ok();
}

// Deleting removes the user, its processes and its directory.
#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_crewmate_removes_its_user_and_directory() {
    let r = rig();
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    let (user, dir) = (user_of(&id), slug_dir(&r, &id));
    assert!(dir.is_dir());
    r.prov_log.lock().clear();
    client
        .call("crewmate_delete", json!({"id": id}))
        .await
        .unwrap();
    let log = r.prov_log.lock().clone();
    assert!(log.contains(&format!("kill {user}")), "{log:?}");
    assert!(log.contains(&format!("userdel {user}")), "{log:?}");
    assert!(!dir.exists(), "the crewmate's directory outlived it");
    client.call("shutdown", json!({})).await.ok();
}

// Without a helper nothing changes: no provisioning, the work tree stays in the crew home.
#[tokio::test(flavor = "multi_thread")]
async fn without_a_helper_crewmates_are_created_exactly_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon_with(&dir, None).await;
    let id = create(&client, "Ana").await;
    assert!(dir.join("crew").join(&id).join("crewmate.json").is_file());
    client
        .call("crewmate_delete", json!({"id": id}))
        .await
        .unwrap();
    client.call("shutdown", json!({})).await.ok();
}

// The whole path for a terminal agent: the task runs `claude` as the crewmate through the helper, in
// the crewmate's own directory, with its own hook endpoint and token; it reports through them and the
// task ends done.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_terminal_agent_runs_in_its_directory_and_reports_through_its_own_endpoint() {
    let _bin_lock = lock_agent_bin().await;
    let r = rig();
    // The fake claude proves where it runs and what it was given, then reports done like a real hook.
    // The helper pins the agent's PATH to the engine directories, so `curl` is named by path.
    let report = format!(
        "CURL=/usr/bin/curl; echo \"cwd=$(pwd)\" > \"$ATLAS_HOOK_ENDPOINT.seen\"; echo \"token=$ATLAS_HOOK_TOKEN\" >> \"$ATLAS_HOOK_ENDPOINT.seen\"; echo \"endpoint=$ATLAS_HOOK_ENDPOINT\" >> \"$ATLAS_HOOK_ENDPOINT.seen\"; {}",
        super::fixture::Hooks::new().done("ok").script()
    );
    r.engine("claude", &report);
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, &id, &task_id, "done", Duration::from_secs(20)).await;

    let dir = slug_dir(&r, &id);
    // The endpoint file is in the crewmate's runtime dir, and it is the crewmate's own token, not hostd's.
    let endpoint = dir.join("crew-runtime/hook-endpoint.env");
    let file = std::fs::read_to_string(&endpoint).unwrap();
    let own = std::fs::read_to_string(hostd.join("crew").join(&id).join("hook-token")).unwrap();
    assert!(
        file.contains(&format!("ATLAS_HOOK_TOKEN={}", own.trim())),
        "{file}"
    );
    let global = std::fs::read_to_string(hostd.join("desktop/hook-endpoint.env")).unwrap();
    let global_token = global
        .lines()
        .find_map(|l| l.strip_prefix("ATLAS_HOOK_TOKEN="))
        .unwrap();
    assert!(
        !file.contains(global_token),
        "the global token reached the agent's files"
    );
    // What the agent saw: its own directory, its own endpoint file and token.
    let seen = std::fs::read_to_string(dir.join("crew-runtime/hook-endpoint.env.seen")).unwrap();
    let real_work = std::fs::canonicalize(dir.join("work")).unwrap();
    assert!(
        seen.contains(&format!("cwd={}", real_work.display())),
        "{seen}"
    );
    assert!(seen.contains(&format!("token={}", own.trim())), "{seen}");
    assert!(!seen.contains(global_token), "{seen}");
    client.call("shutdown", json!({})).await.ok();
}

/// POST `body` to `path` of the hook server on `port` with `token`; the status line's code.
async fn post(port: &str, token: &str, path: &str, body: &serde_json::Value) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let body = body.to_string();
    let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nX-Atlas-Hook-Token: {token}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).await.unwrap();
    r.split_whitespace().nth(1).unwrap().parse().unwrap()
}

// The isolation property of the hook endpoint, over a real socket: a crewmate's token is accepted for
// its own routes and refused (403) for another crewmate's, for webhooks and for anything else; a wrong
// token is a 401; and the global token still opens everything (the desktop's agents rely on it).
#[tokio::test(flavor = "multi_thread")]
async fn a_crewmates_hook_token_opens_only_its_own_routes() {
    let r = rig();
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let (a, b) = (create(&client, "A").await, create(&client, "B").await);
    let store = crate::crew::store::CrewStore::new(&hostd);
    let (ta, tb) = (
        store.ensure_hook_token(&a).unwrap(),
        store.ensure_hook_token(&b).unwrap(),
    );
    let endpoint = std::fs::read_to_string(hostd.join("desktop/hook-endpoint.env")).unwrap();
    let get = |k: &str| {
        endpoint
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let (port, global) = (get("ATLAS_HOOK_PORT"), get("ATLAS_HOOK_TOKEN"));
    let own_pane = format!("crew:{a}:t1");
    let others_pane = format!("crew:{b}:t1");

    // A's token: its own approval stream, yes; B's pane or B's routes, no.
    let ev = json!({"pane_key": own_pane, "payload": {"event": "agent_end", "text": "x"}});
    assert_eq!(post(&port, &ta, "/hook/claude", &ev).await, 204);
    let foreign = json!({"pane_key": others_pane, "payload": {"event": "agent_end", "text": "x"}});
    assert_eq!(post(&port, &ta, "/hook/claude", &foreign).await, 403);
    assert_eq!(
        post(
            &port,
            &ta,
            &format!("/crew/{b}/set_profile"),
            &json!({"name": "pwned"})
        )
        .await,
        403
    );
    assert_eq!(
        post(
            &port,
            &ta,
            &format!("/crew/{a}/set_profile"),
            &json!({"name": "Mine"})
        )
        .await,
        204
    );
    // The webhook route is for outsiders with the webhook's token, never an agent.
    assert_eq!(
        post(&port, &ta, &format!("/crew/{a}/hook/anything"), &json!({})).await,
        403
    );
    // B was not renamed by A's attempt.
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    let name_of = |id: &str| {
        list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!((name_of(&a).as_str(), name_of(&b).as_str()), ("Mine", "B"));
    // B's token is B's.
    let bev = json!({"pane_key": others_pane, "payload": {"event": "agent_end", "text": "x"}});
    assert_eq!(post(&port, &tb, "/hook/claude", &bev).await, 204);
    // Nobody's token, and an almost-right one.
    assert_eq!(post(&port, "nope", "/hook/claude", &ev).await, 401);
    assert_eq!(
        post(&port, &ta[..ta.len() - 1], "/hook/claude", &ev).await,
        401
    );
    // The global token is unchanged.
    assert_eq!(post(&port, &global, "/hook/claude", &foreign).await, 204);
    client.call("shutdown", json!({})).await.ok();
}

// A headless engine of an isolated crewmate runs through the helper too: `claude -p` as the crewmate,
// in its directory, with its MCP config in `crew-runtime/` (the agent reads it there), secrets in its
// environment only, and the turn's reply and session come back as before.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_engine_turn_runs_through_the_helper_with_its_files_in_crew_runtime() {
    let _bin_lock = lock_agent_bin().await;
    crate::crew::engine::set_test_engine_bin(None);
    let r = rig();
    // The fake claude reads the MCP config it was told about (proving it is readable) and answers in
    // the JSON shape of `claude -p --output-format json`.
    r.engine(
        "claude",
        r#"cfg=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp-config" ] && cfg="$a"; prev="$a"; done
n=$(/usr/bin/wc -c < "$cfg" | /usr/bin/tr -d ' ')
printf '{"type":"result","session_id":"ses-iso","result":"cwd=%s cfg=%s secret=%s","is_error":false}\n' "$(pwd)" "$cfg" "$MY_SECRET""#,
    );
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "E", "role": "r", "agent": "claude", "approval": "auto",
                   "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "MY_SECRET", "value": "s3cr3t-value"}),
        )
        .await
        .unwrap();
    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = t["task"]["id"].as_str().unwrap().to_string();
    let task = wait_task_state(&client, &id, &task_id, "done", Duration::from_secs(20)).await;
    let reply = task["reply"]
        .as_str()
        .or(task["result"].as_str())
        .unwrap()
        .to_string();
    let dir = slug_dir(&r, &id);
    let real_work = std::fs::canonicalize(dir.join("work")).unwrap();
    assert!(
        reply.contains(&format!("cwd={}", real_work.display())),
        "{reply}"
    );
    // The MCP config the agent was pointed at is in crew-runtime/, not in the crew home.
    assert!(
        reply.contains(&format!(
            "cfg={}",
            dir.join("crew-runtime/mcp-config.json").display()
        )),
        "{reply}"
    );
    assert!(dir.join("crew-runtime/mcp-config.json").is_file());
    assert!(
        !hostd
            .join("crew")
            .join(&id)
            .join("mcp-config.json")
            .exists()
    );
    // The secret reached the agent's environment (through the helper's socket, never a file).
    assert!(reply.contains("secret=s3cr3t-value"), "{reply}");
    client.call("shutdown", json!({})).await.ok();
}

// A crewmate that exists without a Linux user (imported from an archive, or created before the helper
// was installed) is provisioned the first time it works, and its memory moves into its own directory so
// the agent can reach it: neither a failed task nor lost memory.
#[tokio::test(flavor = "multi_thread")]
async fn a_crewmate_without_a_user_is_provisioned_by_its_first_task_and_keeps_its_memory() {
    let _bin_lock = lock_agent_bin().await;
    crate::crew::engine::set_test_engine_bin(None);
    let r = rig();
    r.engine(
        "claude",
        r#"printf '{"type":"result","session_id":"s","result":"mem=%s","is_error":false}\n' "$(/bin/cat "$MEMORY_FILE")""#,
    );
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    // The crewmate was made by a hostd without a helper: definition and memory in the crew home.
    let plain = crate::crew::store::CrewStore::new(&hostd);
    let id = "5a2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
    plain
        .save_crewmate(&crate::crew::Crewmate {
            id: id.into(),
            name: "Old".into(),
            role: "r".into(),
            agent: "claude".into(),
            workdir: None,
            approval: crate::crew::store::Approval::Auto,
            routines: vec![],
            triggers: vec![],
            look: None,
            engine: Some(crate::crew::store::EngineConfig {
                kind: "claude".into(),
                model: None,
                provider: None,
                base_url: None,
            }),
            project: None,
            setup_step: None,
            short_replies: None,
            created_at: 1,
        })
        .unwrap();
    plain.write_memory(id, "remember the milk").unwrap();
    plain.set_secret(id, "MEMORY_FILE", "unused").unwrap();

    let client = daemon_with(&hostd, Some(r.setup())).await;
    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = t["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, id, &task_id, "done", Duration::from_secs(20)).await;
    let user = user_of(id);
    assert!(
        r.prov_log
            .lock()
            .iter()
            .any(|l| l.starts_with("useradd") && l.ends_with(&user)),
        "{:?}",
        r.prov_log.lock()
    );
    // The memory is now where the agent (and the prompt) look, with its content.
    let moved = slug_dir(&r, id).join("memory.md");
    assert_eq!(
        std::fs::read_to_string(&moved).unwrap(),
        "remember the milk"
    );
    // The UI reads and edits it there too.
    let v = client
        .call("crewmate_memory", json!({"id": id}))
        .await
        .unwrap();
    assert_eq!(v["text"], "remember the milk");
    client.call("shutdown", json!({})).await.ok();
}

// Backups follow the memory: an export on an isolated server contains what is in the crewmate's own
// directory, not the stale crew-home copy.
#[tokio::test(flavor = "multi_thread")]
async fn an_export_of_an_isolated_crewmate_carries_its_current_memory() {
    let r = rig();
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    client
        .call(
            "crewmate_memory_set",
            json!({"id": id, "text": "likes tabs"}),
        )
        .await
        .unwrap();
    let store = crate::crew::store::CrewStore::new(&hostd).with_isolation(Some(r.setup()));
    let mut out = Vec::new();
    crate::crew::store::archive::export(&store, &[], false, &mut out).unwrap();
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(&out[..]));
    let mut found = None;
    for e in a.entries().unwrap() {
        let mut e = e.unwrap();
        if e.path().unwrap().ends_with("memory.md") {
            let mut t = String::new();
            std::io::Read::read_to_string(&mut e, &mut t).unwrap();
            found = Some(t);
        }
    }
    assert_eq!(found.as_deref(), Some("likes tabs"));
    client.call("shutdown", json!({})).await.ok();
}
