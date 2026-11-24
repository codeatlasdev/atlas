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

use atlas_core::scaling::HANG;
use serde_json::{Value, json};

#[path = "../common/mod.rs"]
mod common;
use common::{free_port, libc_kill, wait_for};

mod agents;
mod not_running;
mod protocol;
mod running;
mod socket;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
const RPC_TIMEOUT: Duration = HANG;

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
        Self::spawn_with_env(args, cwd, &[])
    }

    /// [`McpClient::spawn_with`] with extra environment (an agent host's session variables).
    fn spawn_with_env(args: &[&str], cwd: Option<&Path>, envs: &[(&str, &str)]) -> Self {
        let mut cmd = Command::new(BIN);
        // The text of these tests is English whatever language the machine is set to.
        cmd.env("ATLAS_LANG", "en");
        cmd.args(args)
            .envs(envs.iter().copied())
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
        self.handshake_as("t", "0")
    }

    /// `initialize` as the MCP client `name`/`version` (what the audit log records).
    fn handshake_as(&mut self, name: &str, version: &str) -> Value {
        let r = self.request(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":name,"version":version}}),
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
        let deadline = Instant::now() + HANG;
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
            .env("ATLAS_LANG", "en")
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
        let deadline = Instant::now() + HANG;
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
    command: "sh -c 'echo ERROR fixture-boom; while true; do echo tick-line; sleep 1; done'"
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
    start_running_as(p, services, "t", "0")
}

/// [`start_running`] with the MCP client identifying as `name`/`version` (the audit log's `client`).
fn start_running_as(
    p: &Project,
    services: &[&str],
    name: &str,
    version: &str,
) -> (Headless, McpClient) {
    let h = Headless::spawn(p.root());
    let mut c = McpClient::spawn(p.root());
    c.handshake_as(name, version);
    let last = std::cell::RefCell::new(Value::Null);
    common::wait_for_with(
        "services running",
        HANG,
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
