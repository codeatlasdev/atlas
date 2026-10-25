//! Real daemon (`serve_with` on a temp dir) and a real iroh client on localhost, relays off.

use std::path::Path;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::HostClient;

async fn daemon(dir: &Path) -> HostClient {
    let d = dir.to_path_buf();
    let opts = crate::server::ServeOpts {
        no_mobile_relay: true,
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    for _ in 0..100 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

/// `mobile_pair_start` once the endpoint has bound (it binds in the background).
async fn pair_start(c: &HostClient) -> Value {
    for _ in 0..100 {
        let v = c.call("mobile_pair_start", json!({})).await.unwrap();
        if v["addrs"].as_array().is_some_and(|a| !a.is_empty()) {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("mobile endpoint did not bind");
}

struct Phone {
    _ep: Endpoint,
    conn: Connection,
    send: SendStream,
    recv: BufReader<RecvStream>,
    next: u64,
}

/// The published ports on loopback. These tests cover pairing, scopes and prompts, not reachability: dialing the
/// published LAN/tunnel addresses made five of them time out for 30 s on a Mac with Cloudflare WARP (codeatlasdev/atlas#118,
/// reachability itself: #113), while the same endpoint answers at once on loopback.
fn loopback_addrs(info: &Value) -> Vec<std::net::SocketAddr> {
    let mut ports: Vec<(bool, u16)> = info["addrs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().parse::<std::net::SocketAddr>().unwrap())
        .map(|a| (a.is_ipv6(), a.port()))
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
        .into_iter()
        .map(|(v6, port)| {
            let ip: std::net::IpAddr = if v6 {
                std::net::Ipv6Addr::LOCALHOST.into()
            } else {
                std::net::Ipv4Addr::LOCALHOST.into()
            };
            (ip, port).into()
        })
        .collect()
}

#[test]
fn loopback_addrs_keeps_only_the_published_ports_on_loopback() {
    let info = json!({"addrs": ["192.168.68.50:4433", "100.96.0.1:4433", "[2606:4700::1]:4434", "[fe80::1]:4434"]});
    let got: Vec<String> = loopback_addrs(&info)
        .iter()
        .map(|a| a.to_string())
        .collect();
    assert_eq!(got, ["127.0.0.1:4433", "[::1]:4434"]);
}

impl Phone {
    async fn dial(info: &Value) -> Self {
        Self::dial_as(info, iroh::SecretKey::generate()).await
    }

    /// Dial as a given identity (the same phone reconnecting).
    async fn dial_as(info: &Value, key: iroh::SecretKey) -> Self {
        let ep = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(key)
            .bind()
            .await
            .unwrap();
        let id: PublicKey = info["node_id"].as_str().unwrap().parse().unwrap();
        let mut addr = EndpointAddr::new(id);
        for a in loopback_addrs(info) {
            addr = addr.with_ip_addr(a);
        }
        let conn = ep.connect(addr, super::ALPN).await.unwrap();
        let (send, recv) = conn.open_bi().await.unwrap();
        Self {
            _ep: ep,
            conn,
            send,
            recv: BufReader::new(recv),
            next: 1,
        }
    }

    /// Send one request and return its reply (skipping pushes). `None` = stream closed.
    async fn call(&mut self, cmd: &str, args: Value) -> Option<Value> {
        let id = self.next;
        self.next += 1;
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string() + "\n";
        self.send.write_all(line.as_bytes()).await.ok()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut buf = String::new();
            loop {
                buf.clear();
                if self.recv.read_line(&mut buf).await.ok()? == 0 {
                    return None;
                }
                let v: Value = serde_json::from_str(&buf).ok()?;
                if v["id"] == id {
                    return Some(v);
                }
            }
        })
        .await
        .ok()
        .flatten()
    }
}

#[tokio::test]
async fn pairing_scopes_and_revocation() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let code = info["code"].as_str().unwrap().to_string();

    // Unpaired: any command but `pair` is refused and the connection ends.
    let mut stranger = Phone::dial(&info).await;
    let r = stranger.call("pty_list", json!({})).await.unwrap();
    assert_eq!(r["error"], "not paired");
    assert!(stranger.call("pty_list", json!({})).await.is_none());

    // Wrong code refused.
    let mut bad = Phone::dial(&info).await;
    let r = bad
        .call("pair", json!({"code": "x", "name": "n"}))
        .await
        .unwrap();
    assert_eq!(r["error"], "bad code");

    let mut phone = Phone::dial(&info).await;
    let r = phone
        .call("pair", json!({"code": code, "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    // The code is single-use.
    let mut second = Phone::dial(&info).await;
    let r = second
        .call("pair", json!({"code": code, "name": "x"}))
        .await
        .unwrap();
    assert_eq!(r["error"], "no pairing in progress");

    assert_eq!(phone.call("pty_list", json!({})).await.unwrap()["ok"], true);
    // The phone lost its local list and scans a new QR: an already paired phone is
    // answered OK, with no code check, instead of a "bad request".
    let mut again = Phone::dial_as(&info, phone._ep.secret_key().clone()).await;
    let r = again
        .call("pair", json!({"code": "whatever", "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    // Never from a phone, whatever its scopes: shell exec, raw attach, managing devices.
    for cmd in [
        "exec",
        "pty_attach_raw",
        "mobile_set_scopes",
        "mobile_revoke",
        "shutdown",
    ] {
        let r = phone.call(cmd, json!({})).await.unwrap();
        assert_ne!(r["ok"], true, "{cmd} must be refused: {r}");
    }
    let r = phone
        .call(
            "mobile_set_scopes",
            json!({"node_id": phone.conn.remote_id().to_string(), "scopes": ["read"]}),
        )
        .await
        .unwrap();
    assert_eq!(r["error"], "not available on mobile");

    // Scope removed on the desktop → refused at once on the open connection.
    let node = c.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();
    let r = phone
        .call("project_service", json!({"id": "x", "op": "restart"}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act");
    let r = phone
        .call("pty_write", json!({"pty": "x", "data": ""}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: term_write");

    // Revoked → disconnected, and a reconnect is treated as unpaired.
    c.call("mobile_revoke", json!({"node_id": node}))
        .await
        .unwrap();
    assert!(phone.call("pty_list", json!({})).await.is_none());
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    assert!(audit.contains("\"cmd\":\"pair\""), "{audit}");
    c.call("shutdown", json!({})).await.ok();
}

impl Phone {
    /// Pushes (lines without an `id`) that arrive within `ms`.
    async fn pushes(&mut self, ms: u64) -> Vec<Value> {
        let mut out = Vec::new();
        let mut buf = String::new();
        let end = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(n)) = tokio::time::timeout_at(end, self.recv.read_line(&mut buf)).await {
            if n == 0 {
                break;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&buf)
                && v.get("id").is_none()
            {
                out.push(v);
            }
            buf.clear();
        }
        out
    }
}

/// A paired phone and the desktop-side client of the same daemon.
async fn paired(dir: &Path) -> (HostClient, Phone, String) {
    let c = daemon(dir).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    let r = phone
        .call("pair", json!({"code": info["code"], "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let node = c.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    (c, phone, node)
}

// Guards scopes on pushes: they were checked on requests only, so every phone got
// every broadcast (approval prompts with their command, editor diagnostics), and
// taking `term_read` away left an attached terminal streaming.
// Multi-thread: `hook` blocks on std TCP while the daemon's hook server serves it.
#[tokio::test(flavor = "multi_thread")]
async fn pushes_follow_the_phone_scopes() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, node) = paired(tmp.path()).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": tmp.path(), "argv": ["sh", "-c", "while :; do echo tick; sleep 0.1; done"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    let r = phone
        .call(
            "pty_attach",
            json!({"pty": pty, "replay": false, "output": true}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let evs = |v: &[Value]| {
        v.iter()
            .map(|e| e["ev"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>()
    };
    assert!(evs(&phone.pushes(600).await).contains(&"pty_output".to_string()));

    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();
    // Output sent while the phone still had `terminal` can be in flight when the call returns
    // (PR #106 gate: "terminal kept streaming"). Drop it; the pty ticks every 100 ms, so a
    // stream that really kept going still shows up in the window below.
    phone.pushes(300).await;
    hook(tmp.path(), json!({"event": "agent_start"}));
    let got = evs(&phone.pushes(800).await);
    assert!(
        !got.contains(&"pty_output".to_string()),
        "terminal kept streaming: {got:?}"
    );
    assert!(
        got.contains(&"agent_status".to_string()),
        "read still sees agents: {got:?}"
    );
    assert!(
        got.iter()
            .all(|e| !e.starts_with("lsp_") && e != "fs_event"),
        "{got:?}"
    );
    c.call("pty_kill", json!({"pty": pty})).await.ok();
    c.call("shutdown", json!({})).await.ok();
}

// Guards a phone paired against what it was told: the device was kept in memory when
// writing devices.json failed, so a phone shown "failed" was trusted until restart.
#[test]
fn a_pairing_that_cannot_be_saved_does_not_pair() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let m = super::Mobile::load(tmp.path(), false).unwrap();
    let code = m.pair_start().unwrap()["code"]
        .as_str()
        .unwrap()
        .to_string();
    let d = tmp.path().join("mobile");
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o500)).unwrap();
    let r = m.pair("n1", &code, "iPhone");
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(r.is_err(), "{r:?}");
    assert!(m.device("n1").is_none(), "trusted without being saved");
}

// Guards memory on the open gateway: any peer that knows the node id can dial
// before pairing, and a line was read whole before its length was checked, so one
// endless line grew hostd's buffer without bound. It is cut at MAX_LINE_BYTES.
#[tokio::test]
async fn an_unpaired_peer_sending_an_endless_line_is_cut_off() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut p = Phone::dial(&info).await;
    let chunk = vec![b'x'; 256 * 1024];
    let mut sent = 0;
    let cut = tokio::time::timeout(Duration::from_secs(20), async {
        // No newline, ever: hostd must close the stream past the limit.
        while p.send.write_all(&chunk).await.is_ok() {
            sent += chunk.len();
            if sent > 64 * crate::protocol::MAX_LINE_BYTES {
                return false;
            }
        }
        true
    })
    .await
    .unwrap_or(false);
    assert!(cut, "hostd kept reading an endless line ({sent} bytes)");
    assert!(
        sent <= 4 * crate::protocol::MAX_LINE_BYTES,
        "cut late: {sent} bytes"
    );
    c.call("shutdown", json!({})).await.ok();
}

#[tokio::test]
async fn five_wrong_codes_invalidate_the_code() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    for _ in 0..5 {
        let mut p = Phone::dial(&info).await;
        let r = p
            .call("pair", json!({"code": "000000x", "name": "n"}))
            .await
            .unwrap();
        assert_eq!(r["error"], "bad code");
    }
    let mut p = Phone::dial(&info).await;
    let r = p
        .call("pair", json!({"code": info["code"], "name": "n"}))
        .await
        .unwrap();
    assert_eq!(r["error"], "no pairing in progress");
    c.call("shutdown", json!({})).await.ok();
}

/// POST to the hook server the way `atlas hook` does; the reply body.
fn post_hook(dir: &Path, path: &str, body: Value) -> String {
    use std::io::{Read, Write};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = body.to_string();
    let mut s =
        std::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT"))).unwrap();
    write!(
        s,
        "POST {path} HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    )
    .unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    r.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

/// One lifecycle event for pane `p1`.
fn hook(dir: &Path, payload: Value) {
    post_hook(
        dir,
        "/hook/omp",
        json!({"pane_key": "p1", "payload": payload}),
    );
}

/// A permission prompt for pane `p1`, as `atlas hook` asks it.
fn ask_decision(dir: &Path) -> String {
    post_hook(
        dir,
        "/hook/claude/decision",
        json!({"pane_key": "p1", "payload": {"tool_name": "Bash"}}),
    )
}

// Guards an agent stuck 55 s on a phone that cannot answer: any connected phone made
// permission prompts wait, even one without `approve` (its answer is refused). And the
// answer of one that may approve reaches the hook in the body `atlas hook` parses.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_phone_that_may_approve_holds_a_prompt() {
    use crate::agents::hook_server::parse_decision;
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, node) = paired(tmp.path()).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();
    let dir = tmp.path().to_path_buf();
    let t = std::time::Instant::now();
    let body = tokio::task::spawn_blocking(move || ask_decision(&dir))
        .await
        .unwrap();
    assert_eq!(parse_decision(&body), None, "{body}");
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "waited {:?}",
        t.elapsed()
    );

    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read", "approve"]}),
    )
    .await
    .unwrap();
    let dir = tmp.path().to_path_buf();
    let asked = tokio::task::spawn_blocking(move || ask_decision(&dir));
    let req = loop {
        let p = phone.pushes(500).await;
        if let Some(e) = p.iter().find(|e| e["ev"] == "agent_approval") {
            break e["req"].as_str().unwrap().to_string();
        }
    };
    let r = phone
        .call(
            "agent_approve",
            json!({"pane": "p1", "req": req, "allow": true}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(parse_decision(&asked.await.unwrap()), Some(true));
    c.call("shutdown", json!({})).await.ok();
}

/// A prompt typed while the agent runs a turn would land in the middle of it.
// Multi-thread: `hook` blocks on std TCP while the daemon's hook server serves it.
#[tokio::test(flavor = "multi_thread")]
async fn agent_send_types_only_into_an_idle_or_done_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let send = |text: &str| c.call("agent_send", json!({"pane": "p1", "text": text}));
    assert_eq!(
        send("hi").await.unwrap_err().to_string(),
        "unknown agent pane"
    );
    hook(tmp.path(), json!({"event": "agent_start"}));
    assert_eq!(
        send("hi").await.unwrap_err().to_string(),
        "agent is working"
    );
    hook(tmp.path(), json!({"event": "agent_end"}));
    assert_eq!(
        send("hi").await.unwrap_err().to_string(),
        "no terminal for pane"
    );
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": tmp.path(), "argv": ["cat"], "scope": {"pane_key": "p1"}}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    send("hello-agent").await.unwrap();
    let mut text = String::new();
    for _ in 0..50 {
        text = c
            .call("pty_read", json!({"pty": pty, "lines": 10}))
            .await
            .unwrap()["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if text.matches("hello-agent").count() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Echoed by the tty, then printed by `cat` once Enter arrived as its own write.
    assert!(text.matches("hello-agent").count() >= 2, "{text:?}");
    c.call("shutdown", json!({})).await.ok();
}

/// Contract with Settings → Mobile: `apps/desktop/src/lib/__fixtures__/mobile-*.json` are
/// this producer's output (random parts pinned), read by `settings/mobile.test.ts`, whose
/// pairing URL the phone's `pair_url_test.dart` parses. `UPDATE_FIXTURES=1` rewrites them.
#[tokio::test]
async fn settings_fixtures_are_the_real_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let m = std::sync::Arc::new(super::Mobile::load(tmp.path(), false).unwrap());
    m.ensure_endpoint().await.unwrap();
    let mut start = m.pair_start().unwrap();
    let code = start["code"].as_str().unwrap().to_string();
    m.pair("n1", &code, "iPhone").unwrap();
    m.set_scopes("n1", vec![super::Scope::Read, super::Scope::TermRead])
        .unwrap();
    start["node_id"] = json!("NODE");
    start["code"] = json!("012345");
    start["hostname"] = json!("Luan's MacBook");
    start["addrs"] = json!(["192.168.1.2:4433", "[fe80::1]:4433"]);
    let mut devices = json!(m.devices());
    devices[0]["paired_at"] = json!(1_700_000_000);
    for (name, v) in [
        ("mobile-pair-start.json", start),
        ("mobile-devices.json", devices),
    ] {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__")
            .join(name);
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
        }
        let want: Value = serde_json::from_str(
            &std::fs::read_to_string(&p)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        assert_eq!(
            want, v,
            "{name} changed: rerun with UPDATE_FIXTURES=1, then the TS test"
        );
    }
}
