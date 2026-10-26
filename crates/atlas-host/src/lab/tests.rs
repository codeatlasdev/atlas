//! A real MAIN daemon and a real LAB daemon (both `serve_with` on temp dirs) plus a real iroh phone
//! paired with the lab. Nothing here mocks hostd: the lab follows the main one over its unix socket.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::HostClient;
use crate::server::ServeOpts;

/// A plain hostd in `dir`, relays off, until it answers.
async fn main_daemon(dir: &Path) -> HostClient {
    let d = dir.to_path_buf();
    let opts = ServeOpts {
        no_mobile_relay: true,
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    connect(dir).await
}

/// The lab hostd in `dir`, following the main daemon at `main`.
async fn lab_daemon(dir: &Path, main: &Path) -> HostClient {
    let (d, m) = (dir.to_path_buf(), main.to_path_buf());
    let opts = ServeOpts {
        no_mobile_relay: true,
        lab: Some(m),
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    connect(dir).await
}

async fn connect(dir: &Path) -> HostClient {
    for _ in 0..100 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd at {} did not start", dir.display());
}

/// Poll `f` until it returns `Some`, for up to 10 s.
async fn until<T, F: AsyncFnMut() -> Option<T>>(what: &str, mut f: F) -> T {
    for _ in 0..200 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

/// POST one lifecycle event for `pane` to the hook server of the daemon in `dir`, the way
/// `atlas hook` does.
fn hook(dir: &Path, pane: &str, payload: Value) {
    use std::io::{Read, Write};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = json!({"pane_key": pane, "payload": payload}).to_string();
    let mut s =
        std::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT"))).unwrap();
    write!(
        s,
        "POST /hook/omp HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    )
    .unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
}

/// Every file under `dir` (relative paths, sockets and dirs included), so a test can prove
/// that something did not write there.
fn tree(dir: &Path) -> BTreeSet<PathBuf> {
    fn walk(root: &Path, at: &Path, out: &mut BTreeSet<PathBuf>) {
        for e in std::fs::read_dir(at).into_iter().flatten().flatten() {
            let p = e.path();
            out.insert(p.strip_prefix(root).unwrap().to_path_buf());
            if p.is_dir() {
                walk(root, &p, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

/// The scratch layout of a lab test: `<tmp>/main` and `<tmp>/lab` side by side, like
/// `~/.atlas` and `~/.atlas/lab` (the lab one is NOT inside main here, so a stray write into
/// main shows up as a changed tree).
struct Rig {
    tmp: tempfile::TempDir,
    main: HostClient,
    lab: HostClient,
}

impl Rig {
    async fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let main = main_daemon(&tmp.path().join("main")).await;
        let lab = lab_daemon(&tmp.path().join("lab"), &tmp.path().join("main")).await;
        Self { tmp, main, lab }
    }
    fn main_dir(&self) -> PathBuf {
        self.tmp.path().join("main")
    }
}

/// Guards the whole point of the lab: an agent whose hook went to the MAIN daemon (the one the
/// user's omp/Claude talk to) is visible to a phone paired with the LAB, with its state.
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_sees_an_agent_hooked_into_main_and_follows_its_changes() {
    let r = Rig::new().await;
    // Hooked BEFORE the lab asks again: seeded from agent_status_all ...
    hook(&r.main_dir(), "p1", json!({"event": "agent_start"}));
    let all = until("the lab to list p1", async || {
        let v = r.lab.call("agent_status_all", json!({})).await.ok()?;
        v["panes"]["p1"].is_object().then_some(v)
    })
    .await;
    assert_eq!(all["panes"]["p1"]["state"], "working", "{all}");

    // ... and changes AFTER it arrive as agent_status pushes.
    hook(
        &r.main_dir(),
        "p1",
        json!({"event": "agent_end", "text": "done!"}),
    );
    let st = until("the lab to see p1 finish", async || {
        let v = r.lab.call("agent_status_all", json!({})).await.ok()?;
        (v["panes"]["p1"]["state"] == "idle" || v["panes"]["p1"]["state"] == "done")
            .then(|| v["panes"]["p1"].clone())
    })
    .await;
    assert_eq!(st["last_message"], "done!");
}

/// A lab started after the agents already exist must still list them (seeded, not only followed).
#[tokio::test(flavor = "multi_thread")]
async fn a_lab_started_later_is_seeded_with_the_existing_agents() {
    let tmp = tempfile::tempdir().unwrap();
    let main_dir = tmp.path().join("main");
    let _main = main_daemon(&main_dir).await;
    let until_hook = until("the main hook endpoint", async || {
        std::fs::read_to_string(crate::paths::hook_endpoint(&main_dir))
            .ok()
            .filter(|e| e.contains("PORT"))
    })
    .await;
    drop(until_hook);
    hook(&main_dir, "old", json!({"event": "agent_start"}));
    let lab = lab_daemon(&tmp.path().join("lab"), &main_dir).await;
    let v = lab.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(v["panes"]["old"]["state"], "working", "{v}");
}

/// `agent_tree` / `agent_history` are answered by the LAB's own code, over the session file that
/// the agent's status names (the same file the main daemon would read).
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_builds_agent_tree_and_history_from_the_session_file() {
    let r = Rig::new().await;
    let file = r.tmp.path().join("s.jsonl");
    std::fs::write(
        &file,
        [
            json!({"type": "title", "title": "Lab session"}).to_string(),
            json!({"type": "message", "message": {"role": "user", "content": "hello lab"}})
                .to_string(),
        ]
        .join("\n"),
    )
    .unwrap();
    hook(
        &r.main_dir(),
        "p1",
        json!({"event": "agent_start", "session_file": file}),
    );
    let tree = until("agent_tree through the lab", async || {
        r.lab.call("agent_tree", json!({"pane": "p1"})).await.ok()
    })
    .await;
    assert!(tree.is_object(), "{tree}");
    let hist = r
        .lab
        .call("agent_history", json!({"pane": "p1"}))
        .await
        .unwrap();
    assert!(hist.to_string().contains("hello lab"), "{hist}");
}

/// The lab never owns a terminal: it lists MAIN's, and a prompt sent through it is typed into
/// MAIN's pty (the agent must be idle/done, the same rule as on main).
#[tokio::test(flavor = "multi_thread")]
async fn agent_send_through_the_lab_reaches_the_main_pty() {
    let r = Rig::new().await;
    let pty = r
        .main
        .call(
            "pty_create",
            json!({"cwd": r.tmp.path(), "argv": ["cat"], "scope": {"pane_key": "p1"}}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    until("the main hook endpoint", async || {
        std::fs::read_to_string(crate::paths::hook_endpoint(&r.main_dir()))
            .ok()
            .filter(|e| e.contains("PORT"))
    })
    .await;
    hook(&r.main_dir(), "p1", json!({"event": "agent_end"}));
    until("the lab to know p1", async || {
        let v = r.lab.call("agent_status_all", json!({})).await.ok()?;
        v["panes"]["p1"].is_object().then_some(())
    })
    .await;

    // pty_list through the lab is main's list, not an empty local one.
    let list = r.lab.call("pty_list", json!({})).await.unwrap();
    assert!(list.to_string().contains(&pty), "{list}");

    r.lab
        .call(
            "agent_send",
            json!({"pane": "p1", "text": "hello-from-lab"}),
        )
        .await
        .unwrap();
    // Typed into MAIN's pty: echoed by the tty, then printed by `cat` after the Enter.
    let text = until("the prompt in main's pty", async || {
        let t = r
            .main
            .call("pty_read", json!({"pty": pty, "lines": 10}))
            .await
            .ok()?["text"]
            .as_str()?
            .to_string();
        (t.matches("hello-from-lab").count() >= 2).then_some(t)
    })
    .await;
    assert!(text.contains("hello-from-lab"), "{text:?}");

    // The same refusals as main (the lab forwards the call; main decides).
    hook(&r.main_dir(), "p1", json!({"event": "agent_start"}));
    until("p1 to work in the lab", async || {
        let v = r.lab.call("agent_status_all", json!({})).await.ok()?;
        (v["panes"]["p1"]["state"] == "working").then_some(())
    })
    .await;
    let err = r
        .lab
        .call("agent_send", json!({"pane": "p1", "text": "x"}))
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(err, "agent is working");
}

/// `pty_write` / `pty_read` / `pty_signal` go to main's terminal through the lab.
#[tokio::test(flavor = "multi_thread")]
async fn terminal_io_through_the_lab_acts_on_the_main_pty() {
    use base64::Engine;
    let r = Rig::new().await;
    let pty = r
        .main
        .call(
            "pty_create",
            json!({"cwd": r.tmp.path(), "argv": ["/bin/sh", "-c", "stty -echo; cat"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    until("the lab to list the pty", async || {
        r.lab
            .call("pty_list", json!({}))
            .await
            .ok()?
            .to_string()
            .contains(&pty)
            .then_some(())
    })
    .await;
    let data = base64::engine::general_purpose::STANDARD.encode("typed-in-lab\n");
    r.lab
        .call("pty_write", json!({"pty": pty, "data": data}))
        .await
        .unwrap();
    let text = until("lab pty_read to show the line", async || {
        let t = r
            .lab
            .call("pty_read", json!({"pty": pty, "lines": 10}))
            .await
            .ok()?["text"]
            .as_str()?
            .to_string();
        t.contains("typed-in-lab").then_some(t)
    })
    .await;
    assert!(text.contains("typed-in-lab"));
    // SIGINT stops `cat`: main's pty exits, which the lab sees in its own pty_list.
    r.lab
        .call("pty_signal", json!({"pty": pty, "signal": "KILL"}))
        .await
        .unwrap();
    until("the pty to exit on main", async || {
        let l = r.main.call("pty_list", json!({})).await.ok()?;
        l["ptys"][0]["exited"].is_object().then_some(())
    })
    .await;
}

/// An unknown pty is main's error, passed through, not "unknown pty" from an empty local table.
#[tokio::test(flavor = "multi_thread")]
async fn errors_from_main_pass_through_the_lab() {
    let r = Rig::new().await;
    let e = r
        .lab
        .call("pty_read", json!({"pty": "nope", "lines": 1}))
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(e, "unknown pty nope");
}

/// The lab refuses what would act on terminals or agents of main in a way that is not a phone
/// request (it mirrors, it does not own), instead of silently doing it on an empty local table.
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_never_creates_kills_or_hands_off_terminals() {
    let r = Rig::new().await;
    let pty = r
        .main
        .call("pty_create", json!({"cwd": r.tmp.path(), "argv": ["cat"]}))
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    for (cmd, args) in [
        ("pty_create", json!({"cwd": r.tmp.path(), "argv": ["cat"]})),
        ("pty_kill", json!({"pty": pty})),
        ("pty_clear", json!({"pty": pty})),
        ("pty_resize", json!({"pty": pty, "cols": 10, "rows": 10})),
        (
            "agent_bind",
            json!({"pane": "p", "agent": "omp", "cwd": "/"}),
        ),
        ("agent_pane_exited", json!({"pane": "p"})),
        ("upgrade", json!({"exe": "/bin/true"})),
    ] {
        let e = r.lab.call(cmd, args).await.unwrap_err().to_string();
        assert!(e.starts_with("lab hostd"), "{cmd}: {e}");
    }
    // Main's terminal is untouched.
    let l = r.main.call("pty_list", json!({})).await.unwrap();
    assert!(l["ptys"][0]["exited"].is_null(), "{l}");
}

/// The invariant of the whole lane: whatever the lab does, MAIN's directory is not written to
/// by it (sockets/pid/token/key/devices/invites/audit all live under the lab dir). The snapshot is
/// taken with main fully up and BEFORE the lab exists, so even a write at lab startup shows.
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_never_writes_into_the_main_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let main_dir = tmp.path().join("main");
    let main = main_daemon(&main_dir).await;
    until("the main hook endpoint", async || {
        std::fs::read_to_string(crate::paths::hook_endpoint(&main_dir))
            .ok()
            .filter(|e| e.contains("PORT"))
    })
    .await;
    // Main touches mobile/ only once it needs it: create its identity now so a lab that
    // wrongly wrote there is a visible diff, not "main's own lazy file".
    main.call("mobile_devices", json!({})).await.unwrap();
    hook(&main_dir, "p1", json!({"event": "agent_end"}));
    let before = tree(&main_dir);
    let bytes_of = |d: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        tree(d)
            .into_iter()
            .filter(|p| d.join(p).is_file() && !p.starts_with("hostd/scrollback"))
            .map(|p| {
                let b = std::fs::read(d.join(&p)).unwrap_or_default();
                (p, b)
            })
            .collect()
    };
    let main_files = bytes_of(&main_dir);

    let lab = lab_daemon(&tmp.path().join("lab"), &main_dir).await;
    until("the lab to list p1", async || {
        let v = lab.call("agent_status_all", json!({})).await.ok()?;
        v["panes"]["p1"].is_object().then_some(())
    })
    .await;
    // Everything a phone-facing lab does: pair, list, project calls, forwarded terminal calls.
    let info = lab.call("mobile_pair_start", json!({})).await.unwrap();
    assert!(info["code"].is_string());
    lab.call("mobile_devices", json!({})).await.unwrap();
    lab.call("project_list", json!({})).await.unwrap();
    lab.call("pty_list", json!({})).await.unwrap();
    lab.call("info", json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        tree(&main_dir),
        before,
        "the lab added or removed a file under the main dir"
    );
    assert_eq!(
        bytes_of(&main_dir),
        main_files,
        "the lab changed a file under the main dir"
    );
    // The lab has its own identity and files, distinct from main's.
    let lab_dir = tmp.path().join("lab");
    assert_ne!(
        std::fs::read(lab_dir.join("mobile/key")).unwrap(),
        std::fs::read(main_dir.join("mobile/key")).unwrap(),
        "lab must not share main's iroh key"
    );
    for f in [
        "hostd.sock",
        "hostd.pid",
        "hostd.token",
        "mobile/key",
        "mobile",
    ] {
        assert!(lab_dir.join(f).exists(), "lab {f}");
    }
    assert!(
        !lab_dir.join("desktop/hook-endpoint.env").exists(),
        "a lab must not publish a hook endpoint"
    );
}

/// Projects come from MAIN's registry (the lab has none of its own), and agents are attached to
/// them using main's terminals (the lab owns no pty).
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_lists_main_projects_with_main_agents() {
    let tmp = tempfile::tempdir().unwrap();
    let main_dir = tmp.path().join("main");
    let proj = tmp.path().join("shop");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(main_dir.join("desktop")).unwrap();
    std::fs::write(
        main_dir.join("desktop/projects.json"),
        json!([{"id": "a", "root": proj, "pinned": false, "added": "x"}]).to_string(),
    )
    .unwrap();
    let main = main_daemon(&main_dir).await;
    main.call(
        "pty_create",
        json!({"cwd": proj, "argv": ["cat"], "scope": {"pane_key": "p1"}}),
    )
    .await
    .unwrap();
    until("the main hook endpoint", async || {
        std::fs::read_to_string(crate::paths::hook_endpoint(&main_dir))
            .ok()
            .filter(|e| e.contains("PORT"))
    })
    .await;
    hook(&main_dir, "p1", json!({"event": "agent_start"}));
    let lab = lab_daemon(&tmp.path().join("lab"), &main_dir).await;
    let list = until("the lab project list with the agent", async || {
        let v = lab.call("project_list", json!({})).await.ok()?;
        v.to_string().contains("\"p1\"").then_some(v)
    })
    .await;
    assert_eq!(list["projects"][0]["id"], "a", "{list}");
}

/// A lab with no main daemon is a clear startup error, never a silent empty mirror.
#[tokio::test]
async fn the_lab_refuses_to_start_without_a_main_daemon() {
    let tmp = tempfile::tempdir().unwrap();
    let opts = ServeOpts {
        no_mobile_relay: true,
        lab: Some(tmp.path().join("nomain")),
        ..Default::default()
    };
    let e = crate::server::serve_with(&tmp.path().join("lab"), opts)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("no main atlas hostd") && e.contains("nomain"),
        "{e}"
    );
}

/// Lab and main must be different directories: a lab on main's dir would lock, bind and
/// overwrite the very files it exists to leave alone.
#[tokio::test]
async fn the_lab_refuses_main_s_own_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let main_dir = tmp.path().join("main");
    let _main = main_daemon(&main_dir).await;
    let opts = ServeOpts {
        no_mobile_relay: true,
        lab: Some(main_dir.clone()),
        ..Default::default()
    };
    let e = crate::server::serve_with(&main_dir, opts)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("lab dir must differ"), "{e}");
}

/// A live handoff makes no sense for a lab (it owns no terminal): refused up front.
#[tokio::test]
async fn the_lab_cannot_be_a_takeover_successor() {
    let tmp = tempfile::tempdir().unwrap();
    let opts = ServeOpts {
        lab: Some(tmp.path().join("main")),
        takeover: Some(tmp.path().join("x.sock")),
        ..Default::default()
    };
    let e = crate::server::serve_with(&tmp.path().join("lab"), opts)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("lab"), "{e}");
}

/// Which commands the lab forwards, answers itself, or refuses. A new `HostCmd` must be placed
/// here on purpose: the test names every variant a phone can send.
#[test]
fn every_command_a_phone_may_send_has_an_explicit_route() {
    use super::{Route, route};
    use crate::protocol::HostCmd;
    let parse = |cmd: &str, args: Value| -> HostCmd {
        serde_json::from_value(json!({"cmd": cmd, "args": args})).unwrap()
    };
    let main_cmds = [
        ("pty_list", json!({})),
        ("pty_info", json!({"pty": "x"})),
        ("pty_read", json!({"pty": "x"})),
        ("pty_write", json!({"pty": "x", "data": ""})),
        ("pty_signal", json!({"pty": "x", "signal": "INT"})),
        ("agent_send", json!({"pane": "p", "text": "t"})),
        ("agent_interrupt", json!({"pane": "p"})),
        (
            "agent_approve",
            json!({"pane": "p", "req": "r", "allow": true}),
        ),
        ("agent_launch", json!({"project": "p", "agent": "omp"})),
    ];
    for (c, a) in main_cmds {
        assert!(matches!(route(&parse(c, a)), Route::Main), "{c} → main");
    }
    let local_cmds = [
        ("agent_status_all", json!({})),
        ("agent_integrations", json!({})),
        ("agent_tree", json!({"pane": "p"})),
        ("agent_history", json!({"pane": "p"})),
        ("project_list", json!({})),
        ("project_detail", json!({"id": "a"})),
        ("project_logs", json!({"id": "a"})),
        ("project_service", json!({"id": "a", "op": "restart"})),
        ("project_action", json!({"id": "a", "key": "k"})),
        ("info", json!({})),
        ("mobile_pair_start", json!({})),
        ("mobile_devices", json!({})),
    ];
    for (c, a) in local_cmds {
        assert!(matches!(route(&parse(c, a)), Route::Local), "{c} → local");
    }
    // Everything a phone is allowed (`required_scope` Ok) is covered by Main or Local above,
    // never Refuse: a refusal would break the phone silently.
    for (c, a) in [
        ("pty_attach", json!({"pty": "x"})),
        ("pty_detach", json!({"pty": "x"})),
    ] {
        assert!(
            matches!(route(&parse(c, a)), Route::Attach),
            "{c} → the per-connection attach relay"
        );
    }
}

use crate::mobile::tests::{Phone, pair_start};

/// A phone paired with the LAB (its own pairing and key), never with main.
async fn lab_phone(r: &Rig) -> Phone {
    let info = pair_start(&r.lab).await;
    let mut phone = Phone::dial(&info).await;
    let ok = phone
        .call("pair", json!({"code": info["code"], "name": "Lab iPhone"}))
        .await
        .unwrap();
    assert_eq!(ok["ok"], true, "{ok}");
    // The lab pairing is the lab's: main has no device and no endpoint of its own.
    let main_devices = r.main.call("mobile_devices", json!({})).await.unwrap();
    assert_eq!(main_devices["devices"], json!([]), "paired with main!");
    phone
}

/// Terminal output of MAIN's pty reaches a phone attached through the lab: replay first, then live
/// bytes; input typed on the phone lands in the same pty.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_attached_through_the_lab_streams_main_s_terminal() {
    use base64::Engine;
    let r = Rig::new().await;
    let pty = r
        .main
        .call(
            "pty_create",
            json!({"cwd": r.tmp.path(), "argv": ["/bin/sh", "-c", "echo before-attach; stty -echo; cat"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    until("the output to be buffered", async || {
        r.main
            .call("pty_read", json!({"pty": pty, "lines": 5}))
            .await
            .ok()?["text"]
            .as_str()?
            .contains("before-attach")
            .then_some(())
    })
    .await;
    let mut phone = lab_phone(&r).await;
    // The replay is pushed ahead of the attach reply.
    let (attach, mut early) = phone
        .call_with_pushes(
            "pty_attach",
            json!({"pty": pty, "replay": true, "output": true}),
        )
        .await
        .unwrap();
    assert_eq!(attach["ok"], true, "{attach}");
    let data = base64::engine::general_purpose::STANDARD.encode("from-the-phone\n");
    let (w, mid) = phone
        .call_with_pushes("pty_write", json!({"pty": pty, "data": data}))
        .await
        .unwrap();
    assert_eq!(w["ok"], true, "{w}");
    early.extend(mid);

    let mut seen = String::new();
    for _ in 0..40 {
        early.extend(phone.pushes(250).await);
        for ev in early.drain(..) {
            if matches!(ev["ev"].as_str(), Some("pty_replay" | "pty_output")) {
                let b = base64::engine::general_purpose::STANDARD
                    .decode(ev["data"].as_str().unwrap_or_default())
                    .unwrap_or_default();
                seen += &String::from_utf8_lossy(&b);
            }
        }
        if seen.contains("before-attach") && seen.contains("from-the-phone") {
            break;
        }
    }
    assert!(seen.contains("before-attach"), "no replay: {seen:?}");
    assert!(seen.contains("from-the-phone"), "no live output: {seen:?}");
}

/// Main's connected clients: the test's own, the lab's follower and one per attached phone.
async fn main_clients(r: &Rig) -> u64 {
    r.main.call("info", json!({})).await.unwrap()["clients"]
        .as_u64()
        .unwrap()
}

/// A phone attached through the lab costs main one connection (its link), stops streaming on
/// `pty_detach`, and a phone that merely disconnects leaves NOTHING behind on the maintainer's real
/// hostd: no subscriber that keeps buffering output for a phone that is gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_leaving_the_lab_leaves_no_subscriber_on_main() {
    let r = Rig::new().await;
    let pty = r
        .main
        .call(
            "pty_create",
            json!({"cwd": r.tmp.path(), "argv": ["sh", "-c", "while :; do echo tick; sleep 0.1; done"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    let base = main_clients(&r).await;
    let mut phone = lab_phone(&r).await;
    // Pairing alone opens nothing on main: the link is made when a terminal is attached.
    assert_eq!(main_clients(&r).await, base);
    let (ok, mut pushes) = phone
        .call_with_pushes(
            "pty_attach",
            json!({"pty": pty, "replay": false, "output": true}),
        )
        .await
        .unwrap();
    assert_eq!(ok["ok"], true, "{ok}");
    pushes.extend(phone.pushes(800).await);
    assert!(
        pushes.iter().any(|e| e["ev"] == "pty_output"),
        "no live output through the lab"
    );
    assert_eq!(
        main_clients(&r).await,
        base + 1,
        "one link per attached phone"
    );

    // Explicit detach: the stream stops.
    let (d, mut tail) = phone
        .call_with_pushes("pty_detach", json!({"pty": pty}))
        .await
        .unwrap();
    assert_eq!(d["ok"], true, "{d}");
    tail.clear();
    phone.pushes(400).await;
    let after = phone.pushes(600).await;
    assert!(
        !after.iter().any(|e| e["ev"] == "pty_output"),
        "still streaming after pty_detach: {after:?}"
    );

    // Disconnecting (the app is killed): main's link connection is released.
    phone.hang_up();
    until("main to drop the lab link", async || {
        (main_clients(&r).await == base).then_some(())
    })
    .await;
}

/// A prompt main raises for ITS phone is mirrored on the lab (pushed and listed) and answered
/// through main; and the lab never makes main hold a prompt for itself: with no phone of main's own
/// connected, main answers "no decision" at once, so the maintainer's real agents never wait on an
/// experimental phone.
#[tokio::test(flavor = "multi_thread")]
async fn the_lab_mirrors_main_s_approvals_and_answers_them_through_main() {
    let r = Rig::new().await;
    let mut lab_phone = lab_phone(&r).await;
    // No phone on main: the prompt is not held (nothing waits on the lab phone).
    let t = std::time::Instant::now();
    let body = tokio::task::spawn_blocking({
        let d = r.main_dir();
        move || decision(&d)
    })
    .await
    .unwrap();
    assert_eq!(crate::agents::hook_server::parse_decision(&body), None);
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());

    // Main's own phone connects: now a prompt is held, and the lab phone sees and answers it.
    let main_info = pair_start(&r.main).await;
    let mut main_phone = Phone::dial(&main_info).await;
    let ok = main_phone
        .call(
            "pair",
            json!({"code": main_info["code"], "name": "Main iPhone"}),
        )
        .await
        .unwrap();
    assert_eq!(ok["ok"], true, "{ok}");
    let asked = tokio::task::spawn_blocking({
        let d = r.main_dir();
        move || decision(&d)
    });
    let req = loop {
        let p = lab_phone.pushes(500).await;
        if let Some(e) = p.iter().find(|e| e["ev"] == "agent_approval") {
            break e["req"].as_str().unwrap().to_string();
        }
    };
    // The mirrored prompt is also listed for a phone that connects after the push.
    let listed = r.lab.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(listed["pending_approvals"][0]["req"], req, "{listed}");
    let ans = lab_phone
        .call(
            "agent_approve",
            json!({"pane": "p1", "req": req, "allow": true}),
        )
        .await
        .unwrap();
    assert_eq!(ans["ok"], true, "{ans}");
    assert_eq!(
        crate::agents::hook_server::parse_decision(&asked.await.unwrap()),
        Some(true)
    );
    // Once answered, the mirror forgets it.
    until("the lab to drop the answered prompt", async || {
        let v = r.lab.call("agent_status_all", json!({})).await.ok()?;
        v["pending_approvals"].as_array()?.is_empty().then_some(())
    })
    .await;
}

/// POST a permission prompt for pane `p1` to the hook server of `dir`; the reply body.
fn decision(dir: &Path) -> String {
    use std::io::{Read, Write};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = json!({"pane_key": "p1", "payload": {"tool_name": "Bash"}}).to_string();
    let mut s =
        std::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT"))).unwrap();
    write!(
        s,
        "POST /hook/claude/decision HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    )
    .unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    r.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}
