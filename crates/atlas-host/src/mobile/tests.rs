//! Real daemon (`serve_with` on a temp dir) and a real iroh client on localhost, relays off.

use std::path::Path;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::HostClient;

async fn daemon(dir: &Path) -> HostClient {
    // Safety: set once before any endpoint binds; every test sets the same value.
    unsafe { std::env::set_var("ATLAS_MOBILE_RELAY", "0") };
    let d = dir.to_path_buf();
    tokio::spawn(async move { crate::server::serve(&d).await });
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

impl Phone {
    async fn dial(info: &Value) -> Self {
        let ep = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .bind()
            .await
            .unwrap();
        let id: PublicKey = info["node_id"].as_str().unwrap().parse().unwrap();
        let mut addr = EndpointAddr::new(id);
        for a in info["addrs"].as_array().unwrap() {
            addr = addr.with_ip_addr(a.as_str().unwrap().parse().unwrap());
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

fn hook(dir: &Path, payload: Value) {
    use std::io::{Read, Write};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = json!({"pane_key": "p1", "payload": payload}).to_string();
    let mut s =
        std::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT"))).unwrap();
    write!(
        s,
        "POST /hook/omp HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    )
    .unwrap();
    let _ = s.read_to_end(&mut Vec::new());
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
