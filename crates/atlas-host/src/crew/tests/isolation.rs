//! Crewmates as their own Linux users (issue #200), through a real hostd, a real spawn helper and a real
//! provisioning service (`spawn::testing`): only `useradd`/`userdel`/`nft` and the user lookup are
//! replaced, because the `atlas-cm-*` accounts do not exist on a developer's machine. The tests that need
//! real accounts are in `spawn::exec` and `spawn::provision`, `#[ignore]`d and run by the root CI job.

use std::time::Duration;

use serde_json::json;

use super::fixture::{daemon_with, lock_agent_bin, wait_task_state};
use crate::crew::spawn::testing::{Rig, rig, rig_with};

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
    wait_task_state(&client, &id, &task_id, "done", atlas_core::scaling::HANG).await;

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
    let task = wait_task_state(&client, &id, &task_id, "done", atlas_core::scaling::HANG).await;
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
    wait_task_state(&client, id, &task_id, "done", atlas_core::scaling::HANG).await;
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

// ── Engine sign-in per crewmate (#364 meets #200) ───────────────────────────────────────────────────
// A crewmate's engine runs as its own user with its own $HOME, so its sign-in is a file in that home: a
// terminal or a status probe run as the crew user would sign in (or report) the wrong account. The phone
// names the crewmate; the terminal and the probe then run through the helper as that user.

/// What the terminal `pty` of the hostd at `dir` printed, read (replay included) until `needle` shows up.
async fn read_pty(dir: &std::path::Path, pty: &str, needle: &str) -> String {
    use tokio::io::AsyncReadExt;
    let mut raw = crate::client::attach_raw(dir, pty, true).await.unwrap();
    let mut out = String::new();
    let mut buf = [0u8; 4096];
    let read = async {
        while !out.contains(needle) {
            match raw.reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    };
    let _ = tokio::time::timeout(atlas_core::scaling::HANG, read).await;
    out
}

/// The pty ids and argv of the terminals hostd lists.
async fn ptys(client: &crate::HostClient) -> Vec<serde_json::Value> {
    client.call("pty_list", json!({})).await.unwrap()["ptys"]
        .as_array()
        .unwrap()
        .clone()
}

// The login terminal of an isolated crewmate is the engine's allowlisted login argv, started as that
// crewmate by the helper, in its own home: what it prints proves the user, the home and the directory.
#[tokio::test(flavor = "multi_thread")]
async fn an_engine_login_for_an_isolated_crewmate_runs_as_that_crewmate_in_its_home() {
    let _g = lock_agent_bin().await;
    let r = rig();
    // `/bin/echo`: the helper pins the PATH to the engine directory.
    r.engine(
        "claude",
        r#"/bin/echo "login:$* home=$HOME user=$USER cwd=$(/bin/pwd)"; /bin/sleep 30"#,
    );
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    let v = client
        .call(
            "crew_engine_login",
            json!({"engine": "claude", "crewmate": id}),
        )
        .await
        .unwrap();
    let pty = v["pty"].as_str().unwrap().to_string();
    let listed = ptys(&client).await;
    let p = listed.iter().find(|p| p["pty"] == pty).expect("listed");
    // Not an agent pane: it must never report to the hook server as a crewmate.
    assert!(p["scope"]["pane_key"].is_null(), "{p}");
    // What the terminal printed (read through the real pty attach).
    let dir = slug_dir(&r, &id);
    // `$HOME` is the path the helper was given; `pwd` is the resolved one (a temp dir is a symlink on macOS).
    let home = dir.join("home");
    let real_home = std::fs::canonicalize(&home).unwrap();
    let out = read_pty(&hostd, &pty, "cwd=").await;
    assert!(out.contains("login:auth login"), "{out}");
    assert!(out.contains(&format!("home={}", home.display())), "{out}");
    assert!(out.contains(&format!("user={}", user_of(&id))), "{out}");
    assert!(
        out.contains(&format!("cwd={}", real_home.display())),
        "{out}"
    );
    client.call("pty_kill", json!({"pty": pty})).await.unwrap();
    client.call("shutdown", json!({})).await.ok();
}

// Never a silent fallback to the crew user: signing the wrong account in is worse than an error.
#[tokio::test(flavor = "multi_thread")]
async fn an_engine_login_for_a_crewmate_that_is_not_isolated_is_refused_and_opens_nothing() {
    let _g = lock_agent_bin().await;
    let r = rig();
    r.engine("claude", "/bin/sleep 30");
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    for (why, crewmate) in [
        ("not a crewmate id", "nobody"),
        (
            "a crewmate id without a definition",
            "9a2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b",
        ),
    ] {
        let e = client
            .call(
                "crew_engine_login",
                json!({"engine": "claude", "crewmate": crewmate}),
            )
            .await
            .expect_err(why);
        assert!(format!("{e:#}").contains("crewmate"), "{why}: {e:#}");
    }
    // A server without a helper has no per-crewmate sign-in at all.
    let plain = daemon_with(&tmp.path().join("plain"), None).await;
    let e = plain
        .call(
            "crew_engine_login",
            json!({"engine": "claude", "crewmate": "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b"}),
        )
        .await
        .expect_err("no helper");
    assert!(format!("{e:#}").contains("own Linux user"), "{e:#}");
    assert!(ptys(&client).await.is_empty() && ptys(&plain).await.is_empty());
    client.call("shutdown", json!({})).await.ok();
    plain.call("shutdown", json!({})).await.ok();
}

// The status of an engine is the crewmate's own: two crewmates of one server can differ (one signed in,
// the other not), and neither is the crew user's.
#[tokio::test(flavor = "multi_thread")]
async fn engine_status_for_a_crewmate_asks_the_engine_as_that_crewmate() {
    let _g = lock_agent_bin().await;
    let r = rig();
    // Signed in exactly when a token file exists in the caller's own $HOME.
    r.engine(
        "claude",
        r#"if [ "$1 $2" = "auth status" ]; then [ -f "$HOME/token" ] && exit 0; exit 1; fi; exit 0"#,
    );
    // The crew user's status runs the same fake, not whatever claude is on this machine's PATH: on a developer
    // Mac signed in to claude the last assertion failed (CI has no claude, so it passed there).
    let _path = crate::crew::engine_login::TestPath::set(r.engines.clone());
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    let state = |s: &serde_json::Value| {
        s["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["engine"] == "claude")
            .unwrap()["state"]
            .clone()
    };
    let before = client
        .call(
            "crew_engine_status",
            json!({"refresh": true, "crewmate": id}),
        )
        .await
        .unwrap();
    assert_eq!(state(&before), "needs_signin", "{before}");
    std::fs::write(slug_dir(&r, &id).join("home/token"), "x").unwrap();
    let after = client
        .call(
            "crew_engine_status",
            json!({"refresh": true, "crewmate": id}),
        )
        .await
        .unwrap();
    assert_eq!(state(&after), "signed_in", "{after}");
    // Another subject keeps its own answer: asking for the crew user's does not see the token.
    let crew = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_ne!(state(&crew), "signed_in", "{crew}");
    client.call("shutdown", json!({})).await.ok();
}

// A task of an isolated crewmate that fails with the engine's own auth error flips THAT crewmate's
// sign-in, not the crew user's (which may be fine), and the push says whose.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_sign_in_of_an_isolated_crewmate_flips_its_own_row_only() {
    let _g = lock_agent_bin().await;
    crate::crew::engine::set_test_engine_bin(None);
    let r = rig();
    // `auth status` says signed in, a turn fails with claude's own auth error. The crew user's status runs
    // this same fake (not the developer's real `claude`, signed in on one machine and not on another),
    // so its row starts `signed_in` everywhere.
    r.engine(
        "claude",
        r#"if [ "$1 $2" = "auth status" ]; then echo '{"loggedIn":true}'; exit 0; fi
echo '{"type":"result","is_error":true,"result":"Please run /login","session_id":"s"}'"#,
    );
    let _path = crate::crew::engine_login::TestPath::set(r.engines.clone());
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let id = create(&client, "Ana").await;
    // The create above defaulted to `claude` with no engine config: make it a headless claude crewmate.
    client
        .call(
            "crewmate_update",
            json!({"id": id, "engine": {"kind": "claude"}}),
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
    wait_task_state(&client, &id, &task_id, "failed", atlas_core::scaling::HANG).await;
    let row = |s: &serde_json::Value| {
        s["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["engine"] == "claude")
            .unwrap()
            .clone()
    };
    let hers = client
        .call("crew_engine_status", json!({"crewmate": id}))
        .await
        .unwrap();
    assert_eq!(row(&hers)["state"], "needs_signin", "{hers}");
    let crew = client.call("crew_engine_status", json!({})).await.unwrap();
    assert_ne!(
        row(&crew)["state"],
        "needs_signin",
        "the crew user's sign-in did not fail: {crew}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// The auth list and the dispatcher are two places that must agree, over the real hook server: every
// route an agent's MCP server posts to must reach its handler with the crewmate's own token (never 403,
// never 404), and the same route for another crewmate must be refused. `ask` blocks until answered, so
// it is left out of the live call and covered by the pure test.
#[tokio::test(flavor = "multi_thread")]
async fn every_agent_route_reaches_its_handler_with_the_crewmates_own_token() {
    let r = rig();
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let client = daemon_with(&hostd, Some(r.setup())).await;
    let (a, b) = (create(&client, "A").await, create(&client, "B").await);
    let store = crate::crew::store::CrewStore::new(&hostd);
    let ta = store.ensure_hook_token(&a).unwrap();
    let endpoint = std::fs::read_to_string(hostd.join("desktop/hook-endpoint.env")).unwrap();
    let port = endpoint
        .lines()
        .find_map(|l| l.strip_prefix("ATLAS_HOOK_PORT="))
        .unwrap()
        .to_string();
    let pane = format!("crew:{a}:no-such-task");
    for sub in crate::agents::hook_auth::AGENT_ROUTES {
        if sub == "ask" {
            continue;
        }
        let own = post(
            &port,
            &ta,
            &format!("/crew/{a}/{sub}"),
            &json!({"pane_key": pane}),
        )
        .await;
        assert!(
            own != 403 && own != 404 && own != 401,
            "{sub}: the crewmate's own token got {own}"
        );
        let other = post(&port, &ta, &format!("/crew/{b}/{sub}"), &json!({})).await;
        assert_eq!(other, 403, "{sub}: another crewmate's route");
    }
    client.call("shutdown", json!({})).await.ok();
}

/// An isolated engine crewmate whose fake `claude` runs `script`, one task through the real helper.
/// Returns the finished task's `(state, error)`.
async fn isolated_engine_task_that_dies(r: Rig) -> (String, String) {
    isolated_task_that_dies(r, "claude", "echo partial; kill -9 $$").await
}

/// [`isolated_engine_task_that_dies`] for engine `kind` whose fake binary runs `script`.
async fn isolated_task_that_dies(r: Rig, kind: &str, script: &str) -> (String, String) {
    let _bin_lock = lock_agent_bin().await;
    crate::crew::engine::set_test_engine_bin(None);
    // SIGKILL itself: what the kernel's OOM killer does to the engine.
    r.engine(kind, script);
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "M", "role": "r", "agent": kind, "approval": "auto",
                   "engine": {"kind": kind}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = t["task"]["id"].as_str().unwrap().to_string();
    let task = wait_task_state(&client, &id, &task_id, "failed", atlas_core::scaling::HANG).await;
    client.call("shutdown", json!({})).await.ok();
    (
        task["state"].as_str().unwrap_or("").to_string(),
        task["error"].as_str().unwrap_or("").to_string(),
    )
}

// #296: when the helper says the kernel killed the agent for memory, the task says so and names the
// setting to raise, instead of a bare "exited 137" the owner cannot act on.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_the_kernel_killed_for_memory_fails_its_task_with_the_cap_and_the_setting() {
    let (state, error) = isolated_engine_task_that_dies(rig_with(
        crate::crew::spawn::server::Memory::Verdict(3 << 30),
    ))
    .await;
    assert_eq!(state, "failed");
    assert!(
        error.contains("ran out of memory (cap 3 GiB); raise agent_memory_max"),
        "{error}"
    );
    assert!(!error.contains("exited 137"), "{error}");
}

// The other half of the contract: a kill that was not the memory cap's is reported as the engine reports
// it (here: no result in what it printed), so a crash is never blamed on memory.
#[tokio::test(flavor = "multi_thread")]
async fn a_kill_that_was_not_the_memory_cap_keeps_its_exit_code() {
    let (state, error) = isolated_engine_task_that_dies(rig()).await;
    assert_eq!(state, "failed");
    assert!(!error.contains("ran out of memory"), "{error}");
    assert!(error.contains("no result JSON in output"), "{error}");
}

// omp talks to hostd over stdin/stdout for the whole turn and does not exit by itself, so an OOM kill used to
// end the stream and look like a turn that said nothing: the task must fail with the cap instead.
#[tokio::test(flavor = "multi_thread")]
async fn an_omp_turn_the_kernel_killed_for_memory_fails_with_the_cap() {
    let (state, error) = isolated_task_that_dies(
        rig_with(crate::crew::spawn::server::Memory::Verdict(2 << 30)),
        "omp",
        "kill -9 $$",
    )
    .await;
    assert_eq!(state, "failed");
    assert!(
        error.contains("ran out of memory (cap 2 GiB); raise agent_memory_max"),
        "{error}"
    );
}

// The control for omp: a kill that was not the memory cap's is never reported as an OOM, whatever the
// stream looked like.
#[tokio::test(flavor = "multi_thread")]
async fn an_omp_turn_that_dies_for_another_reason_is_not_blamed_on_memory() {
    // No verdict from the helper: whatever the task says, it is not about memory.
    let _bin_lock = lock_agent_bin().await;
    crate::crew::engine::set_test_engine_bin(None);
    let r = rig();
    r.engine("omp", "kill -9 $$");
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon_with(&tmp.path().join("hostd"), Some(r.setup())).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "M", "role": "r", "agent": "omp", "approval": "auto",
                   "engine": {"kind": "omp"}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = t["task"]["id"].as_str().unwrap().to_string();
    // It may end done (an empty reply) or failed: both are the engine's story, neither mentions memory.
    let deadline = std::time::Instant::now() + atlas_core::scaling::HANG;
    let task = loop {
        let l = client
            .call("crewmate_tasks", json!({"id": id, "limit": 20}))
            .await
            .unwrap();
        let found = l["tasks"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["id"] == task_id.as_str()))
            .cloned();
        if let Some(t) = found
            && matches!(t["state"].as_str(), Some("done" | "failed"))
        {
            break t;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the task never finished"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let said = format!("{} {}", task["error"], task["result"]);
    assert!(!said.contains("out of memory"), "{said}");
    client.call("shutdown", json!({})).await.ok();
}

// Regression (security review of #448, should-fix 1): isolation was on only if the helper's socket existed when
// hostd started, so a server whose socket unit was stopped (or never started, #430) quietly ran every crewmate
// as the crew user again. With isolation installed and no helper, a task fails with the reason and the agent is
// never started, as nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_isolation_installed_and_no_helper_refuses_tasks_and_starts_no_agent() {
    use super::fixture::{daemon_with_fault, fake_agent, use_agent_bin};
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("agent-started");
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &format!("echo started > \"{}\"", marker.display()),
    );
    use_agent_bin(&agent);
    let why = "isolation is installed but the spawn helper's socket /run/x is missing";
    let client = daemon_with_fault(&tmp.path().join("hostd"), why).await;
    let id = create(&client, "Ana").await;
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();
    let t = wait_task_state(&client, &id, &task_id, "failed", atlas_core::scaling::HANG).await;
    assert!(t["error"].as_str().unwrap().contains(why), "{t}");
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!marker.exists(), "the agent was started without isolation");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
