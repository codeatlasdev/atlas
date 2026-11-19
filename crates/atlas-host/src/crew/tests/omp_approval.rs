//! omp's `approval: ask`, through the REAL generated approval extension (`omp-approval.ts`) loaded by a fake omp
//! (`fake_omp.mjs`) that puts omp's own limit on the extension's `tool_call` handler and runs on a virtual clock,
//! so an owner who answers 31 s late costs a third of a second. Incident: #524 (omp abandoned every handler
//! after its default 30 s, `extensionHandlers.toolCallTimeoutMs`, whatever the owner decided).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::HostClient;
use crate::agents::hook_server::CREWMATE_DECISION_WAIT;
use crate::crew::engine::omp;

/// Virtual seconds per real second of the fake omp's clock.
const SCALE: f64 = 100.0;

fn have_node() -> bool {
    let ok = Command::new("node").arg("--version").output().is_ok();
    if !ok {
        eprintln!("SKIP: node is not installed, the fake omp cannot run the real extension");
    }
    ok
}

/// The fake omp as a program: `node fake_omp.mjs` on a virtual clock of `scale`.
fn fake_omp(dir: &Path, scale: f64) -> PathBuf {
    let mjs = dir.join("fake_omp.mjs");
    std::fs::write(&mjs, include_str!("fake_omp.mjs")).unwrap();
    fake_agent(
        dir,
        "omp",
        &format!(
            "FAKE_OMP_TIME_SCALE={scale} exec node '{}' \"$@\"",
            mjs.display()
        ),
    )
}

async fn omp_crewmate(client: &HostClient) -> String {
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Revisor", "role": "r", "agent": "omp", "approval": "ask",
                   "engine": {"kind": "omp"}}),
        )
        .await
        .unwrap();
    v["id"].as_str().unwrap().to_string()
}

/// The prompt the omp turn is waiting on: its pane and request id, once it is asked.
async fn asked(client: &HostClient) -> (String, String) {
    let start = std::time::Instant::now();
    loop {
        let st = client.call("agent_status_all", json!({})).await.unwrap();
        if let Some(p) = st["pending_approvals"].get(0) {
            return (
                p["pane_key"].as_str().unwrap().to_string(),
                p["req"].as_str().unwrap().to_string(),
            );
        }
        assert!(
            start.elapsed() < atlas_core::scaling::HANG,
            "omp never asked for an approval"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Run one task on a fresh omp crewmate, answer its approval `after` virtual seconds after it was asked, and
/// return the finished task and the crewmate's `approval_decision` audit entries.
async fn answered_after(secs: f64, allow: bool) -> (Value, Vec<Value>) {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_omp(tmp.path(), SCALE);
    let _lock = lock_agent_bin().await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
        Some(("omp".into(), bin.to_string_lossy().into_owned()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = omp_crewmate(&client).await;
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();

    let (pane, req) = asked(&client).await;
    tokio::time::sleep(Duration::from_secs_f64(secs / SCALE)).await;
    client
        .call(
            "agent_approve",
            json!({"pane": pane, "req": req, "allow": allow}),
        )
        .await
        .unwrap();
    let done = wait_task_state(&client, &id, &task, "done", atlas_core::scaling::HANG).await;
    let audit = client
        .call(
            "crewmate_audit",
            json!({"id": id, "kind": "approval_decision", "limit": 50}),
        )
        .await
        .unwrap();
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    client.call("shutdown", json!({})).await.ok();
    (done, audit["entries"].as_array().unwrap().clone())
}

// #524: the owner answered "allow" and the call was still blocked, "omp-approval.ts timed out after 30000ms":
// the extension waits 30 minutes, but omp abandons any `tool_call` handler after its own limit (30 s unless the
// crew's `--config` overlay raises it) and blocks the tool. 31 s is the first moment that shows it.
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_answered_after_31_seconds_still_lets_the_tool_run() {
    if !have_node() {
        return;
    }
    let (task, audit) = answered_after(31.0, true).await;
    assert_eq!(
        task["result"], "ran: echo hi",
        "the owner allowed it: {task}"
    );
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(
        (audit[0]["decision"].as_str(), audit[0]["by"].as_str()),
        (Some("allow"), Some("desktop"))
    );
}

// Minutes later is still an answer: 20 virtual minutes, well inside the 30 the prompt is held for.
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_answered_after_20_minutes_still_lets_the_tool_run() {
    if !have_node() {
        return;
    }
    let (task, _) = answered_after(20.0 * 60.0, true).await;
    assert_eq!(task["result"], "ran: echo hi", "{task}");
}

// The same wait must not turn a "deny" into anything but a block that says who decided.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_denial_blocks_the_tool_and_says_the_owner_denied_it() {
    if !have_node() {
        return;
    }
    let (task, audit) = answered_after(31.0, false).await;
    assert_eq!(
        task["result"], "blocked: the owner denied this tool call",
        "{task}"
    );
    assert_eq!(audit[0]["decision"], "deny");
}

/// A stand-in for hostd's `/hook/omp/decision` that answers every request after `delay` (real time) with
/// `status` and `body`, or never. Returns its port.
async fn stub_hostd(delay: Option<Duration>, status: &'static str, body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf).await;
                let Some(delay) = delay else {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    return;
                };
                tokio::time::sleep(delay).await;
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(reply.as_bytes()).await;
            });
        }
    });
    port
}

/// Run the fake omp with the extension generated for `port`, on a clock of `scale`, through one tool call, and
/// return its reply.
async fn reply_for_port(port: u16, scale: f64) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let ext = tmp.path().join("omp-approval.ts");
    omp::write_approval_extension_pub(&ext, port, "tok", "crew:c1:t1").unwrap();
    let overlay = omp::overlay_args(tmp.path());
    let mjs = tmp.path().join("fake_omp.mjs");
    std::fs::write(&mjs, include_str!("fake_omp.mjs")).unwrap();
    let out = tokio::task::spawn_blocking(move || {
        let mut child = Command::new("node")
            .arg(&mjs)
            .args(["--mode", "rpc", "--config", &overlay[1], "--extension"])
            .arg(&ext)
            .env("FAKE_OMP_TIME_SCALE", scale.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, r#"{{"id":"r1","type":"prompt","message":"go"}}"#).unwrap();
        let lines = std::io::BufReader::new(child.stdout.take().unwrap()).lines();
        let mut reply = String::new();
        for line in lines.map_while(Result::ok) {
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["type"] == "agent_end" {
                reply = v["messages"][0]["content"].as_str().unwrap().to_string();
                break;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        reply
    })
    .await
    .unwrap();
    out
}

// Every way an answer can fail to be an "allow" blocks the tool (fail closed) and tells the agent which it was,
// where it used to read "no approval received" for a denial, a timeout and a dead hostd alike (so the agent
// asked its owner to "release the tools", which nothing can do).
#[tokio::test(flavor = "multi_thread")]
async fn every_answer_that_is_not_an_allow_blocks_the_tool_and_says_why() {
    if !have_node() {
        return;
    }
    let quick = Some(Duration::from_millis(10));
    let cases: [(&str, u16, &str); 6] = [
        (
            "the owner denied this tool call",
            stub_hostd(quick, "200 OK", r#"{"decision":"deny"}"#).await,
            "deny",
        ),
        (
            "nobody answered the approval request",
            stub_hostd(quick, "200 OK", r#"{"decision":null}"#).await,
            "no decision (hostd's own wait ended)",
        ),
        (
            "nobody answered the approval request",
            stub_hostd(quick, "200 OK", "{}").await,
            "no decision field",
        ),
        (
            "the approval request failed: hostd answered 500",
            stub_hostd(quick, "500 Internal Server Error", "{}").await,
            "a 5xx",
        ),
        (
            "the approval request failed",
            stub_hostd(quick, "200 OK", "not json").await,
            "malformed JSON",
        ),
        (
            "the approval request failed",
            {
                // A port nobody listens on any more.
                let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                l.local_addr().unwrap().port()
            },
            "connection refused",
        ),
    ];
    for (want, port, what) in cases {
        let reply = reply_for_port(port, SCALE).await;
        assert!(
            reply.starts_with("blocked: ") && reply.contains(want),
            "{what}: the reply was {reply:?}, expected a block saying {want:?}"
        );
    }
}

// An answer of "allow" from hostd is the only thing that runs the tool.
#[tokio::test(flavor = "multi_thread")]
async fn only_an_allow_from_hostd_runs_the_tool() {
    if !have_node() {
        return;
    }
    let port = stub_hostd(
        Some(Duration::from_millis(10)),
        "200 OK",
        r#"{"decision":"allow"}"#,
    )
    .await;
    assert_eq!(reply_for_port(port, SCALE).await, "ran: echo hi");
}

// Nobody answers at all: each layer gives up after the one inside it (hostd's wait, then the extension's, then
// omp's), so the failure is the first one's clear message and not omp's "timed out after Nms". Scaled 600x,
// the 30-minute prompt is three seconds.
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_nobody_answers_fails_with_the_extensions_message_not_omps() {
    if !have_node() {
        return;
    }
    let port = stub_hostd(None, "200 OK", "{}").await;
    let reply = reply_for_port(port, 600.0).await;
    assert!(
        reply.starts_with("blocked: nobody answered the approval request"),
        "{reply:?}"
    );
    assert!(
        !reply.contains("timed out after"),
        "omp gave up first: {reply:?}"
    );
}

// The three waits are one chain, derived from hostd's own wait: a drift (someone raising hostd's wait only) would
// bring #524 back as soon as the owner takes longer than omp's old limit. What omp is told is what the overlay says.
#[test]
fn omp_gives_up_after_the_extension_which_gives_up_after_hostd() {
    let (hostd, ext, handler) = (
        CREWMATE_DECISION_WAIT,
        omp::EXTENSION_WAIT,
        omp::HANDLER_LIMIT,
    );
    assert!(
        hostd < ext && ext < handler,
        "{hostd:?} < {ext:?} < {handler:?}"
    );

    let tmp = tempfile::tempdir().unwrap();
    let overlay = std::fs::read_to_string(&omp::overlay_args(tmp.path())[1]).unwrap();
    assert!(
        overlay.contains(&format!(
            "extensionHandlers:\n  toolCallTimeoutMs: {}\n",
            handler.as_millis()
        )),
        "{overlay}"
    );
    let extension = tmp.path().join("omp-approval.ts");
    omp::write_approval_extension_pub(&extension, 1, "t", "p").unwrap();
    let extension = std::fs::read_to_string(extension).unwrap();
    assert!(
        extension.contains(&format!("AbortSignal.timeout({})", ext.as_millis())),
        "{extension}"
    );
}
