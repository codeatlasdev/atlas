//! Manager behavior against a fake language server process: sharing, refcount, idle
//! shutdown, crash restart, sync modes, cancel, applyEdit, shutdown cleanup.

// The fake server is launched through `/bin/sh -c` (it sets the fake's env per test).
#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use atlas_lsp::install::{Launch, Progress};
use atlas_lsp::manager::{Config, Manager, Push, Resolver, State};
use atlas_lsp::registry::Server;
use serde_json::{Value, json};
use tokio::sync::mpsc;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-lsp");

struct Fake {
    env: Vec<(String, String)>,
    resolved: AtomicUsize,
    delay: Duration,
    fail: bool,
}

impl Resolver for Fake {
    fn resolve<'a>(
        &'a self,
        _server: &'static Server,
        progress: &'a dyn Progress,
    ) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<Launch>> + Send + 'a>> {
        Box::pin(async move {
            self.resolved.fetch_add(1, Ordering::SeqCst);
            progress.set("Downloading fake");
            tokio::time::sleep(self.delay).await;
            if self.fail {
                anyhow::bail!("no such server");
            }
            // Environment is passed through a wrapper so each test controls the fake.
            let mut args = vec!["-c".to_string()];
            let exports: String = self
                .env
                .iter()
                .map(|(k, v)| format!("export {k}='{v}'; "))
                .collect();
            args.push(format!("{exports}exec '{FAKE}'"));
            Ok(Launch::custom("/bin/sh", args))
        })
    }
}

struct Rig {
    m: Manager,
    rx: mpsc::Receiver<Push>,
    fake: Arc<Fake>,
    _tmp: tempfile::TempDir,
    project: PathBuf,
}

fn fast() -> Config {
    Config {
        idle_shutdown: Duration::from_millis(300),
        init_timeout: Duration::from_secs(10),
        max_restarts: 3,
        restart_window: Duration::from_secs(60),
        restart_backoff: Duration::from_millis(30),
        request_timeout: Duration::from_secs(5),
    }
}

fn rig_with(cfg: Config, env: &[(&str, &str)], delay: u64, fail: bool) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("Cargo.toml"), "[package]\nname=\"p\"\n").unwrap();
    let (tx, rx) = mpsc::channel(256);
    let fake = Arc::new(Fake {
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        resolved: AtomicUsize::new(0),
        delay: Duration::from_millis(delay),
        fail,
    });
    let m = Manager::with_resolver(tmp.path(), cfg, tx, fake.clone());
    Rig {
        m,
        rx,
        fake,
        _tmp: tmp,
        project,
    }
}

fn rig() -> Rig {
    rig_with(fast(), &[], 0, false)
}

impl Rig {
    fn file(&self, name: &str) -> String {
        let p = self.project.join("src").join(name);
        std::fs::write(&p, "").unwrap();
        p.to_string_lossy().into_owned()
    }

    async fn diag(&mut self, path: &str, pred: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match self.rx.recv().await.expect("push channel closed") {
                    Push::Diagnostics {
                        path: p,
                        diagnostics,
                        ..
                    } if p == path && pred(&diagnostics) => return diagnostics,
                    _ => {}
                }
            }
        })
        .await
        .expect("diagnostics did not arrive")
    }

    async fn state(&self, path: &str) -> Value {
        self.m
            .request(path, "test/state", Value::Null, None)
            .await
            .unwrap()
    }
}

async fn within(f: impl Future<Output = ()>) {
    tokio::time::timeout(Duration::from_secs(5), f)
        .await
        .expect("timed out");
}

fn msg(d: &Value) -> String {
    d[0]["message"].as_str().unwrap_or("").to_string()
}

fn pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 only probes existence.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..400 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn two_docs_share_one_server_and_one_resolution() {
    let mut r = rig();
    let (a, b) = (r.file("a.rs"), r.file("b.rs"));
    let (ra, rb) = tokio::join!(
        r.m.open(&a, "rust", "fn a() {}".into(), 1, None),
        r.m.open(&b, "rust", "fn b() {}".into(), 1, None),
    );
    assert_eq!((ra.status, rb.status), ("ready", "ready"));
    assert_eq!(ra.server, Some("rust-analyzer"));
    assert_eq!(ra.capabilities["hoverProvider"], true);
    assert_eq!(r.m.instance_count(), 1);
    assert_eq!(r.fake.resolved.load(Ordering::SeqCst), 1);
    // both docs reached the same process
    let st = r.state(&a).await;
    assert_eq!(st["open"].as_array().unwrap().len(), 2);
    let d = r.diag(&a, |d| msg(d).starts_with("v1")).await;
    assert!(msg(&d).contains("len=9"));
}

#[tokio::test]
async fn different_roots_get_different_servers() {
    let r = rig();
    let a = r.file("a.rs");
    let other = r.project.parent().unwrap().join("other");
    std::fs::create_dir_all(other.join("src")).unwrap();
    std::fs::write(other.join("Cargo.toml"), "[package]\n").unwrap();
    let b = other.join("src/b.rs").to_string_lossy().into_owned();
    r.m.open(&a, "rust", String::new(), 1, None).await;
    r.m.open(&b, "rust", String::new(), 1, None).await;
    assert_eq!(r.m.instance_count(), 2);
    let pids = r.m.pids();
    assert_eq!(pids.len(), 2);
    assert_ne!(pids[0], pids[1]);
}

#[tokio::test]
async fn unknown_language_is_disabled_and_missing_server_reports_missing() {
    let r = rig();
    let a = r.file("a.rs");
    let rep = r.m.open(&a, "brainfuck", String::new(), 1, None).await;
    assert_eq!((rep.server, rep.status), (None, "disabled"));
    assert_eq!(r.m.instance_count(), 0);

    let r2 = rig_with(fast(), &[], 0, true);
    let a2 = r2.file("a.rs");
    let rep = r2.m.open(&a2, "rust", String::new(), 1, None).await;
    assert_eq!((rep.server, rep.status), (Some("rust-analyzer"), "missing"));
    let st = r2.m.status();
    assert_eq!(st[0].state, State::Missing);
    assert!(st[0].message.as_deref().unwrap().contains("no such server"));
}

#[tokio::test]
async fn concurrent_opens_during_slow_download_wait_on_one_resolution() {
    let r = rig_with(fast(), &[], 300, false);
    let files: Vec<_> = (0..4).map(|i| r.file(&format!("f{i}.rs"))).collect();
    let (a, b, c, d) = tokio::join!(
        r.m.open(&files[0], "rust", String::new(), 1, None),
        r.m.open(&files[1], "rust", String::new(), 1, None),
        r.m.open(&files[2], "rust", String::new(), 1, None),
        r.m.open(&files[3], "rust", String::new(), 1, None),
    );
    assert!(
        [a.status, b.status, c.status, d.status]
            .iter()
            .all(|s| *s == "ready")
    );
    assert_eq!(r.fake.resolved.load(Ordering::SeqCst), 1);
    assert_eq!(r.m.instance_count(), 1);
}

#[tokio::test]
async fn status_passes_through_downloading_then_ready() {
    let mut r = rig_with(fast(), &[], 200, false);
    let a = r.file("a.rs");
    let m = &r.m;
    let open = m.open(&a, "rust", String::new(), 1, None);
    let watch = async {
        let mut seen = vec![];
        loop {
            if let Some(Push::Status(s)) = r.rx.recv().await {
                for x in s {
                    if seen.last() != Some(&x.state) {
                        seen.push(x.state);
                    }
                    if x.state == State::Ready {
                        return seen;
                    }
                }
            }
        }
    };
    let (_, seen) = tokio::join!(open, tokio::time::timeout(Duration::from_secs(10), watch));
    let seen = seen.unwrap();
    assert_eq!(seen.first(), Some(&State::Downloading));
    assert_eq!(seen.last(), Some(&State::Ready));
}

#[tokio::test]
async fn refcount_keeps_server_until_last_doc_closes_then_idle_shutdown() {
    let r = rig();
    let (a, b) = (r.file("a.rs"), r.file("b.rs"));
    r.m.open(&a, "rust", String::new(), 1, None).await;
    r.m.open(&b, "rust", String::new(), 1, None).await;
    let pid = r.m.pids()[0];
    r.m.close(&a).await;
    tokio::time::sleep(Duration::from_millis(600)).await; // > idle_shutdown
    assert_eq!(r.m.instance_count(), 1, "still used by b.rs");
    assert!(pid_alive(pid));
    r.m.close(&b).await;
    assert_eq!(r.m.instance_count(), 1, "grace period before shutdown");
    until("idle shutdown", || r.m.instance_count() == 0).await;
    until("process gone", || !pid_alive(pid)).await;
}

#[tokio::test]
async fn reopening_inside_grace_cancels_idle_shutdown() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let pid = r.m.pids()[0];
    r.m.close(&a).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let rep = r.m.open(&a, "rust", "x".into(), 2, None).await;
    assert_eq!(rep.status, "ready");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(r.m.pids(), vec![pid], "same process kept");
    assert_eq!(r.state(&a).await["open"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn incremental_changes_are_forwarded_verbatim() {
    let mut r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "hello".into(), 1, None).await;
    let ch = json!({"range":{"start":{"line":0,"character":5},"end":{"line":0,"character":5}},"text":" 🦀"});
    r.m.change(&a, 2, vec![ch.clone()]).await.unwrap();
    let d = r.diag(&a, |d| msg(d).starts_with("v2")).await;
    assert!(msg(&d).contains("changes=1"));
    let st = r.state(&a).await;
    assert_eq!(st["change_log"][0][0], ch);
}

#[tokio::test]
async fn full_sync_servers_get_whole_text_from_the_shadow_copy() {
    let r = rig_with(fast(), &[("FAKE_SYNC", "full")], 0, false);
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "ab\ncd".into(), 1, None).await;
    let c1 = json!({"range":{"start":{"line":1,"character":1},"end":{"line":1,"character":2}},"text":"XY"});
    let c2 = json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"text":""});
    r.m.change(&a, 2, vec![c1, c2]).await.unwrap();
    let st = r.state(&a).await;
    assert_eq!(st["change_log"][0], json!([{"text": "b\ncXY"}]));
}

#[tokio::test]
async fn sync_none_servers_get_no_changes() {
    let r = rig_with(fast(), &[("FAKE_SYNC", "none")], 0, false);
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "x".into(), 1, None).await;
    r.m.change(&a, 2, vec![json!({"text": "y"})]).await.unwrap();
    assert_eq!(r.state(&a).await["changes"], 0);
}

#[tokio::test]
async fn bad_change_and_unknown_doc_are_errors_not_panics() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "x".into(), 1, None).await;
    assert!(
        r.m.change(&a, 2, vec![json!({"range": {"start": 1}, "text": "z"})])
            .await
            .is_err()
    );
    assert!(r.m.change("/nope.rs", 1, vec![]).await.is_err());
    assert!(r.m.save("/nope.rs", None).await.is_err());
    assert!(
        r.m.request("/nope.rs", "test/echo", Value::Null, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn request_cancel_by_rid_fails_the_call_and_tells_the_server() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let m = &r.m;
    let slow = m.request(&a, "test/slow", Value::Null, Some("r1".into()));
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        m.cancel("r1").await;
    };
    let (res, ()) = tokio::join!(slow, cancel);
    assert_eq!(res.unwrap_err().code, -32800);
    let mut seen = false;
    for _ in 0..40 {
        if r.state(&a).await["cancelled"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
        {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(seen, "server never received $/cancelRequest");
    // the connection still works afterwards
    let echo =
        r.m.request(&a, "test/echo", json!({"k": 1}), None)
            .await
            .unwrap();
    assert_eq!(echo["k"], 1);
}

#[tokio::test]
async fn request_timeout_cancels_and_reports() {
    let mut cfg = fast();
    cfg.request_timeout = Duration::from_millis(150);
    let r = rig_with(cfg, &[], 0, false);
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let e =
        r.m.request(&a, "test/slow", Value::Null, None)
            .await
            .unwrap_err();
    assert_eq!(e.code, -32001);
}

#[tokio::test]
async fn capabilities_pseudo_method_and_unknown_method_error() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let c =
        r.m.request(&a, "$/capabilities", Value::Null, None)
            .await
            .unwrap();
    assert_eq!(c["renameProvider"]["prepareProvider"], true);
    let e =
        r.m.request(&a, "textDocument/nothing", Value::Null, None)
            .await
            .unwrap_err();
    assert_eq!(e.code, -32601);
}

#[tokio::test]
async fn workspace_configuration_gets_server_defaults() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let v =
        r.m.request(&a, "test/config", Value::Null, None)
            .await
            .unwrap();
    assert_eq!(v[0]["checkOnSave"], true);
}

#[tokio::test]
async fn apply_edit_round_trips_through_the_client_ack() {
    let mut r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let m = &r.m;
    let req = m.request(&a, "test/applyEdit", Value::Null, None);
    let ack = async {
        loop {
            if let Some(Push::ApplyEdit { id, edit }) = r.rx.recv().await {
                assert!(edit["changes"].is_object());
                m.apply_edit_result(&id, true);
                return;
            }
        }
    };
    let (res, ()) = tokio::join!(req, within(ack));
    assert_eq!(res.unwrap()["applied"], true);

    // a declined edit is reported as not applied
    let req = m.request(&a, "test/applyEdit", Value::Null, None);
    let nack = async {
        loop {
            if let Some(Push::ApplyEdit { id, .. }) = r.rx.recv().await {
                m.apply_edit_result(&id, false);
                return;
            }
        }
    };
    let (res, ()) = tokio::join!(req, within(nack));
    assert_eq!(res.unwrap()["applied"], false);
}

#[tokio::test]
async fn crash_restarts_and_replays_open_documents() {
    let dir = tempfile::tempdir().unwrap();
    let count = dir.path().join("count");
    let mut r = rig_with(
        fast(),
        &[("FAKE_COUNT_FILE", count.to_str().unwrap())],
        0,
        false,
    );
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "fn main() {}".into(), 3, None).await;
    let pid1 = r.m.pids()[0];
    let _ = r.m.request(&a, "test/crash", Value::Null, None).await;
    // new process, same document replayed with its current text/version
    until("restart", || r.m.pids().first().is_some_and(|p| *p != pid1)).await;
    let d = r.diag(&a, |d| msg(d).starts_with("v3")).await;
    assert!(msg(&d).contains("len=12"));
    assert_eq!(std::fs::read_to_string(&count).unwrap().trim(), "2");
    assert_eq!(r.m.status()[0].state, State::Ready);
    assert!(!pid_alive(pid1));
}

#[tokio::test]
async fn edits_made_while_down_are_in_the_replayed_text() {
    let mut r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "ab".into(), 1, None).await;
    let pid1 = r.m.pids()[0];
    let _ = r.m.request(&a, "test/crash", Value::Null, None).await;
    // the editor keeps typing while the server restarts
    let ch = json!({"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":2}},"text":"cd"});
    r.m.change(&a, 2, vec![ch]).await.unwrap();
    until("restart", || r.m.pids().first().is_some_and(|p| *p != pid1)).await;
    let d = r.diag(&a, |d| msg(d).starts_with("v2")).await;
    assert!(msg(&d).contains("len=4"), "{}", msg(&d));
}

#[tokio::test]
async fn crash_loop_stops_after_budget_and_reports_crashed() {
    let mut cfg = fast();
    cfg.max_restarts = 2;
    // The server dies on every start (after counting), so initialize never completes.
    let dir = tempfile::tempdir().unwrap();
    let count = dir.path().join("count");
    let r = rig_with(
        cfg,
        &[
            ("FAKE_EXIT_ON_START", "1"),
            ("FAKE_COUNT_FILE", count.to_str().unwrap()),
        ],
        0,
        false,
    );
    let a = r.file("a.rs");
    let rep = r.m.open(&a, "rust", String::new(), 1, None).await;
    assert_eq!(rep.status, "crashed");
    // the initial failed start does not retry on its own: only established servers restart
    assert_eq!(r.m.status()[0].state, State::Crashed);
    // reopening is the retry path
    let rep = r.m.open(&a, "rust", String::new(), 1, None).await;
    assert_eq!(rep.status, "crashed");
}

#[tokio::test]
async fn restart_budget_is_limited_per_window() {
    let mut cfg = fast();
    cfg.max_restarts = 1;
    let r = rig_with(cfg, &[], 0, false);
    let a = r.file("a.rs");
    r.m.open(&a, "rust", String::new(), 1, None).await;
    let pid1 = r.m.pids()[0];
    let _ = r.m.request(&a, "test/crash", Value::Null, None).await;
    until("first restart", || {
        r.m.pids().first().is_some_and(|p| *p != pid1)
    })
    .await;
    let pid2 = r.m.pids()[0];
    let _ = r.m.request(&a, "test/crash", Value::Null, None).await;
    until("gives up", || r.m.status()[0].state == State::Crashed).await;
    assert!(r.m.pids().is_empty());
    assert!(!pid_alive(pid2));
    assert!(
        r.m.status()[0]
            .message
            .as_deref()
            .unwrap()
            .contains("crashed repeatedly")
    );
}

#[tokio::test]
async fn close_clears_diagnostics_and_shutdown_kills_every_process() {
    let mut r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "x".into(), 1, None).await;
    r.diag(&a, |d| !d.as_array().unwrap().is_empty()).await;
    r.m.close(&a).await;
    r.diag(&a, |d| d.as_array().unwrap().is_empty()).await;

    let b = r.file("b.rs");
    r.m.open(&b, "rust", "x".into(), 1, None).await;
    let pids = r.m.pids();
    assert!(!pids.is_empty() && pids.iter().all(|p| pid_alive(*p)));
    r.m.shutdown().await;
    for p in pids {
        until("process killed", || !pid_alive(p)).await;
    }
    assert_eq!(r.m.instance_count(), 0);
    assert_eq!(r.m.open_doc_count(), 0);
}

#[tokio::test]
async fn reopen_after_reconnect_replaces_the_attachment() {
    let r = rig();
    let a = r.file("a.rs");
    r.m.open(&a, "rust", "old".into(), 1, None).await;
    // a reconnecting client re-sends lsp_open with its current text
    let rep = r.m.open(&a, "rust", "newer text".into(), 7, None).await;
    assert_eq!(rep.status, "ready");
    assert_eq!(r.m.open_doc_count(), 1);
    assert_eq!(r.m.instance_count(), 1);
    let st = r.state(&a).await;
    assert_eq!(
        st["texts"].as_object().unwrap().values().next().unwrap(),
        "newer text"
    );
}
