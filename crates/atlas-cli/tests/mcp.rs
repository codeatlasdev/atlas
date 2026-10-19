//! End-to-end tests for `atlas mcp` (stdio) and `atlas dev --headless` (control socket).
//! Everything drives the real binary. Children are owned by RAII guards that kill them on drop.

// The fixtures are sh scripts and Unix sockets.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

mod common;
use common::{free_port, libc_kill, wait_for};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
const RPC_TIMEOUT: Duration = Duration::from_secs(20);

// ───────────────────────── helpers ─────────────────────────

/// A minimal MCP client over a spawned `atlas mcp` process.
struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: i64,
    /// Lines received while waiting for another id (concurrent calls).
    pending: Vec<Value>,
}

impl McpClient {
    fn spawn(root: &Path) -> Self {
        Self::spawn_with(&["mcp", "--dir", root.to_str().unwrap()], None)
    }

    fn spawn_with(args: &[&str], cwd: Option<&Path>) -> Self {
        let mut cmd = Command::new(BIN);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(c) = cwd {
            cmd.current_dir(c);
        }
        let mut child = cmd.spawn().expect("spawn atlas mcp");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            child,
            stdin,
            rx,
            next_id: 1,
            pending: Vec::new(),
        }
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn send(&mut self, v: &Value) {
        self.send_raw(&v.to_string());
    }

    /// Next stdout line, which MUST be a single valid JSON value.
    fn recv_line(&mut self, timeout: Duration) -> Option<Value> {
        match self.rx.recv_timeout(timeout) {
            Ok(l) => {
                assert!(!l.contains('\n'));
                Some(serde_json::from_str(&l).unwrap_or_else(|e| {
                    panic!("stdout line is not JSON ({e}): {}", &l[..l.len().min(200)])
                }))
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => panic!("server closed stdout unexpectedly"),
        }
    }

    /// Read until a response with `id` arrives (others are stashed).
    fn recv_id(&mut self, id: &Value) -> Value {
        if let Some(i) = self.pending.iter().position(|v| &v["id"] == id) {
            return self.pending.remove(i);
        }
        let deadline = Instant::now() + RPC_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let v = self
                .recv_line(left)
                .unwrap_or_else(|| panic!("timed out waiting for response id {id}"));
            if &v["id"] == id {
                return v;
            }
            self.pending.push(v);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = json!(self.next_id);
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        self.recv_id(&id)
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc":"2.0","method":method}));
    }

    fn handshake(&mut self) -> Value {
        let r = self.request(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
        );
        self.notify("notifications/initialized");
        r
    }

    /// Returns (is_error, text).
    fn call(&mut self, tool: &str, args: Value) -> (bool, String) {
        let r = self.request("tools/call", json!({"name":tool,"arguments":args}));
        parse_tool(&r)
    }

    /// Assert nothing arrives on stdout for a short while.
    fn assert_silent(&mut self, window: Duration) {
        if let Some(v) = self.recv_line(window) {
            panic!("unexpected output: {v}");
        }
    }

    /// Close stdin and require a prompt clean exit.
    fn close_and_wait_exit(&mut self) -> i32 {
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return st.code().unwrap_or(-1);
            }
            assert!(Instant::now() < deadline, "atlas mcp did not exit on EOF");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse_tool(r: &Value) -> (bool, String) {
    assert!(r.get("error").is_none(), "protocol error: {r}");
    let res = &r["result"];
    (
        res["isError"].as_bool().expect("isError bool"),
        res["content"][0]["text"]
            .as_str()
            .expect("text content")
            .to_string(),
    )
}

/// `atlas dev --headless` in a project dir; killed (SIGTERM → SIGKILL) on drop.
struct Headless {
    child: Child,
    out_path: PathBuf,
    _out_dir: tempfile::TempDir,
}

impl Headless {
    fn spawn(root: &Path) -> Self {
        let out_dir = tempfile::TempDir::new().unwrap();
        let out_path = out_dir.path().join("stdout.txt");
        let out = std::fs::File::create(&out_path).unwrap();
        let child = Command::new(BIN)
            .args(["dev", "--headless", "--dir", root.to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .expect("spawn atlas dev --headless");
        Self {
            child,
            out_path,
            _out_dir: out_dir,
        }
    }

    /// Everything atlas printed to stdout so far (service + action output).
    fn stdout(&self) -> String {
        std::fs::read_to_string(&self.out_path).unwrap_or_default()
    }

    fn pid(&self) -> i32 {
        self.child.id() as i32
    }

    fn stop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        unsafe { libc_kill(self.pid(), 15) };
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Project {
    dir: tempfile::TempDir,
    web_port: u16,
    never_port: u16,
}

impl Project {
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn logs(&self) -> PathBuf {
        self.root().join(".logs")
    }
}

/// Static HTTP server for the `web` fixture, written to `web.py` in the project. NOT
/// `python3 -m http.server`: its `server_bind` calls `socket.getfqdn()` before `listen()`, and
/// on the macOS CI runners that reverse-DNS lookup stalls for tens of seconds, so the port
/// never opened inside the 30s window (every `atlas dev --headless` fixture stayed `starting`
/// with an empty `web.log` and note "waiting for :PORT"). Binding through
/// `TCPServer.server_bind` skips the lookup. Guarded by `fixture_web_server_does_not_resolve_names`.
const WEB_SERVER: &str = "\
import http.server, socketserver, sys

class Server(http.server.ThreadingHTTPServer):
    server_bind = socketserver.TCPServer.server_bind

http.server.test(
    HandlerClass=http.server.SimpleHTTPRequestHandler,
    ServerClass=Server,
    port=int(sys.argv[1]),
    bind='127.0.0.1',
)
";

/// A temp project: web (static server, port+health), ticker (port-less, prints incl. ERROR),
/// never (port that nothing listens on → never ready), plus actions.
fn project() -> Project {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("web.py"), WEB_SERVER).unwrap();
    let web_port = free_port();
    let never_port = free_port();
    let yaml = format!(
        r#"name: mcp-fixture
services:
  web:
    command: python3 web.py $PORT
    port: {web_port}
    health: http://127.0.0.1:{web_port}/
  ticker:
    command: "sh -c 'while true; do echo tick-line; echo ERROR fixture-boom; sleep 1; done'"
actions:
  - key: h
    label: Hello
    command: echo hello-from-action
  - key: p
    label: Pause web
    command: "sh -c 'echo pausing-now; sleep 2; echo pause-finished'"
    pause_services: [web]
  - key: x
    label: Wipe
    command: echo wiped-the-thing
    confirm: "Wipe all data?"
"#
    );
    std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
    Project {
        dir,
        web_port,
        never_port,
    }
}

/// Run `args` under python with `socket.getfqdn` stalled (what the macOS CI runners' reverse
/// DNS does) and report whether 127.0.0.1:`port` accepts a connection within `within`.
fn serves_with_stalled_dns(dir: &Path, args: &[&str], port: u16, within: Duration) -> bool {
    // `args[0]` is a stdlib module (`http.server`) or a script path in `dir`.
    let wrapper = dir.join("stall_dns.py");
    std::fs::write(
        &wrapper,
        "import runpy, socket, sys, time\n\
         socket.getfqdn = lambda *a, **k: time.sleep(120)\n\
         target = sys.argv[1]\n\
         sys.argv = sys.argv[1:]\n\
         if target.endswith('.py'):\n\
         \x20   runpy.run_path(target, run_name='__main__')\n\
         else:\n\
         \x20   runpy.run_module(target, run_name='__main__', alter_sys=True)\n",
    )
    .unwrap();
    let mut child = Command::new("python3")
        .current_dir(dir)
        .arg(&wrapper)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn python3");
    let deadline = Instant::now() + within;
    let mut up = false;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    up
}

/// Incident: every headless fixture timed out on the macOS runners because `python3 -m
/// http.server` resolves the host name (`socket.getfqdn`) before it listens. The fixture's
/// server must listen without any name lookup; stock `http.server` is the control that proves
/// the stall is real (it must NOT come up in the same time).
#[test]
fn fixture_web_server_does_not_resolve_names() {
    require_python!();
    let p = project();
    let port = free_port();
    assert!(
        !serves_with_stalled_dns(
            p.root(),
            &["http.server", &port.to_string(), "--bind", "127.0.0.1"],
            port,
            Duration::from_secs(3)
        ),
        "control: stock http.server should stall on getfqdn"
    );
    let port = free_port();
    assert!(
        serves_with_stalled_dns(
            p.root(),
            &["web.py", &port.to_string()],
            port,
            Duration::from_secs(5)
        ),
        "fixture web.py must listen even when name resolution hangs"
    );
}

/// Project that additionally has a service that never becomes ready.
fn project_with_never() -> Project {
    let p = project();
    let mut yaml = std::fs::read_to_string(p.root().join("atlas.yaml")).unwrap();
    yaml = yaml.replace(
        "actions:\n",
        &format!(
            "  never:\n    command: \"sh -c 'sleep 600'\"\n    port: {}\n\nactions:\n",
            p.never_port
        ),
    );
    std::fs::write(p.root().join("atlas.yaml"), yaml).unwrap();
    p
}

fn status(c: &mut McpClient) -> Value {
    let (err, text) = c.call("atlas_status", json!({}));
    assert!(!err, "{text}");
    serde_json::from_str(&text).unwrap()
}

fn service_state(st: &Value, name: &str) -> Option<String> {
    st["services"]
        .as_array()?
        .iter()
        .find(|s| s["name"] == name)
        .and_then(|s| s["state"].as_str().map(str::to_string))
}

/// Everything needed to tell why a headless fixture did not come up: what atlas printed, the
/// last `atlas_status`, and each service log. CI only shows the panic message.
fn startup_report(h: &Headless, p: &Project, last_status: &Value) -> String {
    let mut out = format!(
        "--- atlas dev stdout ---\n{}\n--- last atlas_status ---\n{last_status:#}\n--- .logs ---\n",
        h.stdout()
    );
    if let Ok(rd) = std::fs::read_dir(p.logs()) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "log") {
                let body = std::fs::read_to_string(&path).unwrap_or_default();
                out += &format!(
                    "[{}]\n{body}\n",
                    path.file_name().unwrap().to_string_lossy()
                );
            }
        }
    }
    out
}

/// Start headless and wait until `services` are all reported running by atlas_status.
fn start_running(p: &Project, services: &[&str]) -> (Headless, McpClient) {
    let h = Headless::spawn(p.root());
    let mut c = McpClient::spawn(p.root());
    c.handshake();
    let last = std::cell::RefCell::new(Value::Null);
    common::wait_for_with(
        "services running",
        Duration::from_secs(30),
        || {
            let st = status(&mut c);
            let ok = st["running"] == true
                && services
                    .iter()
                    .all(|s| service_state(&st, s).as_deref() == Some("running"));
            *last.borrow_mut() = st;
            ok.then_some(())
        },
        || startup_report(&h, p, &last.borrow()),
    );
    (h, c)
}

fn pids_file(p: &Project) -> String {
    std::fs::read_to_string(p.logs().join(".pids")).unwrap_or_default()
}

/// Item 4: action output is persisted and readable through `atlas_logs`, both by
/// `action:<label>` and by file name, and merged into the all-services tail.
#[test]
fn action_output_is_readable_via_atlas_logs() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web"]);
    let (err, t) = c.call("atlas_run_action", json!({"action": "Hello"}));
    assert!(!err, "{t}");
    let file = p.logs().join("action-hello.log");
    wait_for("action log file", Duration::from_secs(10), || {
        std::fs::read_to_string(&file)
            .ok()
            .filter(|s| s.contains("hello-from-action"))
    });
    for name in ["action:Hello", "action-hello"] {
        let (err, t) = c.call("atlas_logs", json!({"service": name}));
        assert!(!err, "{name}: {t}");
        assert!(t.contains("hello-from-action"), "{name}: {t}");
    }
    let (_, t) = c.call("atlas_logs_search", json!({"query": "hello-from"}));
    assert!(t.contains("action-hello"), "{t}");
    let (_, t) = c.call("atlas_logs", json!({"lines": 200}));
    assert!(t.contains("hello-from-action"), "merged tail: {t}");
    // atlas_status carries the new per-service facts.
    let st = status(&mut c);
    let web = st["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "web")
        .unwrap();
    assert!(web["uptime_s"].as_u64().is_some(), "{web}");
    assert!(web["exit_code"].is_null(), "{web}");
    assert_eq!(web["restarts"], 0);
}

// ───────────────────────── protocol ─────────────────────────

#[test]
fn initialize_negotiates_every_supported_version() {
    let d = tempfile::TempDir::new().unwrap();
    for v in ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"] {
        let mut c = McpClient::spawn(d.path());
        let r = c.request("initialize", json!({"protocolVersion": v}));
        assert_eq!(r["result"]["protocolVersion"], v);
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "atlas");
        assert!(
            r["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("atlas_status")
        );
    }
    let mut c = McpClient::spawn(d.path());
    let r = c.request("initialize", json!({"protocolVersion": "2099-01-01"}));
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    let r = c.request("initialize", json!({}));
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn ids_are_echoed_exactly_and_notifications_are_silent() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    for id in [json!("abc"), json!(42), json!(0), json!(-7), json!("")] {
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"ping"}));
        let r = c.recv_id(&id);
        assert_eq!(r["id"], id);
        assert_eq!(r["result"], json!({}));
    }
    // Numeric-looking string stays a string.
    c.send(&json!({"jsonrpc":"2.0","id":"7","method":"ping"}));
    let r = c.recv_id(&json!("7"));
    assert!(r["id"].is_string());

    // Notifications (known, unknown, with params): never answered.
    c.notify("notifications/initialized");
    c.notify("notifications/cancelled");
    c.notify("totally/unknown");
    c.send(&json!({"jsonrpc":"2.0","method":"ping"})); // ping without id = notification
    c.assert_silent(Duration::from_millis(400));
    // And the server is still alive.
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn json_rpc_errors_use_correct_codes() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());

    c.send_raw("{not json");
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(r["id"], Value::Null);

    // Valid JSON but not a request object.
    c.send_raw("42");
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], Value::Null);

    c.send_raw(r#"{"jsonrpc":"2.0","id":1}"#);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], 1);

    // Bad id type.
    c.send_raw(r#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], Value::Null);

    // Wrong jsonrpc version.
    c.send_raw(r#"{"jsonrpc":"1.0","id":2,"method":"ping"}"#);
    let r = c.recv_id(&json!(2));
    assert_eq!(r["error"]["code"], -32600);

    // Unknown method.
    let r = c.request("sampling/createMessage", json!({}));
    assert_eq!(r["error"]["code"], -32601);

    // tools/call: missing name, bad arguments type, unknown tool → -32602.
    for (id, params) in [
        (10, json!({})),
        (11, json!({"name": 5})),
        (12, json!({"name":"atlas_logs","arguments":[1]})),
        (13, json!({"name":"does_not_exist"})),
    ] {
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":params}));
        let r = c.recv_id(&json!(id));
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }

    // Bad tool *arguments* are tool errors (isError), not protocol errors.
    let (err, text) = c.call("atlas_logs", json!({"lines": "many"}));
    assert!(err, "{text}");
    let (err, text) = c.call("atlas_logs", json!({"level": "loud"}));
    assert!(err, "{text}");
    let (err, text) = c.call("atlas_logs_search", json!({}));
    assert!(err && text.contains("query"), "{text}");
    let (err, text) = c.call("atlas_logs_search", json!({"query": "/(/"}));
    assert!(err && text.contains("regex"), "{text}");
    let (err, _) = c.call("atlas_logs", json!({"service": "../etc/passwd"}));
    assert!(err);

    // Invalid UTF-8 on the wire is a parse error, not a crash.
    c.stdin.as_mut().unwrap().write_all(b"\xff\xfe\n").unwrap();
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn batching_only_with_2025_03_26() {
    let d = tempfile::TempDir::new().unwrap();
    let batch = json!([
        {"jsonrpc":"2.0","id":"a","method":"ping"},
        {"jsonrpc":"2.0","method":"notifications/initialized"},
        {"jsonrpc":"2.0","id":2,"method":"tools/list"}
    ]);

    // Before initialize and under newer/older versions: rejected.
    for v in [
        None,
        Some("2025-11-25"),
        Some("2025-06-18"),
        Some("2024-11-05"),
    ] {
        let mut c = McpClient::spawn(d.path());
        if let Some(v) = v {
            c.request("initialize", json!({"protocolVersion": v}));
        }
        c.send(&batch);
        let r = c.recv_line(RPC_TIMEOUT).unwrap();
        assert_eq!(r["error"]["code"], -32600, "version {v:?}: {r}");
        assert_eq!(r["id"], Value::Null);
        assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    }

    // 2025-03-26: batch answered per message, notifications dropped.
    let mut c = McpClient::spawn(d.path());
    c.request("initialize", json!({"protocolVersion": "2025-03-26"}));
    c.send(&batch);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    let arr = r.as_array().expect("batch response is an array");
    assert_eq!(arr.len(), 2);
    assert!(
        arr.iter()
            .any(|m| m["id"] == "a" && m["result"] == json!({}))
    );
    assert!(
        arr.iter()
            .any(|m| m["id"] == 2 && m["result"]["tools"].is_array())
    );
    // All-notification batch → no response at all; empty batch → invalid request.
    c.send(&json!([{"jsonrpc":"2.0","method":"notifications/initialized"}]));
    c.assert_silent(Duration::from_millis(300));
    c.send(&json!([]));
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
}

#[test]
fn ping_and_tools_work_before_initialize() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    let r = c.request("tools/list", json!({}));
    assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 9);
    let (err, text) = c.call("atlas_status", json!({}));
    assert!(!err, "{text}");
    // Still handshakes fine afterwards, and ping works after too.
    c.handshake();
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn resources_and_prompts_over_stdio() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let r = c.request("resources/list", json!({}));
    assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 2);
    let r = c.request(
        "prompts/get",
        json!({"name": "diagnose", "arguments": {"service": "api"}}),
    );
    let text = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("\"api\""), "{text}");
}

#[test]
fn eof_exits_zero_promptly_and_stdout_is_json_only() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    c.request("tools/list", json!({}));
    assert_eq!(c.close_and_wait_exit(), 0);
    // No stray non-JSON output was produced (recv_line would have panicked); drain.
    while let Ok(l) = c.rx.try_recv() {
        serde_json::from_str::<Value>(&l).expect("only JSON on stdout");
    }

    // EOF with a request still in flight: the answer is delivered before exit.
    let mut c = McpClient::spawn(d.path());
    c.send(&json!({"jsonrpc":"2.0","id":9,"method":"ping"}));
    assert_eq!(c.close_and_wait_exit(), 0);
    let line = c.rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], 9);
}

#[test]
fn tools_list_schemas_are_valid_and_described() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let r = c.request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "atlas_status",
        "atlas_logs",
        "atlas_logs_search",
        "atlas_config",
        "atlas_restart_service",
        "atlas_stop_service",
        "atlas_start_service",
        "atlas_run_action",
        "atlas_wait_ready",
    ] {
        assert!(names.contains(&want), "missing tool {want}");
    }
    let name_ok = |n: &str| {
        (1..=128).contains(&n.len())
            && n.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    };
    let required: &[(&str, &[&str])] = &[
        ("atlas_logs_search", &["query"]),
        ("atlas_restart_service", &["service"]),
        ("atlas_stop_service", &["service"]),
        ("atlas_start_service", &["service"]),
        ("atlas_run_action", &["action"]),
    ];
    for t in tools {
        let n = t["name"].as_str().unwrap();
        assert!(name_ok(n), "bad tool name {n}");
        assert!(
            t["description"].as_str().unwrap().len() > 40,
            "{n}: description too thin"
        );
        let s = &t["inputSchema"];
        assert_eq!(s["type"], "object", "{n}");
        let props = s["properties"].as_object().expect("properties object");
        for (k, p) in props {
            assert!(p["type"].is_string(), "{n}.{k} needs a type");
            assert!(
                p["description"].as_str().is_some_and(|d| !d.is_empty()) || p["enum"].is_array(),
                "{n}.{k} needs a description"
            );
        }
        if let Some(req) = s.get("required") {
            for r in req.as_array().unwrap() {
                assert!(
                    props.contains_key(r.as_str().unwrap()),
                    "{n}: required {r} undeclared"
                );
            }
        }
        if let Some((_, want)) = required.iter().find(|(k, _)| *k == n) {
            let got: Vec<&str> = s["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{n}: required missing"))
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(&got, want, "{n}");
        }
    }
}

#[test]
fn default_dir_is_project_root_found_from_cwd() {
    let p = project();
    let nested = p.root().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let mut c = McpClient::spawn_with(&["mcp"], Some(&nested));
    c.handshake();
    let (err, text) = c.call("atlas_config", json!({}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["project"], "mcp-fixture");
}

#[test]
fn large_logs_response_is_one_valid_json_line() {
    let d = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(d.path().join(".logs")).unwrap();
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&format!(
            "12:00:{:02} [stdout] line {i} {}\n",
            i % 60,
            "x".repeat(150)
        ));
    }
    std::fs::write(d.path().join(".logs/big.log"), body).unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let (err, text) = c.call("atlas_logs", json!({"lines": 2000, "service": "big"}));
    assert!(!err);
    assert_eq!(text.lines().count(), 2000);
    assert!(text.len() > 300_000);
    assert!(text.contains("line 2999"));
    assert!(!text.contains("line 999 "), "must be the tail");
}

// ───────────────────────── not running ─────────────────────────

#[test]
fn not_running_read_tools_work_and_control_tools_error() {
    let p = project();
    std::fs::create_dir_all(p.logs()).unwrap();
    std::fs::write(
        p.logs().join("api.log"),
        "10:00:01 [stdout] boot ok\n10:00:05 [stderr] ERROR db down\n10:00:06 [stdout] WARN slow query\n10:00:09 [stdout] ready\n",
    )
    .unwrap();
    std::fs::write(
        p.logs().join("api.prev.log"),
        "09:00:00 [stdout] old ERROR boom\n",
    )
    .unwrap();
    std::fs::write(p.logs().join("web.log"), "10:00:03 [stdout] vite up\n").unwrap();

    let mut c = McpClient::spawn(p.root());
    c.handshake();

    let st = status(&mut c);
    assert_eq!(st["running"], false);
    assert!(st["hint"].as_str().unwrap().contains("not running"));

    let (err, text) = c.call("atlas_logs", json!({}));
    assert!(!err);
    assert!(
        text.contains("[api:2]") && text.contains("[web:1]"),
        "{text}"
    );
    assert!(
        !text.contains("old ERROR"),
        "prev log must not leak into tail"
    );
    let (_, text) = c.call("atlas_logs", json!({"level": "error"}));
    assert!(
        text.contains("db down") && !text.contains("boot ok"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs", json!({"level": "warn"}));
    assert!(
        text.contains("db down") && text.contains("slow query"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs", json!({"service": "api", "lines": 1}));
    assert_eq!(text.lines().count(), 1);
    assert!(text.contains("ready"));
    let (err, text) = c.call("atlas_logs", json!({"service": "nope"}));
    assert!(
        err && text.contains("api") && text.contains("web"),
        "{text}"
    );

    let (_, text) = c.call("atlas_logs_search", json!({"query": "error"}));
    assert!(
        text.contains("db down") && !text.contains("old ERROR"),
        "{text}"
    );
    let (_, text) = c.call(
        "atlas_logs_search",
        json!({"query": "/db (down|up)/", "service": "api"}),
    );
    assert!(text.contains("db down"), "{text}");
    let (_, text) = c.call(
        "atlas_logs_search",
        json!({"query": "boom", "include_previous": true}),
    );
    assert!(
        text.contains("old ERROR boom") && text.contains("prev run"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs_search", json!({"query": "boom"}));
    assert_eq!(text, "(no matches)");
    let (err, text) = c.call(
        "atlas_logs_search",
        json!({"query": "x", "service": "nope"}),
    );
    assert!(err && text.contains("api"), "{text}");

    let (err, text) = c.call("atlas_config", json!({}));
    assert!(!err, "{text}");
    let cfg: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(cfg["project"], "mcp-fixture");
    let svcs: Vec<&str> = cfg["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(svcs, ["ticker", "web"]);
    let wipe = cfg["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "Wipe")
        .unwrap();
    assert_eq!(wipe["confirm"], "Wipe all data?");

    for (tool, args) in [
        ("atlas_restart_service", json!({"service":"web"})),
        ("atlas_stop_service", json!({"service":"web"})),
        ("atlas_start_service", json!({"service":"web"})),
        ("atlas_run_action", json!({"action":"Hello"})),
    ] {
        let (err, text) = c.call(tool, args);
        assert!(err, "{tool}");
        assert_eq!(text, "atlas dev is not running; start it in a terminal");
    }
    let (err, text) = c.call("atlas_wait_ready", json!({"timeout_s": 1}));
    assert!(err && text.contains("not running"), "{text}");
}

#[test]
fn config_tool_reports_missing_and_invalid_yaml_as_tool_error() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let (err, _) = c.call("atlas_config", json!({}));
    assert!(err);
    std::fs::write(d.path().join("atlas.yaml"), "name: [unclosed").unwrap();
    let (err, _) = c.call("atlas_config", json!({}));
    assert!(err);
}

// ───────────────────────── running ─────────────────────────

#[test]
fn running_full_flow() {
    require_python!();
    let p = project_with_never();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);

    // status
    let st = status(&mut c);
    assert_eq!(st["running"], true);
    assert!(st["pid"].as_i64().unwrap() > 0);
    assert_eq!(service_state(&st, "web").as_deref(), Some("running"));
    assert_eq!(service_state(&st, "ticker").as_deref(), Some("running"));
    let web = st["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "web")
        .unwrap();
    assert_eq!(web["port"].as_u64(), Some(p.web_port as u64));

    // logs: wait for the ticker to have written lines.
    wait_for("ticker logs", Duration::from_secs(15), || {
        let (_, t) = c.call("atlas_logs", json!({"service": "ticker"}));
        t.contains("fixture-boom").then_some(())
    });
    let (_, t) = c.call("atlas_logs", json!({"service": "ticker", "lines": 1}));
    assert_eq!(t.lines().count(), 1);
    let (_, t) = c.call("atlas_logs", json!({"service": "ticker", "level": "error"}));
    assert!(t.lines().all(|l| l.contains("fixture-boom")), "{t}");
    let (_, t) = c.call("atlas_logs", json!({"since_minutes": 5}));
    assert!(t.contains("[ticker:") && t.contains("[web:"), "{t}");
    let (_, t) = c.call(
        "atlas_logs",
        json!({"since_minutes": 0, "service": "ticker"}),
    );
    assert!(t.lines().count() <= 3, "since 0 should be near-empty: {t}");
    let (_, t) = c.call("atlas_logs", json!({}));
    assert!(t.contains("[ticker:") && t.contains("[web:"), "merged: {t}");

    // search: substring, regex, include_previous (no prev yet → still ok)
    let (_, t) = c.call("atlas_logs_search", json!({"query": "FIXTURE-BOOM"}));
    assert!(t.contains("fixture-boom"), "{t}");
    wait_for("two ticks", Duration::from_secs(15), || {
        let (_, t) = c.call("atlas_logs", json!({"service": "ticker"}));
        (t.matches("tick-line").count() >= 2).then_some(())
    });
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "/tick-(line|none)/", "service": "ticker", "limit": 2}),
    );
    assert_eq!(t.lines().count(), 2, "{t}");
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "fixture-boom", "include_previous": true, "service": "ticker"}),
    );
    assert!(t.contains("fixture-boom"));

    // restart_service changes the pid and the service returns to running.
    let pids_before = pids_file(&p);
    assert!(!pids_before.trim().is_empty(), "no .pids while running");
    let (err, t) = c.call("atlas_restart_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    assert_eq!(serde_json::from_str::<Value>(&t).unwrap()["queued"], true);
    wait_for("web pid to change", Duration::from_secs(20), || {
        (pids_file(&p) != pids_before).then_some(())
    });
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "web", "timeout_s": 30}),
    );
    assert!(!err, "{t}");
    assert!(
        t.contains("ready after") && t.contains("web=running"),
        "{t}"
    );
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", p.web_port)).is_ok(),
        "web must listen again after restart"
    );

    // stop → stopped/disabled, start → running
    let (err, t) = c.call("atlas_stop_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    wait_for("web stopped", Duration::from_secs(20), || {
        let st = status(&mut c);
        matches!(
            service_state(&st, "web").as_deref(),
            Some("stopped" | "disabled")
        )
        .then_some(())
    });
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", p.web_port)).is_err(),
        "web port must be closed after stop"
    );
    let (err, t) = c.call("atlas_start_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    wait_for("web running again", Duration::from_secs(20), || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("running")).then_some(())
    });

    // unknown service lists known names (for every control tool).
    for tool in [
        "atlas_restart_service",
        "atlas_stop_service",
        "atlas_start_service",
    ] {
        let (err, t) = c.call(tool, json!({"service": "ghost"}));
        assert!(err, "{tool}");
        assert!(
            t.contains("unknown service 'ghost'") && t.contains("web") && t.contains("ticker"),
            "{t}"
        );
        let (err, t) = c.call(tool, json!({}));
        assert!(err && t.contains("service"), "{t}");
    }

    // actions
    let (err, t) = c.call("atlas_run_action", json!({"action": "Wipe"}));
    assert!(err, "must refuse without confirmed");
    assert!(
        t.contains("Wipe all data?") && t.contains("confirmed"),
        "{t}"
    );
    let (err, _) = c.call(
        "atlas_run_action",
        json!({"action": "x", "confirmed": false}),
    );
    assert!(err);
    let (err, t) = c.call(
        "atlas_run_action",
        json!({"action": "Wipe", "confirmed": true}),
    );
    assert!(!err, "{t}");
    let (err, t) = c.call("atlas_run_action", json!({"action": "h"}));
    assert!(!err, "{t}");
    assert_eq!(
        serde_json::from_str::<Value>(&t).unwrap()["action"],
        "Hello"
    );
    let (err, t) = c.call("atlas_run_action", json!({"action": "hello"})); // label, case-insens
    assert!(!err || t.contains("already running"), "{t}");
    let (err, t) = c.call("atlas_run_action", json!({"action": "nope"}));
    assert!(err);
    assert!(
        t.contains("unknown action") && t.contains("Hello") && t.contains("Wipe"),
        "{t}"
    );
    let (err, t) = c.call("atlas_run_action", json!({}));
    assert!(err && t.contains("action"), "{t}");

    // Never-ready service: wait_ready times out correctly (short timeout).
    let started = Instant::now();
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "never", "timeout_s": 2}),
    );
    assert!(err, "{t}");
    assert!(
        t.contains("timed out after 2s") && t.contains("never="),
        "{t}"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(2) && started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "ghost", "timeout_s": 1}),
    );
    assert!(err && t.contains("unknown service 'ghost'"), "{t}");
    let (err, _) = c.call("atlas_wait_ready", json!({"timeout_s": "soon"}));
    assert!(err);
}

#[test]
fn action_output_is_observable_and_paused_services_resume() {
    require_python!();
    let p = project();
    let (h, mut c) = start_running(&p, &["web", "ticker"]);

    // Plain action: output reaches atlas's stdout, and the in-flight guard is
    // released afterwards so the same action can run again.
    let (err, t) = c.call("atlas_run_action", json!({"action": "Hello"}));
    assert!(!err, "{t}");
    wait_for("action output", Duration::from_secs(10), || {
        h.stdout().contains("hello-from-action").then_some(())
    });
    wait_for("Hello runnable again", Duration::from_secs(10), || {
        let (err, _) = c.call("atlas_run_action", json!({"action": "Hello"}));
        (!err).then_some(())
    });

    // Confirmed action runs after confirmed=true only.
    let (err, _) = c.call("atlas_run_action", json!({"action": "Wipe"}));
    assert!(err);
    assert!(!h.stdout().contains("wiped-the-thing"));
    let (err, t) = c.call(
        "atlas_run_action",
        json!({"action": "Wipe", "confirmed": true}),
    );
    assert!(!err, "{t}");
    wait_for("wipe output", Duration::from_secs(10), || {
        h.stdout().contains("wiped-the-thing").then_some(())
    });

    // pause_services: web is suspended while the action runs, restarted after.
    let (err, t) = c.call("atlas_run_action", json!({"action": "p"}));
    assert!(!err, "{t}");
    // The same action cannot start twice concurrently.
    let (err, t) = c.call("atlas_run_action", json!({"action": "Pause web"}));
    assert!(err && t.contains("already running"), "{t}");
    wait_for("web suspended", Duration::from_secs(10), || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("suspended")).then_some(())
    });
    wait_for("pause action finished", Duration::from_secs(15), || {
        h.stdout().contains("pause-finished").then_some(())
    });
    wait_for("web resumed", Duration::from_secs(30), || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("running")).then_some(())
    });
    let (err, t) = c.call("atlas_wait_ready", json!({"timeout_s": 30}));
    assert!(!err, "{t}");
}

#[test]
fn concurrent_calls_are_all_answered_with_correct_ids() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);

    // 10 in flight: mix of reads, a control call and a (short) blocking wait_ready.
    let mut ids = Vec::new();
    for i in 0..10i64 {
        let id = json!(format!("req-{i}"));
        let (tool, args) = match i % 5 {
            0 => ("atlas_status", json!({})),
            1 => ("atlas_logs", json!({"service": "ticker", "lines": 5})),
            2 => ("atlas_config", json!({})),
            3 => (
                "atlas_wait_ready",
                json!({"service": "web", "timeout_s": 10}),
            ),
            _ => ("atlas_start_service", json!({"service": "ticker"})),
        };
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}}));
        ids.push((id, tool));
    }
    // A ping sent behind a blocking wait_ready must not be stuck behind it.
    c.send(&json!({"jsonrpc":"2.0","id":"ping-1","method":"ping"}));
    let r = c.recv_id(&json!("ping-1"));
    assert_eq!(r["result"], json!({}));

    for (id, tool) in ids.into_iter().rev() {
        let r = c.recv_id(&id);
        assert_eq!(r["id"], id);
        let (err, text) = parse_tool(&r);
        assert!(!err, "{tool}: {text}");
    }
}

// ───────────────────────── socket ─────────────────────────

#[test]
fn stale_socket_is_replaced_perms_0600_and_malformed_lines_are_survived() {
    require_python!();
    let p = project();
    std::fs::create_dir_all(p.logs()).unwrap();
    let sock = p.logs().join("atlas.sock");
    // A socket file left by a crashed run: bound once, listener dropped, file stays.
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    // A process forked by a parallel test between bind and drop holds the listener
    // fd until its exec (CLOEXEC), so the refusal can take a moment.
    wait_for("stale socket refusing", Duration::from_secs(2), || {
        std::os::unix::net::UnixStream::connect(&sock)
            .is_err()
            .then_some(())
    });

    let h = Headless::spawn(p.root());
    wait_for("socket serving", Duration::from_secs(20), || {
        std::os::unix::net::UnixStream::connect(&sock).ok()
    });
    let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket mode is {mode:o}");

    let roundtrip = |line: &[u8]| -> Value {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s.write_all(line).unwrap();
        s.write_all(b"\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("bad reply {reply:?}: {e}"))
    };
    for bad in [
        &b"garbage"[..],
        b"{}",
        b"[1,2,3]",
        b"{\"cmd\":5}",
        b"{\"cmd\":\"nope\"}",
        b"{\"cmd\":\"restart_service\"}",
        b"\xff\xfe\xfd",
        b"{\"cmd\":\"restart_service\",\"args\":{\"service\":\"ghost\"}}",
    ] {
        let r = roundtrip(bad);
        assert_eq!(r["ok"], false, "{:?} → {r}", String::from_utf8_lossy(bad));
        assert!(r["error"].as_str().is_some_and(|e| !e.is_empty()));
    }
    // Oversized line is rejected, not buffered forever / crashed on.
    {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        // The server answers and closes without draining; a late write may hit EPIPE.
        let _ = s.write_all(&vec![b'a'; 200 * 1024]);
        let _ = s.write_all(b"\n");
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply).unwrap();
        let r: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("too large"), "{r}");
    }
    // Connection that hangs up mid-line must not hurt either.
    {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.write_all(b"{\"cmd\":\"restart_serv").unwrap();
    }
    // Atlas is still alive and serving valid requests.
    let r = roundtrip(br#"{"cmd":"start_service","args":{"service":"ticker"}}"#);
    assert_eq!(r["ok"], true, "{r}");
    assert!(
        unsafe { libc_kill(h.pid(), 0) } == 0,
        "atlas must survive malformed control input"
    );

    // Clean shutdown removes the socket.
    let mut h = h;
    h.stop();
    wait_for("socket removed", Duration::from_secs(5), || {
        (!sock.exists()).then_some(())
    });
}

#[test]
fn second_headless_instance_refuses_and_leaves_first_working() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);
    let out = Command::new(BIN)
        .args(["dev", "--headless", "--dir", p.root().to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("already running"));
    // The first instance still owns its socket and services.
    let st = status(&mut c);
    assert_eq!(service_state(&st, "web").as_deref(), Some("running"));
    let (err, t) = c.call("atlas_start_service", json!({"service": "web"}));
    assert!(!err, "{t}");
}
