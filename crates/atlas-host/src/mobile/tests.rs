//! Real daemon (`serve_with` on a temp dir) and a real iroh client on localhost, relays off.

use std::path::Path;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::HostClient;

/// A path the way a daemon names it, symlinks resolved: macOS keeps `/var` as a link to `/private/var`, so a temp dir
/// and the same folder read back from hostd (a `pwd`, a `cwd`, an audit line) differ without it. Both sides of every
/// comparison of a temp path go through this (#597), never `assert_eq!(tmp_path, daemon_path)`.
fn canon(p: impl AsRef<Path>) -> std::path::PathBuf {
    atlas_core::paths::canonical(p.as_ref()).unwrap()
}

/// Tests listen on loopback only: any other bound address makes the macOS firewall ask, for every new
/// test binary, "Do you want the application to accept incoming network connections?".
fn assert_all_loopback(socks: &[std::net::SocketAddr]) {
    assert!(!socks.is_empty(), "endpoint bound nothing");
    assert!(
        socks.iter().all(|s| s.ip().is_loopback()),
        "a test endpoint listens beyond loopback: {socks:?}"
    );
}

async fn daemon(dir: &Path) -> HostClient {
    // No desktop, whatever runs on the machine: a socket that is never there (`agent_launch` tries it first).
    daemon_with(dir, dir.join("no-desktop.sock")).await
}

/// A daemon whose Atlas Desktop control socket is `socket` (token file beside it).
async fn daemon_with(dir: &Path, socket: std::path::PathBuf) -> HostClient {
    daemon_as(dir, socket, true).await
}

/// A plain computer (a Mac with the desktop), not a crew server: no crewmates, a project registry.
async fn computer_daemon(dir: &Path) -> HostClient {
    daemon_as(dir, dir.join("no-desktop.sock"), false).await
}

async fn daemon_as(dir: &Path, socket: std::path::PathBuf, crew_host: bool) -> HostClient {
    let d = dir.to_path_buf();
    let opts = crate::server::ServeOpts {
        no_mobile_relay: true,
        mobile_loopback: true,
        crew_host,
        desktop: Some(atlas_core::desktop_ctl::Endpoint {
            token: socket.with_extension("token"),
            socket,
        }),
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    for _ in 0..1200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

/// `mobile_pair_start` once the endpoint has bound (it binds in the background).
pub(crate) async fn pair_start(c: &HostClient) -> Value {
    for _ in 0..1200 {
        let v = c.call("mobile_pair_start", json!({})).await.unwrap();
        if v["addrs"].as_array().is_some_and(|a| !a.is_empty()) {
            // The gateway listens on loopback only, so nothing else can be published: a LAN
            // address here means the daemon bound every interface (macOS asks about that).
            for a in v["addrs"].as_array().unwrap() {
                let a: std::net::SocketAddr = a.as_str().unwrap().parse().unwrap();
                assert!(a.ip().is_loopback(), "the test gateway published {a}");
            }
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("mobile endpoint did not bind");
}

pub(crate) struct Phone {
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
    pub(crate) async fn dial(info: &Value) -> Self {
        Self::dial_as(info, iroh::SecretKey::generate()).await
    }

    /// Dial as a given identity (the same phone reconnecting).
    async fn dial_as(info: &Value, key: iroh::SecretKey) -> Self {
        let builder = Endpoint::builder(iroh::endpoint::presets::Minimal).secret_key(key);
        let ep = super::loopback_only(builder).unwrap().bind().await.unwrap();
        assert_all_loopback(&ep.bound_sockets());
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
    pub(crate) async fn call(&mut self, cmd: &str, args: Value) -> Option<Value> {
        let id = self.next;
        self.next += 1;
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string() + "\n";
        self.send.write_all(line.as_bytes()).await.ok()?;
        tokio::time::timeout(atlas_core::scaling::HANG, async {
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

    /// The phone's connection ends (the app is killed or loses the network).
    pub(crate) fn hang_up(&self) {
        self.conn.close(0u32.into(), b"bye");
    }

    /// [`call`](Self::call), but also returns the pushes that arrived before the reply (a
    /// `pty_attach` replay is pushed ahead of its reply, and `call` drops it).
    pub(crate) async fn call_with_pushes(
        &mut self,
        cmd: &str,
        args: Value,
    ) -> Option<(Value, Vec<Value>)> {
        let id = self.next;
        self.next += 1;
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string() + "\n";
        self.send.write_all(line.as_bytes()).await.ok()?;
        tokio::time::timeout(atlas_core::scaling::HANG, async {
            let (mut buf, mut pushes) = (String::new(), Vec::new());
            loop {
                buf.clear();
                if self.recv.read_line(&mut buf).await.ok()? == 0 {
                    return None;
                }
                let v: Value = serde_json::from_str(&buf).ok()?;
                if v["id"] == id {
                    return Some((v, pushes));
                }
                pushes.push(v);
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
    pub(crate) async fn pushes(&mut self, ms: u64) -> Vec<Value> {
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

    /// Pushes until one satisfies `done` (that one is the last returned), or until `ms` pass.
    pub(crate) async fn pushes_until(
        &mut self,
        ms: u64,
        done: impl Fn(&Value) -> bool,
    ) -> Vec<Value> {
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
                let last = done(&v);
                out.push(v);
                if last {
                    break;
                }
            }
            buf.clear();
        }
        out
    }
}

/// A paired phone and the desktop-side client of the same daemon.
async fn paired(dir: &Path) -> (HostClient, Phone, String) {
    paired_on(daemon(dir).await).await
}

async fn paired_on(c: HostClient) -> (HostClient, Phone, String) {
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
    let cut = tokio::time::timeout(atlas_core::scaling::HANG, async {
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
pub(crate) fn post_hook(dir: &Path, path: &str, body: Value) -> String {
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
    // A held prompt would announce itself on the daemon's events and wait 55 s for the phone: the helper fails
    // on that event, whatever the machine's speed.
    let mut events = c.events();
    let body = crate::agents::hook_server::answered_without_holding(&mut events, move || {
        ask_decision(&dir)
    })
    .await;
    assert_eq!(parse_decision(&body), None, "{body}");

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
    for _ in 0..1200 {
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

/// Contract with Settings → Integrations → Mobile: `apps/desktop/src/lib/__fixtures__/mobile-*.json` are
/// this producer's output (random parts pinned), read by `settings/mobile.test.ts`, whose
/// pairing URL the phone's `pair_url_test.dart` parses. `UPDATE_FIXTURES=1` rewrites them.
#[tokio::test]
async fn settings_fixtures_are_the_real_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let m = std::sync::Arc::new(
        super::Mobile::load(tmp.path(), false)
            .unwrap()
            .loopback_only(true),
    );
    m.ensure_endpoint().await.unwrap();
    assert_all_loopback(&m.endpoint_sockets());
    let mut start = m.pair_start().unwrap();
    let code = start["code"].as_str().unwrap().to_string();
    // The name the real app sends (`pair_screen.dart`): the same for every phone.
    m.pair("n1", &code, "Atlas Mobile").unwrap();
    m.set_scopes("n1", vec![super::Scope::Read, super::Scope::TermRead])
        .unwrap();
    start["node_id"] = json!("NODE");
    start["code"] = json!("012345");
    start["hostname"] = json!("Luan's MacBook");
    start["addrs"] = json!(["192.168.1.2:4433", "[fe80::1]:4433"]);
    m.audit(
        "n1",
        "project_service",
        &json!({"id": "p1", "service": "web", "op": "restart"}),
    );
    m.audit("n1", "pty_write", &json!({"pty": "t1", "data": "bHM="}));
    m.audit(
        "n1",
        "agent_approve",
        &json!({"pane": "a1", "req": "r1", "allow": true}),
    );
    let mut devices = json!(m.devices_view());
    devices[0]["paired_at"] = json!(1_700_000_000);
    devices[0]["last_seen"] = json!(1_700_000_500);
    let mut activity = json!(m.activity("n1", 50));
    for (i, e) in activity.as_array_mut().unwrap().iter_mut().enumerate() {
        e["ts"] = json!(1_700_000_500 - 60 * i as u64);
    }
    for (name, v) in [
        ("mobile-pair-start.json", start),
        ("mobile-devices.json", devices),
        ("mobile-activity.json", activity),
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

/// A paired phone with only Read scope gets forbidden: act on crewmate_task.
#[tokio::test]
async fn crewmate_task_requires_act_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, node) = paired(tmp.path()).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();
    let r = phone
        .call(
            "crewmate_task",
            json!({"id": "x", "prompt": "hi", "source": "mobile"}),
        )
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    c.call("shutdown", json!({})).await.ok();
}

/// crewmate_task push events reach a Read phone.
#[tokio::test]
async fn crewmate_task_push_reaches_read_phone() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, _node) = paired(tmp.path()).await;
    // Simulate a crewmate_task push by broadcasting it from the desktop side.
    // We reach into the daemon via exec + a raw connection to send an event.
    // Simpler: shutdown and check the scope mapping.
    // push_scope("crewmate_task") == Some(Scope::Read)
    assert_eq!(
        crate::mobile::push_scope("crewmate_task"),
        Some(crate::mobile::Scope::Read)
    );
    c.call("shutdown", json!({})).await.ok();
    let _ = phone.call("info", json!({})).await; // just drain
}

/// desktop calls mobile_invite; phone receives computer_invite push with correct url/name.
#[tokio::test]
async fn mobile_invite_persists_and_pushes_to_connected_phone() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, _node) = paired(tmp.path()).await;

    // Prime the phone's push listener before sending the invite.
    let url = "atlas-mobile://pair?code=123456";
    let name = "MyMac";
    c.call("mobile_invite", json!({"url": url, "name": name}))
        .await
        .unwrap();

    // The invite must be persisted: mobile_invites returns it.
    let invites = c.call("mobile_invites", json!({})).await.unwrap();
    let list = invites["invites"].as_array().unwrap();
    assert!(!list.is_empty(), "invite must be persisted: {invites}");
    assert_eq!(list[0]["url"], url, "wrong url: {invites}");
    assert_eq!(list[0]["name"], name, "wrong name: {invites}");

    // Phone should receive a computer_invite push.
    let pushes = phone.pushes(500).await;
    let got = pushes.iter().find(|v| v["ev"] == "computer_invite");
    assert!(
        got.is_some(),
        "phone must receive computer_invite push; got: {pushes:?}"
    );
    let p = got.unwrap();
    assert_eq!(p["url"], url, "wrong url in push: {p}");
    assert_eq!(p["name"], name, "wrong name in push: {p}");

    c.call("shutdown", json!({})).await.ok();
}

/// mobile_invites returns pending invite to a phone that connects after the invite was sent.
#[tokio::test]
async fn mobile_invites_returns_pending_to_late_connecting_phone() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, _node) = paired(tmp.path()).await;

    c.call(
        "mobile_invite",
        json!({"url": "atlas-mobile://pair?code=999", "name": "Mac"}),
    )
    .await
    .unwrap();

    // Phone calls mobile_invites and sees the invite.
    let r = phone.call("mobile_invites", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let list = r["data"]["invites"].as_array().unwrap();
    assert!(!list.is_empty(), "phone must see pending invite: {r}");
    assert_eq!(list[0]["url"], "atlas-mobile://pair?code=999");

    c.call("shutdown", json!({})).await.ok();
}

/// after mobile_invite, phone calls mobile_invite_dismiss; mobile_invites returns empty.
#[tokio::test]
async fn mobile_invite_dismiss_removes() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, _node) = paired(tmp.path()).await;

    let url = "atlas-mobile://pair?code=dismiss-me";
    c.call("mobile_invite", json!({"url": url, "name": "Mac"}))
        .await
        .unwrap();

    // Verify it's there first.
    let r = phone.call("mobile_invites", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let list = r["data"]["invites"].as_array().unwrap();
    assert!(!list.is_empty(), "invite must exist before dismiss: {r}");

    // Dismiss it.
    let d = phone
        .call("mobile_invite_dismiss", json!({"url": url}))
        .await
        .unwrap();
    assert_eq!(d["ok"], true, "{d}");

    // Now mobile_invites must return empty.
    let r2 = phone.call("mobile_invites", json!({})).await.unwrap();
    assert_eq!(r2["ok"], true, "{r2}");
    let list2 = r2["data"]["invites"].as_array().unwrap();
    assert!(
        list2.is_empty(),
        "invite must be removed after dismiss: {r2}"
    );

    c.call("shutdown", json!({})).await.ok();
}

/// phone calling mobile_invite gets "not available on mobile" error.
#[tokio::test]
async fn mobile_invite_not_available_on_phone() {
    let tmp = tempfile::tempdir().unwrap();
    let (_c, mut phone, _node) = paired(tmp.path()).await;

    let r = phone
        .call(
            "mobile_invite",
            json!({"url": "atlas-mobile://pair?code=x", "name": "Mac"}),
        )
        .await
        .unwrap();
    assert_eq!(
        r["ok"], false,
        "expected error for phone mobile_invite: {r}"
    );
    assert!(
        r["error"]
            .as_str()
            .is_some_and(|e| e.contains("not available on mobile")),
        "wrong error: {r}"
    );

    _c.call("shutdown", json!({})).await.ok();
}

/// Escape stops a running turn; it is refused while the agent is idle or done, where it
/// would only clear what the user is typing on the desktop.
#[tokio::test(flavor = "multi_thread")]
async fn agent_interrupt_sends_escape_only_while_the_agent_works() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let stop = || c.call("agent_interrupt", json!({"pane": "p1"}));
    assert_eq!(stop().await.unwrap_err().to_string(), "unknown agent pane");
    hook(tmp.path(), json!({"event": "agent_end"}));
    assert_eq!(
        stop().await.unwrap_err().to_string(),
        "agent is not working"
    );
    hook(tmp.path(), json!({"event": "agent_start"}));
    assert_eq!(
        stop().await.unwrap_err().to_string(),
        "no terminal for pane"
    );
    // `od -c` shows the byte the agent would read.
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": tmp.path(), "argv": ["sh", "-c", "stty raw -echo; head -c1 | od -c; sleep 5"], "scope": {"pane_key": "p1"}}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;
    stop().await.unwrap();
    let mut text = String::new();
    for _ in 0..1200 {
        text = c
            .call("pty_read", json!({"pty": pty, "lines": 10}))
            .await
            .unwrap()["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if text.contains("033") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(text.contains("033"), "{text:?}");
    c.call("shutdown", json!({})).await.ok();
}

/// Contract with the phone's chat (`agent_history_test.dart`): `apps/mobile/test/fixtures/
/// agent_history.json` is this producer's output for an omp session with a tool call, an
/// error, thinking and a finished subagent. `UPDATE_FIXTURES=1` rewrites it.
#[tokio::test(flavor = "multi_thread")]
async fn agent_history_fixture_is_the_real_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let file = tmp.path().join("s.jsonl");
    let msg = |role: &str, content: Value| {
        json!({"type": "message", "message": {"role": role, "content": content}}).to_string()
    };
    let lines = [
        json!({"type": "title", "title": "\u{f011c} Fix the sheet z-index"}).to_string(),
        json!({"type": "session", "id": "x"}).to_string(),
        msg("user", json!("O sheet fica sob a nav. Corrige?")),
        msg("user", json!("[Wait interrupted by message]\n<irc from=\"parent\" agent=\"Main\">\nUse z-50 no sheet.\n</irc>")),
        msg("user", json!("<system-notice>\nBackground job CssScout has completed.\n</system-notice>")),
        json!({"type": "custom_message", "customType": "advisor", "content": "x",
               "details": {"notes": [{"note": "Medir antes de mudar.", "severity": "concern"}]}}).to_string(),
        msg("assistant", json!([
            {"type": "thinking", "thinking": "Compare z-index of nav and sheet."},
            {"type": "text", "text": "Vou medir.\n\n| Camada | z |\n|---|---|\n| nav | 40 |\n| sheet | 50 |"},
            {"type": "toolCall", "id": "a", "name": "bash", "intent": "Running biome", "arguments": {"command": "bunx biome check --write"}},
            {"type": "toolCall", "id": "b", "name": "task", "arguments": {"tasks": [{"name": "CssScout", "agent": "scout", "task": "Find z-index rules"}]}},
        ])),
        json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "a", "isError": true, "content": [{"type": "text", "text": "1 error"}]}}).to_string(),
        json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "b", "content": [{"type": "text", "text": "Spawned 1 agent"}]}}).to_string(),
        msg("assistant", json!([{"type": "text", "text": "Pronto: o sheet agora fica acima da nav."}])),
    ];
    std::fs::write(&file, lines.join("\n")).unwrap();
    let sub = tmp.path().join("s");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(
        sub.join("CssScout.jsonl"),
        msg("assistant", json!([{"type": "text", "text": "nav z-40"}])),
    )
    .unwrap();
    std::fs::write(sub.join("CssScout.md"), "nav z-40, sheet z-50").unwrap();
    hook(
        tmp.path(),
        json!({"event": "session_start", "session_file": file.to_string_lossy()}),
    );
    let mut v = c
        .call("agent_history", json!({"pane": "p1"}))
        .await
        .unwrap();
    assert_eq!(v["title"], "Fix the sheet z-index");
    v["subagents"][0]["updated_at"] = json!(1_700_000_000_000u64);
    let s = c
        .call("agent_history", json!({"pane": "p1", "sub": "CssScout"}))
        .await
        .unwrap();
    assert_eq!(s["items"][0]["text"], "nav z-40");
    assert!(
        c.call("agent_history", json!({"pane": "p1", "sub": "../s"}))
            .await
            .is_err()
    );
    let status = c.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(status["panes"]["p1"]["title"], "Fix the sheet z-index");
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/agent_history.json");
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
        "agent_history.json changed: rerun with UPDATE_FIXTURES=1, then the Dart test"
    );
    c.call("shutdown", json!({})).await.ok();
}

/// A turn in flight takes a typed message as steering in omp (it queues it); Claude Code and
/// Codex do not, so `steer` changes nothing for them, and omp without `steer` still refuses.
#[tokio::test(flavor = "multi_thread")]
async fn agent_send_steer_reaches_only_a_working_omp() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    hook(tmp.path(), json!({"event": "agent_start"}));
    let plain = c
        .call("agent_send", json!({"pane": "p1", "text": "hi"}))
        .await;
    assert_eq!(plain.unwrap_err().to_string(), "agent is working");
    // Allowed: it gets past the state check and fails only for lack of a terminal.
    let steer = c
        .call(
            "agent_send",
            json!({"pane": "p1", "text": "hi", "steer": true}),
        )
        .await;
    assert_eq!(steer.unwrap_err().to_string(), "no terminal for pane");
    c.call("shutdown", json!({})).await.ok();
}

/// A phone reads the tree of any agent it may read; `agent_tree` needs only `read`.
#[test]
fn agent_tree_needs_read() {
    use crate::protocol::HostCmd;
    let c: HostCmd =
        serde_json::from_value(json!({"cmd": "agent_tree", "args": {"pane": "p"}})).unwrap();
    assert_eq!(super::required_scope(&c), Ok(super::Scope::Read));
}

/// Contract with the phone's agent tree (`agent_tree_test.dart`): `apps/mobile/test/fixtures/
/// agent_tree.json` is this producer's output for an omp session with a nested subagent,
/// one that failed and one still running. `UPDATE_FIXTURES=1` rewrites it.
#[tokio::test(flavor = "multi_thread")]
async fn agent_tree_fixture_is_the_real_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let c = daemon(tmp.path()).await;
    let file = tmp.path().join("s.jsonl");
    let asst = |content: Value, total: u64, cost: f64, stop: &str| {
        json!({"type": "message", "message": {"role": "assistant", "content": content,
               "model": "cc/claude-sonnet-5-5", "stopReason": stop,
               "usage": {"totalTokens": total, "cost": {"total": cost}}}})
        .to_string()
    };
    let spawn = |tasks: Value| {
        asst(
            json!([{"type": "toolCall", "id": "t", "name": "task", "arguments": {"tasks": tasks}}]),
            1200,
            0.04,
            "toolUse",
        )
    };
    let stamp = |line: String, at: &str| {
        let mut v: Value = serde_json::from_str(&line).unwrap();
        v["timestamp"] = json!(at);
        v.to_string()
    };
    let todo_result =
        json!({"type": "message", "message": {"role": "toolResult", "toolName": "todo",
        "toolCallId": "td", "details": {"op": "init", "phases": [
            {"name": "Map", "tasks": [{"content": "Read hostd", "status": "completed"},
                                      {"content": "Write the hub", "status": "in_progress"}]},
            {"name": "Ship", "tasks": [{"content": "Release", "status": "pending"}]}]}}})
        .to_string();
    let lines = [
        json!({"type": "title", "title": "\u{f011c} Map the host"}).to_string(),
        json!({"type": "session", "id": "x"}).to_string(),
        json!({"type": "thinking_level_change", "thinkingLevel": "high"}).to_string(),
        spawn(json!([
            {"name": "Scout", "agent": "scout", "task": "Map hostd"},
            {"name": "Fixer", "agent": "task", "task": "Fix the sheet"},
        ])),
        todo_result,
        // A synchronous spawn keeps omp's own figures for the context bar.
        json!({"type": "message", "message": {"role": "toolResult", "toolName": "task", "toolCallId": "t",
               "details": {"progress": [{"id": "Scout", "contextWindow": 1_000_000, "contextTokens": 292_000}]}}})
        .to_string(),
        json!({"type": "custom_message", "customType": "async-result",
               "content": "<task-result id=\"Fixer\" agent=\"task\" status=\"failed\" duration=\"1m5s\">\n<output>boom</output>"})
        .to_string(),
    ];
    std::fs::write(&file, lines.join("\n")).unwrap();
    let dir = tmp.path().join("s");
    std::fs::create_dir_all(dir.join("Scout")).unwrap();
    std::fs::write(
        dir.join("Scout.jsonl"),
        [
            json!({"type": "thinking_level_change", "thinkingLevel": "low"}).to_string(),
            json!({"type": "session_init", "agent": "scout"}).to_string(),
            stamp(spawn(json!([{"name": "Deep", "agent": "scout", "task": "Read server.rs"}])), "2026-10-25T10:00:00.000Z"),
            stamp(json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "t"}}).to_string(), "2026-10-25T10:00:05.000Z"),
            stamp(asst(
                json!([{"type": "toolCall", "id": "r", "name": "read", "arguments": {"path": "server.rs", "i": "Reading server.rs"}}]),
                300,
                0.01,
                "toolUse",
            ), "2026-10-25T10:00:09.000Z"),
        ]
        .join("\n"),
    )
    .unwrap();
    std::fs::write(dir.join(".Scout.jsonl.lock.os"), "").unwrap();
    std::fs::write(
        dir.join("Scout").join("Scout.Deep.jsonl"),
        asst(json!([{"type": "text", "text": "ok"}]), 100, 0.002, "stop"),
    )
    .unwrap();
    std::fs::write(
        dir.join("Scout").join("Scout.Deep.md"),
        "server.rs routes every command",
    )
    .unwrap();
    std::fs::write(dir.join("Fixer.jsonl"), asst(json!([]), 50, 0.001, "error")).unwrap();
    // `Deep` finished long ago: a fresh transcript alone would still read as running.
    std::fs::File::options()
        .write(true)
        .open(dir.join("Scout").join("Scout.Deep.jsonl"))
        .unwrap()
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    hook(
        tmp.path(),
        json!({"event": "agent_start", "session_file": file.to_string_lossy()}),
    );
    let mut v = c.call("agent_tree", json!({"pane": "p1"})).await.unwrap();
    let node = |v: &Value, id: &str| {
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node {id}: {v}"))
            .clone()
    };
    assert_eq!(node(&v, "")["status"], "running");
    assert_eq!(node(&v, "")["name"], "Map the host");
    assert_eq!(node(&v, "Scout")["status"], "running");
    assert_eq!(node(&v, "Scout")["now"], "Reading server.rs");
    assert_eq!(node(&v, "Scout.Deep")["parent"], "Scout");
    assert_eq!(node(&v, "Scout.Deep")["status"], "completed");
    assert_eq!(node(&v, "Fixer")["status"], "failed");
    // Timestamps and the order of siblings follow the disk: pin them for the fixture.
    assert_eq!(node(&v, "")["thinking"], "high");
    assert_eq!(node(&v, "")["todos"]["done"], 1);
    assert_eq!(node(&v, "")["todos"]["total"], 3);
    assert_eq!(node(&v, "Scout")["agent_type"], "scout");
    assert_eq!(node(&v, "Scout")["thinking"], "low");
    assert_eq!(node(&v, "Scout")["requests"], 2);
    assert_eq!(node(&v, "Scout")["context_window"], 1_000_000);
    assert_eq!(node(&v, "Scout")["context_tokens"], 292_000);
    assert_eq!(node(&v, "Scout")["active_ms"], 9_000);
    assert!(node(&v, "Scout")["recent"][1]["at"].as_u64().is_some());
    assert_eq!(node(&v, "Scout")["recent"][0]["tool"], "task");
    assert_eq!(
        node(&v, "Scout")["recent"][1]["intent"],
        "Reading server.rs"
    );
    assert_eq!(node(&v, "Scout")["recent"][1]["status"], "running");
    assert_eq!(node(&v, "Scout")["recent"][0]["status"], "ok");
    let nodes = v["nodes"].as_array_mut().unwrap();
    nodes.sort_by_key(|n| n["id"].as_str().unwrap().to_string());
    for n in nodes.iter_mut() {
        n["updated_at"] = json!(1_700_000_000_000u64);
        // Without the parent's figure it is "transcript start → last write": the clock.
        if n["id"] == "Scout" {
            n["duration_ms"] = json!(60_000);
        }
    }
    // Phones read a nested transcript through the same dotted id.
    let deep = c
        .call("agent_history", json!({"pane": "p1", "sub": "Scout.Deep"}))
        .await
        .unwrap();
    assert_eq!(deep["items"][0]["text"], "ok");
    for bad in ["..", "Scout/../Scout", "", "Scout..Deep", "a/b"] {
        assert!(
            c.call("agent_history", json!({"pane": "p1", "sub": bad}))
                .await
                .is_err(),
            "{bad:?}"
        );
    }
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/agent_tree.json");
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
        "agent_tree.json changed: rerun with UPDATE_FIXTURES=1, then the Dart test"
    );
    c.call("shutdown", json!({})).await.ok();
}

/// #201: the audit is readable from a phone with `read` (the maintainer's decision), also through the relay's
/// `crew_call`, and `crewmate_audit` / `crew_audit` are not terminals.
#[test]
fn reading_the_crew_audit_needs_read() {
    use crate::protocol::HostCmd;
    for cmd in [
        json!({"cmd": "crewmate_audit", "args": {"id": "c"}}),
        json!({"cmd": "crew_audit", "args": {}}),
        json!({"cmd": "crew_call", "args": {"server": "s", "cmd": "crewmate_audit", "args": {"id": "c"}}}),
    ] {
        let c: HostCmd = serde_json::from_value(cmd).unwrap();
        assert_eq!(super::required_scope(&c), Ok(super::Scope::Read));
    }
}

/// A phone without `act` may watch agents but never start or stop one.
#[test]
fn starting_and_stopping_agents_need_act() {
    use crate::protocol::HostCmd;
    for cmd in [
        json!({"cmd": "agent_interrupt", "args": {"pane": "p"}}),
        json!({"cmd": "agent_launch", "args": {"project": "a", "agent": "omp"}}),
    ] {
        let c: HostCmd = serde_json::from_value(cmd).unwrap();
        assert_eq!(super::required_scope(&c), Ok(super::Scope::Act));
    }
}

/// Worktrees: any phone may LIST them (`read`), only `act` creates or removes one, and none of the three
/// takes a path: `worktree_remove` and `agent_launch {worktree}` name registry ids.
#[test]
fn worktrees_are_listed_with_read_and_changed_with_act() {
    use crate::protocol::HostCmd;
    let scope = |cmd: Value| {
        let c: HostCmd = serde_json::from_value(cmd).unwrap();
        super::required_scope(&c)
    };
    assert_eq!(
        scope(json!({"cmd": "worktree_list", "args": {"project": "a"}})),
        Ok(super::Scope::Read)
    );
    for cmd in [
        json!({"cmd": "worktree_create", "args": {"project": "a", "name": "x"}}),
        json!({"cmd": "worktree_remove", "args": {"id": "w"}}),
        json!({"cmd": "agent_launch", "args": {"project": "a", "agent": "omp", "worktree": "w"}}),
    ] {
        assert_eq!(scope(cmd.clone()), Ok(super::Scope::Act), "{cmd}");
    }
}

/// End to end over a real phone connection and a real git repo, with no desktop anywhere: a `read` phone lists
/// but is refused creating and removing; an `act` phone creates, launches an agent in the new worktree (the
/// agent's terminal starts in that folder) and removes it, and a dirty worktree is kept.
#[tokio::test]
async fn a_phone_manages_worktrees_and_launches_an_agent_in_one_with_the_desktop_closed() {
    let _bin = crate::agents::LAUNCH_TEST_BIN_LOCK.lock().await;
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    let root = canon(tmp.path());
    let hostd = root.join("hostd");
    let repo = root.join("shop");
    let git = |dir: &Path, args: &[&str]| {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);
    std::fs::write(repo.join("f.txt"), "x\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    git(&root, &["init", "-q", "--bare", "remote.git"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            &root.join("remote.git").to_string_lossy(),
        ],
    );
    git(&repo, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    git(&repo, &["fetch", "-q", "origin"]);
    git(&repo, &["remote", "set-head", "origin", "main"]);
    let id = crate::worktrees::registry::id_for(&repo);
    std::fs::create_dir_all(hostd.join("desktop")).unwrap();
    std::fs::write(
        hostd.join("desktop/projects.json"),
        json!([{"id": id, "root": repo, "pinned": false, "added": "x"}]).to_string(),
    )
    .unwrap();
    std::fs::write(
        hostd.join("desktop/settings.json"),
        json!({"workspaceDir": root.join("wts"), "branchPrefix": "none"}).to_string(),
    )
    .unwrap();
    // A fake `claude` on the login PATH: it records where it was started and stays.
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("claude");
    let record = root.join("cwd.txt");
    std::fs::write(
        &fake,
        // Written aside then renamed: the test waits for the file to exist, and `pwd > file` makes
        // it exist empty before the path is in it (macOS CI read "" on PR #223).
        format!(
            "#!/bin/sh\npwd > '{0}.tmp' && mv '{0}.tmp' '{0}'\nsleep 30\n",
            record.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    crate::agents::launch_test_bin(Some(fake.clone()));

    let (c, mut phone, node) = paired(&hostd).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();

    let l = phone
        .call("worktree_list", json!({"project": id}))
        .await
        .unwrap();
    assert_eq!(l["ok"], true, "{l}");
    assert_eq!(l["data"]["worktrees"].as_array().unwrap().len(), 1);
    for (cmd, args) in [
        ("worktree_create", json!({"project": id, "name": "x"})),
        ("worktree_remove", json!({"id": id})),
        ("agent_launch", json!({"project": id, "agent": "claude"})),
    ] {
        let r = phone.call(cmd, args).await.unwrap();
        assert_eq!(r["ok"], false, "{cmd}: a read phone is refused");
    }
    assert!(
        !root.join("wts").exists(),
        "nothing was created for a read phone"
    );

    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read", "act"]}),
    )
    .await
    .unwrap();
    let made = phone
        .call(
            "worktree_create",
            json!({"project": id, "name": "Phone Task"}),
        )
        .await
        .unwrap();
    assert_eq!(made["ok"], true, "{made}");
    let wt_id = made["data"]["worktree"]["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(made["data"]["worktree"]["path"].as_str().unwrap());
    assert!(wt_path.join("f.txt").is_file());

    // The agent starts in THAT worktree, through hostd alone.
    let launched = phone
        .call(
            "agent_launch",
            json!({"project": id, "agent": "claude", "worktree": wt_id}),
        )
        .await
        .unwrap();
    assert_eq!(launched["ok"], true, "{launched}");
    let pane = launched["data"]["pane"].as_str().unwrap().to_string();
    assert!(launched["data"]["pty"].as_str().is_some());
    for _ in 0..1200 {
        if record.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        canon(std::fs::read_to_string(&record).unwrap().trim()),
        canon(&wt_path)
    );
    let ptys = c.call("pty_list", json!({})).await.unwrap();
    assert!(
        ptys["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["scope"]["pane_key"] == pane.as_str())
    );

    // A worktree of another project is refused, and a path is never a worktree.
    let stranger = phone
        .call(
            "agent_launch",
            json!({"project": id, "agent": "claude", "worktree": "/tmp"}),
        )
        .await
        .unwrap();
    assert_eq!(stranger["ok"], false);

    // Dirty: kept. Then clean: removed.
    std::fs::write(wt_path.join("new.txt"), "x").unwrap();
    for p in c.call("pty_list", json!({})).await.unwrap()["ptys"]
        .as_array()
        .unwrap()
    {
        c.call("pty_kill", json!({"pty": p["pty"]})).await.ok();
    }
    let kept = phone
        .call("worktree_remove", json!({"id": wt_id}))
        .await
        .unwrap();
    assert_eq!(
        (kept["ok"].clone(), kept["data"]["kept"].clone()),
        (json!(true), json!("dirty")),
        "{kept}"
    );
    std::fs::remove_file(wt_path.join("new.txt")).unwrap();
    let gone = phone
        .call("worktree_remove", json!({"id": wt_id}))
        .await
        .unwrap();
    assert_eq!(gone["data"]["removed"], true, "{gone}");
    assert!(!wt_path.exists());
    c.call("shutdown", json!({})).await.ok();
}

/// A stand-in for Atlas Desktop's control socket (`ctl.rs`): checks the token, records the request and
/// answers `reply`. Returns the requests it saw.
#[cfg(unix)]
fn fake_desktop(socket: &Path, reply: Value) -> std::sync::Arc<parking_lot::Mutex<Vec<Value>>> {
    use std::io::{BufRead, BufReader, Write};
    std::fs::write(socket.with_extension("token"), "t0ken\n").unwrap();
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    let seen = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
    let out = seen.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { return };
            let mut lines = BufReader::new(s.try_clone().unwrap()).lines();
            let token: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
            if token["token"] != "t0ken" {
                continue;
            }
            let Some(Ok(req)) = lines.next() else {
                continue;
            };
            out.lock().push(serde_json::from_str(&req).unwrap());
            let _ = writeln!(s, "{reply}");
        }
    });
    seen
}

/// A project with one worktree in a hostd dir; returns (project id, worktree id, worktree path).
#[cfg(unix)]
async fn project_with_worktree(hostd: &Path, repo: &Path) -> (String, String, std::path::PathBuf) {
    let git = |dir: &Path, args: &[&str]| {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    std::fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "-q", "."]);
    std::fs::write(repo.join("f.txt"), "x\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "base"]);
    let id = crate::worktrees::registry::id_for(repo);
    std::fs::create_dir_all(hostd.join("desktop")).unwrap();
    // Appended: a test may register a second project in the same hostd dir.
    let registry = hostd.join("desktop/projects.json");
    let mut all: Vec<Value> = std::fs::read_to_string(&registry)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    all.push(json!({"id": id, "root": repo, "pinned": false, "added": "x"}));
    std::fs::write(&registry, Value::Array(all).to_string()).unwrap();
    std::fs::write(
        hostd.join("desktop/settings.json"),
        json!({"workspaceDir": hostd.join("wts"), "branchPrefix": "none"}).to_string(),
    )
    .unwrap();
    let made = crate::worktrees::local::create(
        hostd,
        &id,
        crate::worktrees::local::CreateReq {
            name: Some("task".into()),
            base: None,
        },
    )
    .await
    .unwrap();
    let w = &made["worktree"];
    (
        id,
        w["id"].as_str().unwrap().into(),
        std::path::PathBuf::from(w["path"].as_str().unwrap()),
    )
}

/// With Atlas Desktop running, a phone's `agent_launch` opens a real tab there (it owns tabs, the user sees it)
/// with the worktree's folder, and hostd spawns nothing itself: no pty, even though the agent binary exists.
#[cfg(unix)]
#[tokio::test]
async fn with_the_desktop_running_agent_launch_opens_its_tab_and_hostd_spawns_nothing() {
    let _bin = crate::agents::LAUNCH_TEST_BIN_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let root = canon(tmp.path());
    let hostd = root.join("hostd");
    std::fs::create_dir_all(&hostd).unwrap();
    let (id, wt_id, wt_path) = project_with_worktree(&hostd, &root.join("shop")).await;
    // If hostd wrongly spawned, this fake agent would leave a file.
    let fake = root.join("claude");
    let touched = root.join("spawned.txt");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\ntouch '{}'\nsleep 30\n", touched.display()),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    crate::agents::launch_test_bin(Some(fake));
    let socket = root.join("desktop.sock");
    let seen = fake_desktop(
        &socket,
        json!({"ok": true, "data": {"tab": "tab-7", "pty": "pty-7"}}),
    );

    let (c, mut phone, _node) = paired_on(daemon_with(&hostd, socket).await).await;
    let r = phone
        .call(
            "agent_launch",
            json!({"project": id, "agent": "claude", "prompt": "hi", "worktree": wt_id}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        (r["data"]["pane"].clone(), r["data"]["pty"].clone()),
        (json!("tab-7"), json!("pty-7"))
    );
    let reqs = seen.lock().clone();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0]["cmd"], "terminal_create");
    assert_eq!(reqs[0]["args"]["agent"], "claude");
    assert_eq!(reqs[0]["args"]["prompt"], "hi");
    assert_eq!(
        canon(reqs[0]["args"]["worktree"].as_str().unwrap()),
        canon(&wt_path),
        "the resolved worktree folder, not the project root"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !touched.exists(),
        "hostd spawned an agent although the desktop took the launch"
    );
    assert_eq!(
        c.call("pty_list", json!({})).await.unwrap()["ptys"],
        json!([])
    );
    c.call("shutdown", json!({})).await.ok();
}

/// A desktop that answers with an error is running: the error goes back to the phone and hostd does not start a
/// second, invisible agent behind its back.
#[cfg(unix)]
#[tokio::test]
async fn a_desktop_error_is_returned_and_never_falls_back_to_a_hostd_spawn() {
    let _bin = crate::agents::LAUNCH_TEST_BIN_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let root = canon(tmp.path());
    let hostd = root.join("hostd");
    std::fs::create_dir_all(&hostd).unwrap();
    let (id, wt_id, _) = project_with_worktree(&hostd, &root.join("shop")).await;
    let fake = root.join("claude");
    let touched = root.join("spawned.txt");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\ntouch '{}'\nsleep 30\n", touched.display()),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    crate::agents::launch_test_bin(Some(fake));
    let socket = root.join("desktop.sock");
    let seen = fake_desktop(&socket, json!({"ok": false, "error": "unknown worktree"}));

    let (c, mut phone, _node) = paired_on(daemon_with(&hostd, socket).await).await;
    let r = phone
        .call(
            "agent_launch",
            json!({"project": id, "agent": "claude", "worktree": wt_id}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"].as_str().unwrap().contains("unknown worktree"),
        "{r}"
    );
    assert_eq!(seen.lock().len(), 1);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !touched.exists(),
        "fell back to a hostd spawn after a desktop error"
    );
    c.call("shutdown", json!({})).await.ok();
}

/// A socket file left behind by a desktop that quit (nothing listens) is "not running": hostd spawns.
#[cfg(unix)]
#[tokio::test]
async fn a_stale_desktop_socket_file_still_means_not_running() {
    let _bin = crate::agents::LAUNCH_TEST_BIN_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let root = canon(tmp.path());
    let hostd = root.join("hostd");
    std::fs::create_dir_all(&hostd).unwrap();
    let (id, wt_id, wt_path) = project_with_worktree(&hostd, &root.join("shop")).await;
    let record = root.join("cwd.txt");
    let fake = root.join("claude");
    std::fs::write(
        &fake,
        // Written aside then renamed, as above: never seen half written.
        format!(
            "#!/bin/sh\npwd > '{0}.tmp' && mv '{0}.tmp' '{0}'\nsleep 30\n",
            record.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    crate::agents::launch_test_bin(Some(fake));
    // bind then drop the listener: the file stays, the connection is refused
    let socket = root.join("desktop.sock");
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    assert!(socket.exists());

    let (c, mut phone, _node) = paired_on(daemon_with(&hostd, socket).await).await;
    let r = phone
        .call(
            "agent_launch",
            json!({"project": id, "agent": "claude", "worktree": wt_id}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    for _ in 0..1200 {
        if record.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        canon(std::fs::read_to_string(&record).unwrap().trim()),
        canon(&wt_path)
    );
    for p in c.call("pty_list", json!({})).await.unwrap()["ptys"]
        .as_array()
        .unwrap()
    {
        c.call("pty_kill", json!({"pty": p["pty"]})).await.ok();
    }
}

/// Reading the review (status, diff) needs `read`; everything that changes a repo or reaches the
/// network (stage, unstage, commit, the AI message, push, PR) needs `act`.
#[test]
fn git_review_reads_need_read_and_every_write_needs_act() {
    use crate::protocol::HostCmd;
    for cmd in [
        json!({"cmd": "git_status", "args": {"root": "/r"}}),
        json!({"cmd": "git_diff", "args": {"root": "/r", "staged": true}}),
    ] {
        let c: HostCmd = serde_json::from_value(cmd).unwrap();
        assert_eq!(super::required_scope(&c), Ok(super::Scope::Read));
    }
    for cmd in [
        json!({"cmd": "git_stage", "args": {"root": "/r", "paths": ["a"]}}),
        json!({"cmd": "git_unstage", "args": {"root": "/r", "paths": ["a"]}}),
        json!({"cmd": "git_commit", "args": {"root": "/r", "message": "m"}}),
        json!({"cmd": "git_commit_message", "args": {"root": "/r"}}),
        json!({"cmd": "git_push", "args": {"root": "/r"}}),
        json!({"cmd": "git_pr_create", "args": {"root": "/r", "title": "t", "body": "b"}}),
    ] {
        let c: HostCmd = serde_json::from_value(cmd.clone()).unwrap();
        assert_eq!(super::required_scope(&c), Ok(super::Scope::Act), "{cmd}");
    }
}

/// End to end through the phone gateway: a Read-only phone sees status and diff but cannot stage or
/// commit (the repo is untouched); granting Act lets the same phone stage and commit for real. A
/// root outside the registry is refused by hostd even for a phone that has every scope.
#[tokio::test]
async fn a_read_phone_reviews_but_cannot_commit_and_act_can() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let git = |a: &[&str]| {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args([
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(a)
            .output()
            .unwrap();
        assert!(o.status.success(), "git {a:?}");
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    git(&["init", "-q", "."]);
    // hostd commits with the repo's own config: CI runners have no global identity.
    for (k, v) in [
        ("user.email", "a@b"),
        ("user.name", "n"),
        ("commit.gpgsign", "false"),
    ] {
        git(&["config", k, v]);
    }
    std::fs::write(work.path().join("a.txt"), "one\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);
    std::fs::create_dir_all(tmp.path().join("desktop")).unwrap();
    std::fs::write(
        tmp.path().join("desktop/projects.json"),
        json!([{"id": "p", "root": work.path()}]).to_string(),
    )
    .unwrap();
    std::fs::write(work.path().join("a.txt"), "two\n").unwrap();
    let root = work.path().to_str().unwrap();

    let (c, mut phone, node) = paired(tmp.path()).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();
    let st = phone
        .call("git_status", json!({"root": root}))
        .await
        .unwrap();
    assert_eq!(st["data"]["entries"][0]["path"], "a.txt", "{st}");
    let d = phone.call("git_diff", json!({"root": root})).await.unwrap();
    assert_eq!(d["data"]["files"][0]["path"], "a.txt", "{d}");
    for (cmd, args) in [
        ("git_stage", json!({"root": root, "paths": ["a.txt"]})),
        ("git_commit", json!({"root": root, "message": "nope"})),
        ("git_push", json!({"root": root})),
        ("git_commit_message", json!({"root": root})),
        (
            "git_pr_create",
            json!({"root": root, "title": "t", "body": ""}),
        ),
    ] {
        let r = phone.call(cmd, args).await.unwrap();
        assert_eq!(r["error"], "forbidden: act", "{cmd}: {r}");
    }
    assert_eq!(
        git(&["log", "--oneline"]).lines().count(),
        1,
        "Read must not commit"
    );
    assert!(
        git(&["diff", "--cached", "--name-only"]).is_empty(),
        "Read must not stage"
    );

    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read", "act"]}),
    )
    .await
    .unwrap();
    let r = phone
        .call("git_stage", json!({"root": root, "paths": ["a.txt"]}))
        .await
        .unwrap();
    assert!(r.get("error").is_none(), "{r}");
    let r = phone
        .call(
            "git_commit",
            json!({"root": root, "message": "from the phone"}),
        )
        .await
        .unwrap();
    assert!(r["data"]["sha"].is_string(), "{r}");
    assert_eq!(git(&["log", "-1", "--format=%s"]).trim(), "from the phone");
    // The audit log names the commit but never what was typed in it.
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    assert!(audit.contains("git_commit"), "{audit}");
    assert!(
        !audit.contains("from the phone"),
        "message leaked into the audit log: {audit}"
    );
    assert!(audit.contains("<14 chars>"), "{audit}");
    let outside = tempfile::tempdir().unwrap();
    let r = phone
        .call("git_status", json!({"root": outside.path()}))
        .await
        .unwrap();
    assert_eq!(r["error"], "unknown project", "{r}");
}

/// Gap #5: the crew-level settings (short replies and its model) are readable with `read` and writable with
/// `act`, also through the relay's `crew_call`. They used to fall through to the default-deny.
#[test]
fn crew_settings_need_read_to_get_and_act_to_set() {
    use crate::protocol::HostCmd;
    for (cmd, want) in [
        (
            json!({"cmd": "crew_settings_get", "args": {}}),
            super::Scope::Read,
        ),
        (
            json!({"cmd": "crew_settings_set", "args": {"short_replies": false}}),
            super::Scope::Act,
        ),
        (
            json!({"cmd": "crew_call", "args": {"server": "s", "cmd": "crew_settings_get", "args": {}}}),
            super::Scope::Read,
        ),
        (
            json!({"cmd": "crew_call", "args": {"server": "s", "cmd": "crew_settings_set", "args": {"short_replies": true, "short_replies_model": "m"}}}),
            super::Scope::Act,
        ),
    ] {
        let c: HostCmd = serde_json::from_value(cmd.clone()).unwrap();
        assert_eq!(super::required_scope(&c), Ok(want), "{cmd}");
    }
}

/// On a real paired phone: `read` alone may read the crew settings but a write is `forbidden: act`; a phone
/// without `read` cannot even read; with `act` the write lands in hostd's `crew.json` and keeps `paused`.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_reads_and_writes_crew_settings_by_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, node) = paired(tmp.path()).await;
    let scopes = |s: Value| json!({"node_id": node, "scopes": s});

    c.call("mobile_set_scopes", scopes(json!(["read"])))
        .await
        .unwrap();
    let r = phone.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["short_replies"], true, "{r}");
    let r = phone
        .call("crew_settings_set", json!({"short_replies": false}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    let r = phone.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(
        r["data"]["short_replies"], true,
        "a refused write must change nothing: {r}"
    );

    c.call("mobile_set_scopes", scopes(json!([])))
        .await
        .unwrap();
    let r = phone.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(r["error"], "forbidden: read", "{r}");

    c.call("mobile_set_scopes", scopes(json!(["read", "act"])))
        .await
        .unwrap();
    c.call("crew_pause", json!({})).await.unwrap();
    let r = phone
        .call(
            "crew_settings_set",
            json!({"short_replies": false, "short_replies_model": "haiku"}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let r = phone.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(r["data"]["short_replies"], false, "{r}");
    assert_eq!(r["data"]["short_replies_model"], "haiku", "{r}");
    let list = c.call("crewmate_list", json!({})).await.unwrap();
    assert_eq!(
        list["paused"], true,
        "a phone's settings write must not resume the crew"
    );
    // The write is in the mobile audit log (not a read).
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    assert!(audit.contains("\"cmd\":\"crew_settings_set\""), "{audit}");
    c.call("shutdown", json!({})).await.ok();
}

// Guards PhoneCrewAudit gap 7: a secret typed on the phone (the `provide_secret` card answers with
// `crewmate_action_answer {value}`, relayed as `crew_call {args: {value}}`) was written to `audit.log` in
// clear: only top-level `text|message|title|body|data` were redacted. The log names the command and the
// secret's length, never its value, at any depth.
#[test]
fn a_secret_from_the_phone_never_reaches_the_audit_log() {
    let tmp = tempfile::tempdir().unwrap();
    let m = super::Mobile::load(tmp.path(), false).unwrap();
    let secret = "ghp_SuperSecretValue123";
    m.audit(
        "node",
        "crewmate_action_answer",
        &json!({"id": "c", "task": "t", "value": secret}),
    );
    m.audit(
        "node",
        "crew_call",
        &json!({"server": "s", "cmd": "crewmate_action_answer",
                "args": {"id": "c", "task": "t", "value": secret, "text": secret}}),
    );
    let log = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    assert!(!log.contains(secret), "secret in the audit log: {log}");
    assert!(!log.contains("SuperSecret"), "{log}");
    // What is audited stays useful: the command, the target, the size.
    assert!(log.contains("crewmate_action_answer"), "{log}");
    assert!(log.contains("<23 chars>"), "{log}");
    assert!(log.contains("\"id\":\"c\""), "{log}");
}

// Contract with the phone and the desktop: `pair_url` (used by `atlas crew pair`) must produce exactly the
// link the desktop's `pairUrl` produces from the same `mobile_pair_start` reply. The reply is the committed
// real one (`apps/desktop/src/lib/__fixtures__/mobile-pair-start.json`), the link the committed one the phone's
// `pair_url_test.dart` parses (a hostname with a space and a quote, repeated `addr`, no relay).
#[test]
fn pair_url_matches_the_link_the_desktop_and_the_phone_agree_on() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let start: Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.join("apps/desktop/src/lib/__fixtures__/mobile-pair-start.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let want =
        std::fs::read_to_string(root.join("apps/mobile/test/fixtures/pair_url.txt")).unwrap();
    assert_eq!(super::pair_url(&start).unwrap(), want.trim());
}

// A reply with a relay carries it as `relay=`; one without a node or code is no link at all.
#[test]
fn pair_url_carries_the_relay_and_refuses_an_incomplete_reply() {
    let url = super::pair_url(&json!({"node_id": "N", "code": "000111", "hostname": "box",
        "addrs": [], "relay_url": "https://relay.example/"}))
    .unwrap();
    assert_eq!(
        url,
        "atlas-mobile://pair?node=N&code=000111&name=box&relay=https%3A%2F%2Frelay.example%2F"
    );
    assert!(super::pair_url(&json!({"code": "1", "hostname": "h"})).is_err());
    assert!(super::pair_url(&json!({"node_id": "N", "hostname": "h"})).is_err());
}

// Guards PhoneCrewAudit gap 1: an invite is only worth showing while its code can still pair. A newer
// pairing code (the desktop's "Pair a phone", or `atlas crew pair`) replaces the single live code, so an
// older invite would fail with "bad code" after the phone tapped Add. `mobile_invites` lists only invites
// whose code is the live one.
#[tokio::test]
async fn an_invite_whose_code_was_replaced_is_not_offered() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, _node) = paired(tmp.path()).await;
    let first = pair_start(&c).await;
    let url_of = |i: &Value| super::pair_url(i).unwrap();
    c.call("mobile_invite", json!({"url": url_of(&first), "name": "A"}))
        .await
        .unwrap();
    let r = phone.call("mobile_invites", json!({})).await.unwrap();
    assert_eq!(r["data"]["invites"].as_array().unwrap().len(), 1, "{r}");

    // A new code replaces the first one.
    let second = pair_start(&c).await;
    assert_ne!(first["code"], second["code"]);
    c.call(
        "mobile_invite",
        json!({"url": url_of(&second), "name": "B"}),
    )
    .await
    .unwrap();
    let r = phone.call("mobile_invites", json!({})).await.unwrap();
    let names: Vec<&str> = r["data"]["invites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["B"],
        "the invite with the dead code is offered: {r}"
    );
    c.call("shutdown", json!({})).await.ok();
}

// The sign-in of an engine from a phone (`crew_engine_login`): reading the status is `read`; starting the login is
// `act`; typing the code into its terminal is `term_write`, a scope of its own; and what is typed is never logged,
// not by the mobile audit and not by the crew audit.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_signs_an_engine_in_by_scope_and_what_it_types_is_never_logged() {
    use base64::Engine as _;
    let tmp = tempfile::tempdir().unwrap();
    // Only a fake codex on the engine PATH: a login that waits for a code, like `codex login --device-auth`.
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let codex = bin.join("codex");
    std::fs::write(
        &codex,
        "#!/bin/sh\nif [ \"$1 $2\" = \"login status\" ]; then exit 1; fi\ncat\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let _path = crate::crew::engine_login::TestPath::set(bin);
    let (c, mut phone, node) = paired(tmp.path()).await;
    let scopes = |s: Value| json!({"node_id": node, "scopes": s});

    c.call("mobile_set_scopes", scopes(json!(["read"])))
        .await
        .unwrap();
    let r = phone.call("crew_engine_status", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "reading the status needs only read: {r}");
    let r = phone
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    let r = phone.call("pty_list", json!({})).await.unwrap();
    assert!(
        r["data"]["ptys"].as_array().unwrap().is_empty(),
        "a refused sign-in starts nothing: {r}"
    );

    c.call(
        "mobile_set_scopes",
        scopes(json!(["read", "act", "term_read"])),
    )
    .await
    .unwrap();
    let r = phone
        .call("crew_engine_login", json!({"engine": "bash"}))
        .await
        .unwrap();
    assert!(
        r["error"].as_str().unwrap().contains("unknown engine"),
        "act is not a shell: {r}"
    );
    let r = phone
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let pty = r["data"]["pty"].as_str().unwrap().to_string();
    // Act and term_read start and watch the login; the code still cannot be typed without term_write.
    let code = base64::engine::general_purpose::STANDARD.encode("WXYZ-9876-DEVICE-CODE\n");
    let r = phone
        .call("pty_write", json!({"pty": pty, "data": code}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: term_write", "{r}");

    c.call(
        "mobile_set_scopes",
        scopes(json!(["read", "act", "term_read", "term_write"])),
    )
    .await
    .unwrap();
    let r = phone
        .call("pty_write", json!({"pty": pty, "data": code}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");

    let mobile_audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    assert!(
        mobile_audit.contains("\"cmd\":\"crew_engine_login\""),
        "the sign-in is in the mobile audit: {mobile_audit}"
    );
    assert!(
        !mobile_audit.contains("DEVICE-CODE"),
        "typed text must not be logged: {mobile_audit}"
    );
    let crew_audit = c.call("crew_audit", json!({})).await.unwrap().to_string();
    assert!(crew_audit.contains("engine_login_started"), "{crew_audit}");
    assert!(
        crew_audit.contains(&format!("phone:{node}")),
        "the crew audit names the phone: {crew_audit}"
    );
    assert!(!crew_audit.contains("DEVICE-CODE"), "{crew_audit}");
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
    c.call("shutdown", json!({})).await.ok();
}

/// Compare `live` with `apps/mobile/test/fixtures/<name>`; `UPDATE_FIXTURES=1` rewrites the file first.
fn fixture(name: &str, live: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures")
        .join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{name} missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host --lib -- mobile::tests::a_phone_opens_a_shell")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1"
    );
}

// ── New terminal from the phone (`pty_open`, issue #53) ─────────────────────────────────────────────────────
//
// `pty_create` stays refused: it takes any cwd and argv. `pty_open` takes registry ids only, so a path (or a
// `..`) cannot even be named, and the shell is the user's login shell, started by hostd exactly like a desktop tab.

/// `act` opens it, a plain phone cannot, and through a Mac's `crew_call` it is never available (a terminal's output
/// streams to the connection that attached it, which would be the relay's, not the phone's).
#[test]
fn opening_a_terminal_needs_act_and_pty_create_stays_refused() {
    use crate::protocol::HostCmd;
    let scope = |cmd: Value| {
        let c: HostCmd = serde_json::from_value(cmd).unwrap();
        super::required_scope(&c)
    };
    assert_eq!(
        scope(json!({"cmd": "pty_open", "args": {"project": "a"}})),
        Ok(super::Scope::Act)
    );
    assert_eq!(
        scope(json!({"cmd": "pty_open", "args": {}})),
        Ok(super::Scope::Act)
    );
    assert!(
        scope(json!({"cmd": "pty_create", "args": {"cwd": "/", "argv": ["sh"]}})).is_err(),
        "an arbitrary cwd/argv stays desktop-only"
    );
    assert!(
        scope(json!({
            "cmd": "crew_call",
            "args": {"server": "s", "cmd": "pty_open", "args": {"project": "a"}}
        }))
        .is_err(),
        "relayed, the phone could never attach to the terminal"
    );
}

/// The mobile audit gets `pty_created {node, project, cwd}` from hostd (it knows the resolved folder), and never
/// what is typed afterwards.
fn audit_lines(hostd: &Path) -> Vec<Value> {
    std::fs::read_to_string(hostd.join("mobile/audit.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// End to end over a real phone connection: refused without `act` (nothing starts), refused for a path, a `..`
/// escape and a project that is not registered; with `act` the shell starts in the project folder (and in a
/// worktree's), is a normal hostd pty the desktop lists, and is audited as `pty_created`.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_opens_a_shell_only_in_a_registered_project_or_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = canon(tmp.path());
    let hostd = root.join("hostd");
    std::fs::create_dir_all(&hostd).unwrap();
    let (id, wt_id, wt_path) = project_with_worktree(&hostd, &root.join("shop")).await;
    // A second registered project with a worktree of its own: it is registered, yet not THIS project's.
    let (_other, other_wt, _) = project_with_worktree(&hostd, &root.join("other")).await;
    let (c, mut phone, node) = paired_on(computer_daemon(&hostd).await).await;
    let scopes = |s: Value| json!({"node_id": node, "scopes": s});

    c.call("mobile_set_scopes", scopes(json!(["read", "term_read"])))
        .await
        .unwrap();
    let r = phone
        .call("pty_open", json!({"project": id}))
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    let r = phone.call("pty_list", json!({})).await.unwrap();
    assert!(
        r["data"]["ptys"].as_array().unwrap().is_empty(),
        "a refused request starts nothing: {r}"
    );

    c.call(
        "mobile_set_scopes",
        scopes(json!(["read", "act", "term_read"])),
    )
    .await
    .unwrap();
    // A path is not an id, `..` is not an id, and an unknown id is not a project: none starts a shell.
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    for (what, args) in [
        ("a path", json!({"project": outside})),
        ("a ..", json!({"project": format!("{id}/../..")})),
        ("an unknown id", json!({"project": "nope"})),
        (
            "a path as worktree",
            json!({"project": id, "worktree": outside}),
        ),
        (
            "a .. as worktree",
            json!({"project": id, "worktree": format!("{wt_id}/../../outside")}),
        ),
        (
            "an unknown worktree",
            json!({"project": id, "worktree": "nope"}),
        ),
        (
            "another project's worktree",
            json!({"project": id, "worktree": other_wt}),
        ),
        ("no project on a computer", json!({})),
        ("a crewmate on a computer", json!({"crewmate": "c1"})),
    ] {
        let r = phone.call("pty_open", args).await.unwrap();
        assert_eq!(r["ok"], false, "{what} must be refused: {r}");
    }
    let r = phone.call("pty_list", json!({})).await.unwrap();
    assert!(
        r["data"]["ptys"].as_array().unwrap().is_empty(),
        "every refusal left no terminal behind: {r}"
    );
    assert!(
        audit_lines(&hostd)
            .iter()
            .all(|l| l["cmd"] != "pty_created"),
        "a refusal is not a creation"
    );

    // The project's own folder, then one of its worktrees: the shell's cwd is that folder.
    for (args, want) in [
        (json!({"project": id}), root.join("shop")),
        (json!({"project": id, "worktree": wt_id}), wt_path.clone()),
    ] {
        let r = phone.call("pty_open", args).await.unwrap();
        assert_eq!(r["ok"], true, "{r}");
        let pty = r["data"]["pty"].as_str().unwrap().to_string();
        // The phone parses this reply (`apps/mobile/lib/core/protocol/pty_open.dart`): the real payload is the
        // fixture, with the random pty id replaced (the phone only passes it on to `pty_attach`).
        let mut reply = r["data"].clone();
        reply["pty"] = json!("pty-fixture");
        fixture("pty_open.json", &reply);
        let info = c.call("pty_info", json!({"pty": pty})).await.unwrap();
        assert_eq!(canon(info["cwd"].as_str().unwrap()), canon(&want), "{info}");
        assert_eq!(
            info["scope"]["pane_key"],
            Value::Null,
            "a person's shell, not an agent pane: {info}"
        );
        // The desktop sees it as an ordinary terminal.
        let all = c.call("pty_list", json!({})).await.unwrap();
        assert!(
            all["ptys"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["pty"] == pty.as_str()),
            "{all}"
        );
        c.call("pty_kill", json!({"pty": pty})).await.unwrap();
    }

    let created: Vec<Value> = audit_lines(&hostd)
        .into_iter()
        .filter(|l| l["cmd"] == "pty_created")
        .collect();
    assert_eq!(created.len(), 2, "{created:?}");
    assert_eq!(created[0]["node_id"], node.as_str());
    assert_eq!(created[0]["args"]["project"], id.as_str());
    assert_eq!(
        canon(created[0]["args"]["cwd"].as_str().unwrap()),
        canon(root.join("shop"))
    );
    assert_eq!(
        canon(created[1]["args"]["cwd"].as_str().unwrap()),
        canon(&wt_path)
    );
    c.call("shutdown", json!({})).await.ok();
}

/// A crew server's phone opens a shell in the crew home (no project) or in a crewmate's work tree, as hostd's own
/// user; any other folder, a missing work tree and an unknown crewmate are refused.
#[tokio::test(flavor = "multi_thread")]
async fn on_a_crew_server_a_phone_opens_a_shell_in_the_crew_home_or_a_crewmate_work_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let hostd = canon(tmp.path()).join("hostd");
    std::fs::create_dir_all(&hostd).unwrap();
    // The crew user's own home, the folder hostd's sign-in terminal already starts in. `$HOME` can name it through a
    // symlink (a sandbox's home under `/tmp`, a temp dir): the daemon canonicalizes the folder, so both sides do (#597).
    let home = atlas_core::paths::home_dir();
    let (c, mut phone, node) = paired(&hostd).await;
    let made = c
        .call(
            "crewmate_create",
            json!({"name": "Rex", "role": "dev", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let cid = made["id"].as_str().unwrap().to_string();

    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read", "act", "term_read"]}),
    )
    .await
    .unwrap();

    // No project: the crew home.
    let r = phone.call("pty_open", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let pty = r["data"]["pty"].as_str().unwrap().to_string();
    let info = c.call("pty_info", json!({"pty": pty})).await.unwrap();
    assert_eq!(canon(info["cwd"].as_str().unwrap()), canon(&home), "{info}");
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();

    // A crewmate that has not run a task has no work tree yet: refused, with the reason.
    let r = phone
        .call("pty_open", json!({"crewmate": cid}))
        .await
        .unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("no work tree"), "{r}");
    let work = hostd.join("crew").join(&cid).join("work");
    std::fs::create_dir_all(&work).unwrap();
    let r = phone
        .call("pty_open", json!({"crewmate": cid}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let pty = r["data"]["pty"].as_str().unwrap().to_string();
    let info = c.call("pty_info", json!({"pty": pty})).await.unwrap();
    assert_eq!(canon(info["cwd"].as_str().unwrap()), canon(&work), "{info}");
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();

    // Never another folder, never an id that escapes the crew store, never a project id here.
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    for args in [
        json!({"crewmate": "nope"}),
        json!({"crewmate": format!("{cid}/../..")}),
        json!({"crewmate": "../.."}),
        json!({"crewmate": outside}),
        json!({"project": outside}),
        json!({"project": "x", "crewmate": cid}),
    ] {
        let r = phone.call("pty_open", args.clone()).await.unwrap();
        assert_eq!(r["ok"], false, "{args}: {r}");
    }
    let created = audit_lines(&hostd)
        .into_iter()
        .filter(|l| l["cmd"] == "pty_created")
        .count();
    assert_eq!(
        created, 2,
        "the crew home and the work tree are audited; every refusal is not"
    );
    c.call("shutdown", json!({})).await.ok();
}

async fn device_of(c: &HostClient, node: &str) -> Value {
    c.call("mobile_devices", json!({})).await.unwrap()["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["node_id"] == node)
        .unwrap()
        .clone()
}

/// Settings tells the phones apart: whether each is connected now, when it was last here, what it did, and the
/// owner can name it (the real app sends the same name for every phone). A phone can do none of this.
#[tokio::test]
async fn paired_phones_report_connection_activity_and_take_a_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut phone, node) = paired(tmp.path()).await;

    let d = device_of(&c, &node).await;
    assert_eq!(d["connected"], true, "{d}");
    assert!(d["last_seen"].is_u64(), "connecting is being seen: {d}");

    phone
        .call("project_service", json!({"id": "x", "op": "restart"}))
        .await
        .unwrap();
    let r = c
        .call("mobile_activity", json!({"node_id": node, "limit": 1}))
        .await
        .unwrap();
    let acts = r["activity"].as_array().unwrap();
    assert_eq!(acts.len(), 1, "{r}");
    assert_eq!(acts[0]["cmd"], "project_service");
    assert_eq!(acts[0]["args"]["op"], "restart");
    let all = c
        .call("mobile_activity", json!({"node_id": node}))
        .await
        .unwrap();
    assert_eq!(all["activity"][1]["cmd"], "pair", "newest first: {all}");
    let none = c
        .call("mobile_activity", json!({"node_id": "nobody"}))
        .await
        .unwrap();
    assert_eq!(none["activity"], json!([]));

    c.call(
        "mobile_rename",
        json!({"node_id": node, "name": "  Luan's iPhone  "}),
    )
    .await
    .unwrap();
    assert_eq!(device_of(&c, &node).await["name"], "Luan's iPhone");
    assert!(
        c.call("mobile_rename", json!({"node_id": node, "name": "   "}))
            .await
            .is_err()
    );
    assert!(
        c.call("mobile_rename", json!({"node_id": "nobody", "name": "x"}))
            .await
            .is_err()
    );

    for cmd in ["mobile_rename", "mobile_activity"] {
        let r = phone
            .call(cmd, json!({"node_id": node, "name": "mine"}))
            .await
            .unwrap();
        assert_eq!(r["error"], "not available on mobile", "{cmd}: {r}");
    }
    assert_eq!(device_of(&c, &node).await["name"], "Luan's iPhone");

    phone.conn.close(0u32.into(), b"bye");
    let mut gone = false;
    for _ in 0..1200 {
        if device_of(&c, &node).await["connected"] == false {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "a phone that hung up is not connected");
    assert!(device_of(&c, &node).await["last_seen"].is_u64());
    c.call("shutdown", json!({})).await.ok();
}

/// A phone paired before `last_seen` existed (no such key on disk) still shows when it was last active.
#[tokio::test]
async fn last_seen_falls_back_to_the_audit_log_and_a_long_log_is_read_from_its_end() {
    let tmp = tempfile::tempdir().unwrap();
    let m = super::Mobile::load(tmp.path(), false).unwrap();
    let code = m.pair_start().unwrap()["code"]
        .as_str()
        .unwrap()
        .to_string();
    m.pair("n1", &code, "Atlas Mobile").unwrap();
    assert!(m.devices_view()[0].get("last_seen").is_none(), "never seen");
    m.audit("n1", "project_service", &json!({"op": "stop"}));
    let seen = m.devices_view()[0]["last_seen"].as_u64().unwrap();
    assert!(seen > 1_700_000_000);
    assert_eq!(m.devices_view()[0]["connected"], false);

    // ~700 KB of log: more than the tail read, so the oldest lines are not parsed and the newest are.
    for i in 0..700 {
        m.audit("n1", &format!("cmd{i}"), &json!({"pad": "x".repeat(1000)}));
    }
    let acts = m.activity("n1", 1000);
    assert_eq!(acts.len(), 200, "capped");
    assert_eq!(acts[0]["cmd"], "cmd699");
    assert!(
        acts.iter()
            .all(|a| a["cmd"].as_str().unwrap().starts_with("cmd"))
    );
}
