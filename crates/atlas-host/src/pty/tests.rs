use std::time::Duration;

use tokio::sync::mpsc;

use super::title::{TitleParser, shown};
use super::{RAW_QUEUE, Session};
use crate::protocol::PtyCreate;

#[test]
fn title_split_across_chunks() {
    let mut p = TitleParser::default();
    assert_eq!(p.feed("x\x1b]0;π ".as_bytes()), None);
    assert_eq!(p.feed(b": work\x07y").as_deref(), Some("π : work"));
    assert_eq!(p.feed(b"\x1b]2;two\x1b\\").as_deref(), Some("two"));
    assert_eq!(p.feed(b"\x1b]1;icon\x07"), None);
}

/// #56: an agent without hooks shows working/idle from terminal output. The event is throttled to one per
/// `ACTIVITY_EVERY` so a stream of output costs one line a second, not one per chunk.
#[test]
fn activity_is_reported_at_most_once_per_interval() {
    use super::stream::{ACTIVITY_EVERY, Activity};
    let t0 = std::time::Instant::now();
    let mut a = Activity::default();
    assert!(a.due(t0), "first output");
    assert!(!a.due(t0 + ACTIVITY_EVERY / 2), "inside the interval");
    assert!(a.due(t0 + ACTIVITY_EVERY), "a full interval later");
    assert!(!a.due(t0 + ACTIVITY_EVERY), "same instant twice");
}

#[cfg(unix)]
fn sh(cmd: &str) -> PtyCreate {
    PtyCreate {
        cwd: "/".into(),
        argv: vec!["/bin/sh".into(), "-c".into(), cmd.into()],
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Default::default(),
        title: None,
        mode: Default::default(),
    }
}

/// Listener-only clients (the app watches every pty that way) get `pty_activity` when the program prints,
/// and none from a program that stays silent. Unix only: ConPTY writes its own setup sequences for every
/// program, so on Windows no program is silent.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn listeners_get_activity_only_from_output() {
    let dir = tempfile::tempdir().unwrap();
    for (i, (cmd, want)) in [("printf hello", true), ("sleep 0.2", false)]
        .into_iter()
        .enumerate()
    {
        // The program waits for `go`, created after the attach: output printed before the listener
        // attached sends it no activity, so on a loaded CI runner (Linux, PR #106) `printf hello`
        // could finish first and the test saw nothing. Same race as the title tests (#193).
        let go = dir.path().join(format!("go{i}"));
        let script = format!(
            "while [ ! -e '{}' ]; do sleep 0.01; done; {cmd}",
            go.display()
        );
        let s = Session::spawn(sh(&script), dir.path()).unwrap();
        let (tx, mut rx) = super::out_channel();
        s.attach(1, tx, false, false);
        std::fs::write(&go, b"").unwrap();
        until_exited(&s).await;
        let mut got = false;
        while let Some(l) = rx.try_recv() {
            got |= l.contains(r#""ev":"pty_activity""#);
        }
        assert_eq!(got, want, "{cmd}");
    }
}

// Incident: on Windows every terminal tab read "C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe":
// ConPTY titles the console with the program's full path (cmd adds " - <command>" while one runs).
#[test]
fn a_console_title_that_is_a_program_path_shows_the_program_name() {
    let ps = r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe";
    assert_eq!(shown(ps.into()), "powershell");
    assert_eq!(
        shown(r"C:\WINDOWS\system32\cmd.EXE - npm run dev".into()),
        "cmd - npm run dev"
    );
    assert_eq!(
        shown(r"Administrator: C:\Program Files\PowerShell\7\pwsh.exe".into()),
        "Administrator: pwsh"
    );
    // Anything else is the program's own title, untouched.
    for t in [
        "vim notes.txt",
        "~/src: zsh",
        r"C:\src\atlas",
        "claude",
        "build.exe finished",
        "",
    ] {
        assert_eq!(shown(t.into()), t);
    }
}

// Incident (Windows): PowerShell titles its console once, at start, before the app has attached to the new
// terminal. That `pty_title` push went to nobody and the tab kept "Terminal". The attach answer carries the
// current title; `protocol::title_event` turns it into the event the app's tab store reads.
#[tokio::test(flavor = "multi_thread")]
async fn a_title_set_before_attach_comes_back_as_a_title_event() {
    let dir = std::env::temp_dir().join(format!("atlas-host-title-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = if cfg!(windows) {
        "title built"
    } else {
        "printf '\\033]0;built\\007'"
    };
    let s = Session::spawn(shell_req(script), &dir).unwrap();
    until_exited(&s).await;
    let (tx, _rx) = super::out_channel();
    let info = s.attach(1, tx, true, true);
    let ev = crate::protocol::title_event(&s.id, &info).expect("a title event");
    assert_eq!(
        (&ev["ev"], &ev["pty"]),
        (&serde_json::json!("pty_title"), &serde_json::json!(s.id))
    );
    // An elevated Windows console (CI runners are) puts "Administrator: " before what the program set, in the
    // system's language ("Administrador: " on a pt-BR machine).
    let title = ev["title"].as_str().unwrap_or_default();
    let set = if cfg!(windows) {
        title.split_once(':').map_or(title, |(_, r)| r).trim_start()
    } else {
        title
    };
    assert_eq!(set, "built", "{title:?}");
    assert_eq!(
        crate::protocol::title_event("p", &serde_json::json!({"title": ""})),
        None
    );
}

/// A raw reader that never reads must not freeze the program: it is dropped
/// after `RAW_STALL` and the output (far more than the queues hold) completes.
#[tokio::test(flavor = "multi_thread")]
async fn stuck_raw_reader_does_not_block_the_program() {
    let dir = std::env::temp_dir().join(format!("atlas-host-raw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Far more output than the queues hold (ConPTY renders it, so Windows prints lines). On Unix
    // the output waits for `go`, created once the reader is attached: otherwise a loaded machine
    // could finish the program first and the test passed without a reader to stall on (#198).
    let go = dir.join("go-raw");
    let _ = std::fs::remove_file(&go);
    let script = if cfg!(windows) {
        "for /L %i in (1,1,20000) do @echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
    } else {
        format!(
            "while [ ! -e '{}' ]; do sleep 0.01; done; head -c 20000000 /dev/zero",
            go.display()
        )
    };
    let s = Session::spawn(shell_req(&script), &dir).unwrap();
    let (tx, _rx_never_read) = mpsc::channel(RAW_QUEUE);
    s.attach_raw(1, tx, false);
    std::fs::write(&go, "").unwrap();
    let t0 = std::time::Instant::now();
    while s.exited().is_none() {
        assert!(
            t0.elapsed() < Duration::from_secs(20),
            "program froze behind the reader"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(s.buf.lock().raw.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pty running `script` through the platform shell, in the temp dir.
fn shell_req(script: &str) -> PtyCreate {
    let argv = if cfg!(windows) {
        vec!["cmd.exe".into(), "/d".into(), "/c".into(), script.into()]
    } else {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    };
    PtyCreate {
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        argv,
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Default::default(),
        title: None,
        mode: Default::default(),
    }
}

async fn until_exited(s: &Session) -> Option<i32> {
    let t0 = std::time::Instant::now();
    loop {
        if let Some(code) = s.exited() {
            return code;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(15),
            "the session never ended after its program exited"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A finished program ends its session with its exit code and its last output kept. On Windows
/// this guards the ConPTY rule that the output pipe only ends once the pseudo console is closed:
/// without closing it on exit the session would stay "running" forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_exits_ends_its_session_with_its_code_and_output() {
    let dir = tempfile::tempdir().unwrap();
    let s = Session::spawn(shell_req("echo atlas-pty-ok&& exit 3"), dir.path()).unwrap();
    assert_eq!(until_exited(&s).await, Some(3));
    let text = s.read_text(50);
    assert!(text.contains("atlas-pty-ok"), "{text:?}");
    // Exited: input is refused, and signalling is a no-op rather than an error.
    assert!(s.write(b"x").is_err());
    assert!(s.signal("TERM").is_ok());
}

/// Incident (main CI, 2026-11-15, `a_finished_sign_in_terminal_is_removed_after_the_grace_...`): an exit
/// listener (`attach` with `output: false`) attached after the program already ended was put in a map that
/// never fires again, so the watcher waited forever. A sign-in whose command exits at once (engine not
/// installed) then stayed in `pty_list` until hostd restarted. A late listener gets `pty_exit` right away.
#[tokio::test(flavor = "multi_thread")]
async fn an_exit_listener_attached_after_the_exit_still_gets_pty_exit() {
    let dir = tempfile::tempdir().unwrap();
    let s = Session::spawn(shell_req("exit 4"), dir.path()).unwrap();
    assert_eq!(until_exited(&s).await, Some(4));
    let (tx, mut rx) = crate::pty::out_channel();
    s.attach(1, tx, false, false);
    let line = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("a late listener never heard the exit")
        .expect("the listener closed without the exit");
    assert!(line.contains(r#""ev":"pty_exit""#), "{line}");
    assert!(line.contains(r#""code":4"#), "{line}");
}

/// `KILL` ends a running program (and on Windows everything it started).
#[tokio::test(flavor = "multi_thread")]
async fn kill_ends_a_running_program() {
    let dir = tempfile::tempdir().unwrap();
    let script = if cfg!(windows) {
        "ping -n 60 127.0.0.1 >NUL"
    } else {
        "sleep 60"
    };
    let s = Session::spawn(shell_req(script), dir.path()).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(s.exited().is_none());
    s.signal("KILL").unwrap();
    until_exited(&s).await;
    assert!(s.signal("bogus").is_err());
}

// The login-env fixtures are POSIX shells.
#[cfg(unix)]
/// Incident (Linux, app launched from the menu): a terminal or agent started through hostd inherited the
/// bare session PATH and could not find `bun` (`~/.bun/bin`, added only by the user's rc file). Every pty
/// and exec hostd starts runs with the login shell's environment, under the caller's own `env`.
#[tokio::test(flavor = "multi_thread")]
async fn pty_and_exec_find_binaries_the_login_shell_put_on_path() {
    use atlas_core::login_env::{fixtures, install, probe_with};
    let Some(sh) = fixtures::some_shell() else {
        return;
    };
    let home = fixtures::fake_home();
    let _env = install(probe_with(sh, home.path(), &fixtures::gui_env(home.path())));
    let dir = tempfile::tempdir().unwrap();

    let req = PtyCreate {
        cwd: "/".into(),
        argv: vec!["/bin/sh".into(), "-c".into(), "bun from-pty".into()],
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Default::default(),
        title: None,
        mode: Default::default(),
    };
    let s = Session::spawn(req, dir.path()).unwrap();
    let t0 = std::time::Instant::now();
    while s.exited().is_none() {
        assert!(t0.elapsed() < Duration::from_secs(10), "pty did not exit");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out = String::from_utf8_lossy(&s.buf.lock().ring_bytes()).into_owned();
    assert!(out.contains("bun-ran from-pty"), "{out:?}");

    let r = crate::exec::run(
        "/",
        &[
            "/bin/sh".into(),
            "-c".into(),
            "bun from-exec; echo $CALLER".into(),
        ],
        &[("CALLER".to_string(), "mine".to_string())].into(),
        None,
        10_000,
    )
    .await
    .unwrap();
    let stdout = {
        use base64::Engine;
        let b = r["stdout"].as_str().unwrap_or_default();
        String::from_utf8_lossy(
            &base64::engine::general_purpose::STANDARD
                .decode(b)
                .unwrap_or_default(),
        )
        .into_owned()
    };
    assert!(
        stdout.contains("bun-ran from-exec") && stdout.contains("mine"),
        "{r}"
    );
}

/// Incident (maintainer, `bun run desktop:dev`): `bun run` runs scripts under `/bin/bash`, so the hostd
/// inherited SHELL=/bin/bash and every new terminal tab (no argv) started bash, while the account's login
/// shell is fish. A tab with no argv starts the shell the probe resolved, whatever `$SHELL` says. The
/// stand-in shell prints its own name and flags, so the test sees which program the tab ran and how.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_tab_without_argv_starts_the_probed_login_shell_not_the_inherited_one() {
    use atlas_core::login_env::{fixtures, install, probe_with};
    use std::os::unix::fs::PermissionsExt;

    let home = fixtures::fake_home();
    let fake = home.path().join("loginsh");
    std::fs::write(&fake, "#!/bin/sh\necho \"ran-login-shell $0 $*\"\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut p = probe_with("/bin/sh", home.path(), &fixtures::gui_env(home.path()));
    p.shell = fake.display().to_string();
    let _env = install(p);
    // The launcher's value that used to win. Set before any thread of this test reads it.
    // SAFETY: only this serialized (`install`) test touches SHELL.
    unsafe { std::env::set_var("SHELL", "/bin/bash") };

    let dir = tempfile::tempdir().unwrap();
    let req = PtyCreate {
        cwd: "/".into(),
        argv: vec![],
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Default::default(),
        title: None,
        mode: Default::default(),
    };
    let s = Session::spawn(req, dir.path()).unwrap();
    let t0 = std::time::Instant::now();
    while s.exited().is_none() {
        assert!(t0.elapsed() < Duration::from_secs(10), "pty did not exit");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out = String::from_utf8_lossy(&s.buf.lock().ring_bytes()).into_owned();
    assert!(
        out.contains(&format!("ran-login-shell {} -l", fake.display())),
        "{out:?}"
    );
    assert_eq!(s.meta.argv, [fake.display().to_string(), "-l".into()]);
}

/// A client that never reads while a program prints far more than the cap: hostd's queue for it
/// stays bounded, the program is not slowed, a healthy attacher misses nothing, and a reattach
/// with replay continues. Unix only: it needs a fast byte source.
// Regression: #109, hostd queued every `pty_output` for a client that stopped reading, without limit.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_attacher_that_never_reads_is_dropped_and_the_others_keep_streaming() {
    use super::OUT_CAP;
    use base64::Engine;

    const TOTAL: usize = 40 * 1024 * 1024;
    let dir = tempfile::tempdir().unwrap();
    let script = format!("head -c {TOTAL} /dev/zero | tr '\\0' a; echo END; sleep 30");
    let s = Session::spawn(shell_req(&script), dir.path()).unwrap();

    let (stuck, mut stuck_rx) = super::out_channel();
    let (healthy, mut healthy_rx) = super::out_channel();
    s.attach(1, stuck.clone(), false, true);
    s.attach(2, healthy, false, true);

    let decode = |line: &str| -> Option<Vec<u8>> {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        matches!(v["ev"].as_str(), Some("pty_output" | "pty_replay")).then(|| {
            base64::engine::general_purpose::STANDARD
                .decode(v["data"].as_str().unwrap())
                .unwrap()
        })
    };
    let (mut text_tail, mut seen, mut stuck_max) = (String::new(), 0usize, 0usize);
    let t0 = std::time::Instant::now();
    while !text_tail.contains("END") {
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "the program froze behind a client that does not read (saw {seen} bytes)"
        );
        while let Some(line) = healthy_rx.try_recv() {
            if let Some(b) = decode(&line) {
                seen += b.len();
                text_tail.push_str(&String::from_utf8_lossy(&b));
                let keep = text_tail.len().saturating_sub(16);
                text_tail.drain(..keep);
            }
        }
        stuck_max = stuck_max.max(stuck.pending());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Bounded: the cap plus the one line that crossed it (a coalesced chunk is at most 64 KiB,
    // 4/3 of that as base64, plus the JSON envelope).
    assert!(
        stuck_max <= OUT_CAP + 128 * 1024,
        "queued {stuck_max} bytes for a client that never reads"
    );
    assert!(
        seen >= TOTAL,
        "the healthy attacher saw {seen} of {TOTAL} bytes"
    );
    // The stuck one is unsubscribed, and its queue ends with the reason.
    {
        let b = s.buf.lock();
        assert!(
            !b.attachers.contains_key(&1),
            "stuck attacher still subscribed"
        );
        assert!(b.attachers.contains_key(&2), "healthy attacher was dropped");
    }
    let mut last = None;
    while let Some(line) = stuck_rx.try_recv() {
        last = Some(line);
    }
    let notice: serde_json::Value = serde_json::from_str(&last.unwrap()).unwrap();
    assert_eq!(notice["ev"], "pty_dropped", "{notice}");
    assert_eq!(notice["reason"], "slow_client");
    assert_eq!(notice["pty"], s.id.as_str());
    assert!(super::dropped_total() >= 1);

    // Reattach with replay: the ring (2 MiB) comes back and ends where the program is.
    let (again, mut again_rx) = super::out_channel();
    s.attach(3, again, true, true);
    let mut replay = Vec::new();
    while let Some(line) = again_rx.try_recv() {
        replay.extend(decode(&line).unwrap_or_default());
    }
    assert!(
        replay.len() > 1024 * 1024,
        "replay came back with {} bytes",
        replay.len()
    );
    assert!(String::from_utf8_lossy(&replay).contains("END"));
    s.signal("KILL").unwrap();
}

/// The notice a dropped client gets names the pty and the reason, the others are untouched.
// Regression: #109
#[test]
fn a_connection_over_its_cap_is_told_why_and_unsubscribed() {
    use super::{Buf, out_channel_with_cap};
    use std::collections::HashMap;

    let (slow, mut slow_rx) = out_channel_with_cap(8);
    let (fast, mut fast_rx) = out_channel_with_cap(1 << 20);
    let mut conns = HashMap::from([(1u64, slow), (2u64, fast)]);
    // The cap is checked before a line is added: the first line fits, the next does not.
    Buf::broadcast("p1", &mut conns, "0123456789");
    assert_eq!(conns.len(), 2);
    Buf::broadcast("p1", &mut conns, "next");
    assert_eq!(
        conns.keys().copied().collect::<Vec<_>>(),
        vec![2],
        "slow one removed"
    );
    assert_eq!(slow_rx.try_recv().as_deref(), Some("0123456789"));
    let notice: serde_json::Value = serde_json::from_str(&slow_rx.try_recv().unwrap()).unwrap();
    assert_eq!(
        (&notice["ev"], &notice["pty"], &notice["reason"]),
        (
            &serde_json::json!("pty_dropped"),
            &serde_json::json!("p1"),
            &serde_json::json!("slow_client")
        )
    );
    assert!(slow_rx.try_recv().is_none(), "nothing after the notice");
    assert_eq!(fast_rx.try_recv().as_deref(), Some("0123456789"));
    assert_eq!(fast_rx.try_recv().as_deref(), Some("next"));
}

/// `pty_exit` reaches a client that is over its cap: it must learn the program ended.
// Regression: #109 (dropping over-cap output must not drop the exit)
#[test]
fn pty_exit_is_delivered_even_to_a_connection_over_its_cap() {
    use super::{Buf, out_channel_with_cap};

    let (out, mut rx) = out_channel_with_cap(1);
    out.send("fill".into()).unwrap();
    let mut b = Buf::new(Default::default(), None, 0);
    b.attachers.insert(1, out);
    b.notify_force(r#"{"ev":"pty_exit","pty":"p","code":0}"#);
    assert_eq!(rx.try_recv().as_deref(), Some("fill"));
    assert!(rx.try_recv().unwrap().contains("pty_exit"));
}

/// Every `pty_title` line a listener got, in order.
#[cfg(unix)]
fn titles(rx: &mut super::OutRx) -> Vec<String> {
    let mut got = Vec::new();
    while let Some(l) = rx.try_recv() {
        let v: serde_json::Value = serde_json::from_str(&l).unwrap();
        if v["ev"] == "pty_title" {
            got.push(v["title"].as_str().unwrap_or_default().to_string());
        }
    }
    got
}

// Incident (#110, "terminal freezes with 7+ agents"): omp redraws its title on every spinner frame, usually
// with the same text as the frame before. Each OSC became a `pty_title` push to every attached client, and
// each push became a Tauri event for ~4 webview listeners: hundreds of identical events a second across the
// agents, all on the UI thread. Only a title that differs from the current one is news.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_title_repeated_by_the_program_is_pushed_once() {
    let dir = tempfile::tempdir().unwrap();
    // `sleep 0.02` keeps each OSC in its own 8 ms coalescing window, so the parser sees every one.
    // The program waits for `go`, created after the attach: a title pushed before the client is
    // attached is not replayed, so on a loaded CI runner the first one was lost.
    let go = dir.path().join("go");
    let script = format!(
        "while [ ! -e '{}' ]; do sleep 0.01; done; for t in a a a b b a a a; do printf '\\033]0;%s\\007' $t; sleep 0.02; done",
        go.display()
    );
    let s = Session::spawn(shell_req(&script), dir.path()).unwrap();
    let (tx, mut rx) = super::out_channel();
    s.attach(1, tx, false, false);
    std::fs::write(&go, b"").unwrap();
    until_exited(&s).await;
    assert_eq!(titles(&mut rx), ["a", "b", "a"], "one push per change");
    assert_eq!(
        *s.title.lock(),
        "a",
        "the stored title is still the last one"
    );
}

// Class of the incident above: a title set again to what it already is must not even wake the clients, but one
// that goes back to an EARLIER title is a change (a -> b -> a is two pushes after the first).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_title_that_returns_to_an_earlier_value_is_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let go = dir.path().join("go");
    let script = format!(
        "while [ ! -e '{}' ]; do sleep 0.01; done; for t in x y x; do printf '\\033]0;%s\\007' $t; sleep 0.02; done",
        go.display()
    );
    let s = Session::spawn(shell_req(&script), dir.path()).unwrap();
    let (tx, mut rx) = super::out_channel();
    s.attach(1, tx, false, false);
    std::fs::write(&go, b"").unwrap();
    until_exited(&s).await;
    assert_eq!(titles(&mut rx), ["x", "y", "x"]);
}

// Incident (#110, "terminal freezes with many agents"): `pty_list` built every session's `summary()`, and the summary
// asked `ps` for the foreground program: one blocking fork+exec per terminal, serially, on a runtime worker. The
// desktop calls `pty_list` on every connect, tab operation and sidebar refresh, so with 30 terminals that was
// ~100+ ms of blocked worker each time. Nothing reads `fg` from the list (only `pty_info` shows it), so the list
// no longer pays for it. The keys everything else reads must stay.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_pty_list_summary_does_not_fork_ps_for_the_foreground_program() {
    let dir = tempfile::tempdir().unwrap();
    let s = Session::spawn(shell_req("sleep 30"), dir.path()).unwrap();
    let list = s.summary();
    assert!(
        list.get("fg").is_none(),
        "pty_list entries carry no fg: {list}"
    );
    for k in [
        "pty",
        "cwd",
        "argv",
        "scope",
        "title",
        "created_at",
        "exited",
        "pid",
        "cols",
        "rows",
    ] {
        assert!(list.get(k).is_some(), "pty_list lost `{k}`: {list}");
    }
    let info = s.info_summary();
    assert!(
        info["fg"]["pid"].as_i64().is_some(),
        "pty_info still names the foreground program: {info}"
    );
    assert!(
        info["fg"]["name"].as_str().is_some_and(|n| !n.is_empty()),
        "{info}"
    );
    s.kill().await;
}

/// What a pty program printed, once it exited (the ring holds the raw bytes, `\r\n` line ends).
#[cfg(unix)]
async fn ran(req: PtyCreate, dir: &std::path::Path) -> String {
    let s = Session::spawn(req, dir).unwrap();
    until_exited(&s).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out = String::from_utf8_lossy(&s.buf.lock().ring_bytes()).into_owned();
    out.replace("\r\n", "\n")
}

// Study gap 7 (Ghostty parity): agents (Claude Code, omp, codex) branch on `TERM_PROGRAM` for scroll speed,
// link clicks and graphics. A terminal tab says who it is, and hostd's own environment never leaks the
// terminal that launched it (`TERM_PROGRAM=ghostty` from the shell that ran `bun run desktop:dev`). One test:
// it edits the process environment, which the others read.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_says_it_is_atlas_whatever_launched_hostd() {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: no other test reads these two variables; set before any child is spawned.
    unsafe {
        std::env::set_var("TERM_PROGRAM", "ghostty");
        std::env::set_var("TERM_PROGRAM_VERSION", "9.9.9");
    }
    let show = "echo \"[${TERM_PROGRAM-unset}|${TERM_PROGRAM_VERSION-unset}|$COLORTERM|$TERM]\"";
    let out = ran(shell_req(show), dir.path()).await;
    let want = format!(
        "[Atlas|{}|truecolor|xterm-256color]",
        env!("CARGO_PKG_VERSION")
    );
    assert!(out.contains(&want), "{out:?}");

    // The caller's own env has the last word (an agent that asks for another identity).
    let mut req = shell_req(show);
    req.env = [
        ("TERM_PROGRAM".to_string(), "custom".to_string()),
        ("TERM_PROGRAM_VERSION".to_string(), "1".to_string()),
    ]
    .into();
    let out = ran(req, dir.path()).await;
    assert!(out.contains("[custom|1|"), "caller wins: {out:?}");

    // A pipe session is a JSONL protocol, not a terminal: no identity of ours (and none inherited here).
    unsafe {
        std::env::remove_var("TERM_PROGRAM");
        std::env::remove_var("TERM_PROGRAM_VERSION");
    }
    let mut pipe = shell_req(show);
    pipe.mode = crate::protocol::Mode::Pipe;
    let s = Session::spawn(pipe, dir.path()).unwrap();
    until_exited(&s).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out = String::from_utf8_lossy(&s.buf.lock().ring_bytes()).into_owned();
    assert!(out.contains("[unset|unset|"), "pipe session: {out:?}");
}

/// A shell that leaves a daemonized `sleep` behind (double fork: its parent exits at once, so it
/// re-parents to init and no parent chain reaches it) and writes that pid to `marker`. The
/// subshell lingers 1.5 s before it exits, so hostd's recording sees the `sleep` below the shell
/// first (a process that detaches between two recordings escapes on macOS, `atlas_core::hygiene`).
#[cfg(unix)]
fn daemonizing_shell(marker: &std::path::Path, then: &str) -> String {
    format!(
        "( sleep 300 & echo $! > {}; sleep 1.5 ) & wait; {then}",
        marker.display()
    )
}

#[cfg(unix)]
async fn daemon_pid(marker: &std::path::Path) -> u32 {
    for _ in 0..200 {
        if let Ok(s) = std::fs::read_to_string(marker)
            && let Ok(p) = s.trim().parse::<u32>()
        {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the daemonized process never started");
}

/// The recording hostd's ticker does every couple of seconds, done by hand.
#[cfg(unix)]
fn track(dir: &std::path::Path) {
    atlas_core::hygiene::Registry::at(dir).track().unwrap();
}

#[cfg(unix)]
async fn gone_within(pid: u32, within: Duration) -> bool {
    let end = std::time::Instant::now() + within;
    while std::time::Instant::now() < end {
        let zombie = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().starts_with('Z'))
            .unwrap_or(false);
        if atlas_core::procs::start_time(pid).is_none() || zombie {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Wait (bounded) until the daemon has left the shell's parent chain.
#[cfg(unix)]
async fn left_the_parent_chain(shell: u32, daemon: u32) -> bool {
    for _ in 0..200 {
        if !atlas_core::procs::tree(shell, &atlas_core::procs::table()).contains(&daemon) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

// Incident (#127): closing a terminal killed the shell, but the dev server or browser it had
// daemonized kept running for good. hostd records what runs under each terminal while it lives;
// the close ends all of it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_terminal_ends_the_process_it_daemonized() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("daemon.pid");
    let s = Session::spawn(
        shell_req(&daemonizing_shell(&marker, "exec sleep 300")),
        dir.path(),
    )
    .unwrap();
    let daemon = daemon_pid(&marker).await;
    track(dir.path());
    assert!(
        left_the_parent_chain(s.pid.unwrap(), daemon).await,
        "premise: daemon is outside the parent chain"
    );
    let registry = atlas_core::hygiene::Registry::at(dir.path());
    assert!(
        registry.roots().iter().any(|r| r.session == s.id),
        "the shell is registered"
    );
    assert!(
        registry.members().iter().any(|m| m.pid == daemon),
        "the daemon was recorded under the shell"
    );

    s.kill().await;
    assert!(
        gone_within(daemon, Duration::from_secs(10)).await,
        "daemon {daemon} survived the close"
    );
    assert!(
        registry.roots().iter().all(|r| r.session != s.id),
        "a closed terminal leaves no record"
    );
    assert!(registry.members().iter().all(|m| m.session != s.id));
}

// The shell ending by itself (`exit`, a crash) leaves the same orphan: reaped without a close.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_that_exits_by_itself_does_not_leave_its_daemon_behind() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("daemon.pid");
    // The shell exits only after `go`, created once the registry has seen its daemon: with
    // `sleep 1` a loaded machine could end the shell before `track` (#198).
    let go = dir.path().join("go-exit");
    let then = format!(
        "while [ ! -e '{}' ]; do sleep 0.01; done; exit 0",
        go.display()
    );
    let s = Session::spawn(shell_req(&daemonizing_shell(&marker, &then)), dir.path()).unwrap();
    let daemon = daemon_pid(&marker).await;
    track(dir.path());
    std::fs::write(&go, "").unwrap();
    until_exited(&s).await;
    assert!(
        gone_within(daemon, Duration::from_secs(10)).await,
        "daemon {daemon} outlived its terminal's shell"
    );
}

// Terminals own nothing of each other: closing one never touches another's processes.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_terminal_leaves_another_terminals_processes_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (ma, mb) = (dir.path().join("a.pid"), dir.path().join("b.pid"));
    let a = Session::spawn(
        shell_req(&daemonizing_shell(&ma, "exec sleep 300")),
        dir.path(),
    )
    .unwrap();
    let b = Session::spawn(
        shell_req(&daemonizing_shell(&mb, "exec sleep 300")),
        dir.path(),
    )
    .unwrap();
    let (da, db) = (daemon_pid(&ma).await, daemon_pid(&mb).await);
    track(dir.path());
    a.kill().await;
    assert!(gone_within(da, Duration::from_secs(10)).await);
    assert!(
        atlas_core::procs::start_time(db).is_some(),
        "the other terminal's daemon must survive"
    );
    b.kill().await;
    assert!(gone_within(db, Duration::from_secs(10)).await);
}

// ---- sessions that run as a crewmate's own user (issue #200), through a real spawn helper ----------

#[cfg(unix)]
mod isolated {
    use super::*;
    use crate::crew::spawn::testing::{USER, rig};

    fn req(r: &crate::crew::spawn::testing::Rig, fake: &str) -> PtyCreate {
        PtyCreate {
            cwd: r.work().to_string_lossy().into_owned(),
            argv: vec!["claude".into()],
            env: [("FAKE".to_string(), fake.to_string())].into(),
            cols: 100,
            rows: 30,
            scope: Default::default(),
            title: None,
            mode: Default::default(),
        }
    }

    async fn until_text(s: &Session, needle: &str) {
        for _ in 0..200 {
            if s.read_text(50).contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("never saw {needle:?}; saw {:?}", s.read_text(50));
    }

    // The branch under test: the child is started by the helper, hostd drives it through the master it
    // got back, and the session ends with the code the helper recorded.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_isolated_session_streams_output_takes_input_and_ends_with_the_childs_code() {
        let r = rig();
        r.engine("claude", "echo ready; read l; echo got=$l; exit 5");
        let scroll = tempfile::tempdir().unwrap();
        let s = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER))).unwrap();
        until_text(&s, "ready").await;
        s.write(b"hello\n").unwrap();
        until_text(&s, "got=hello").await;
        assert_eq!(until_exited(&s).await, Some(5));
        // The pid is the child's, as the helper reported it.
        assert!(s.pid.is_some_and(|p| p > 1));
    }

    // Incident class: hostd used to kill(pid) directly; for another uid that is EPERM, so a task could
    // not be cancelled. Signals go through the helper.
    #[tokio::test(flavor = "multi_thread")]
    async fn kill_ends_an_isolated_program_through_the_helper() {
        let r = rig();
        r.engine(
            "claude",
            "echo up; trap '' TERM; while :; do /bin/sleep 0.05; done",
        );
        let scroll = tempfile::tempdir().unwrap();
        let s = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER))).unwrap();
        until_text(&s, "up").await;
        // TERM is ignored by the program: kill() must escalate to KILL, as for a plain session.
        tokio::time::timeout(Duration::from_secs(15), s.kill())
            .await
            .expect("kill never returned");
        assert_eq!(until_exited(&s).await, Some(137));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn signal_reaches_an_isolated_program_and_unknown_names_are_refused() {
        let r = rig();
        r.engine(
            "claude",
            "trap 'exit 42' INT; echo up; while :; do /bin/sleep 0.05; done",
        );
        let scroll = tempfile::tempdir().unwrap();
        let s = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER))).unwrap();
        until_text(&s, "up").await;
        assert!(s.signal("USR1").is_err());
        s.signal("INT").unwrap();
        assert_eq!(until_exited(&s).await, Some(42));
        // Signalling an ended session is not an error (same as a plain one).
        s.signal("TERM").unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    async fn cwd_of_an_isolated_session_comes_from_the_helper() {
        let r = rig();
        r.engine("claude", "cd /tmp; echo up; /bin/sleep 30");
        let scroll = tempfile::tempdir().unwrap();
        let s = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER))).unwrap();
        until_text(&s, "up").await;
        let cwd = s.cwd_now().await.unwrap();
        assert_eq!(
            std::fs::canonicalize(cwd).unwrap(),
            std::fs::canonicalize("/tmp").unwrap()
        );
        s.kill().await;
    }

    // The helper refuses a spawn (an engine it does not have): Session::spawn_as reports it, no session.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_isolated_spawn_is_an_error_naming_the_user() {
        let r = rig(); // no engines installed
        let scroll = tempfile::tempdir().unwrap();
        let e = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER)))
            .err()
            .expect("spawn must fail")
            .to_string();
        assert!(e.contains(USER), "{e}");
    }

    // An isolated session is not hostd's child: the live upgrade must hand over which helper session
    // it is, or the successor could never signal it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_live_upgrade_keeps_an_isolated_session_signalable() {
        let r = rig();
        r.engine(
            "claude",
            "trap 'exit 9' TERM; echo up; while :; do /bin/sleep 0.05; done",
        );
        let scroll = tempfile::tempdir().unwrap();
        let s = Session::spawn_as(req(&r, ""), scroll.path(), Some((&r.client, USER))).unwrap();
        until_text(&s, "up").await;
        let (state, fd) = s.freeze().await.unwrap();
        let iso = state
            .iso
            .clone()
            .expect("the helper session travels with the state");
        assert_eq!(iso.user, USER);
        // The wire form is what the successor reads.
        let state: crate::handoff::SessionState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        let fd = fd.expect("a live session hands over its master");
        // SAFETY: freeze leaves the fd owned by `s`; the successor gets its own duplicate.
        let dup = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()
            .unwrap();
        let s2 = Session::adopt(state, Some(dup), scroll.path(), Some(&r.client)).unwrap();
        s2.signal("TERM").unwrap();
        assert_eq!(until_exited(&s2).await, Some(9));
    }
}

/// A session that stays quiet, with `old` already in its scrollback file.
#[cfg(unix)]
fn quiet_session_with_scrollback(dir: &std::path::Path, old: &[u8]) -> std::sync::Arc<Session> {
    use std::io::Write;
    let s = Session::spawn(shell_req("sleep 30"), dir).unwrap();
    let mut b = s.buf.lock();
    b.file.as_mut().unwrap().write_all(old).unwrap();
    b.file_len = old.len() as u64;
    drop(b);
    s
}

/// #301: past `FILE_MAX` the scrollback was rewritten with `fs::write`, truncating the file a cold restore
/// reads, under the output lock. A rewrite that fails (here: no write access to the directory, so no temp
/// file) must leave the previous `.bin` whole, keep appending to it, and ask again later.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_scrollback_rewrite_leaves_the_previous_file_intact() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let s = quiet_session_with_scrollback(dir.path(), b"old-output;");
    let bin = super::bin_path(dir.path(), &s.id);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::File::create(dir.path().join("probe")).is_ok() {
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        s.signal("KILL").unwrap();
        return; // root ignores directory modes: the failure cannot be injected
    }
    // Output that arrives while the rewrite runs sits in `trim`.
    s.buf.lock().trim = Some(b"mid;".to_vec());
    let epoch = s.buf.lock().epoch;
    s.rewrite_scrollback(b"snapshot", epoch);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    s.signal("KILL").unwrap();

    assert_eq!(std::fs::read(&bin).unwrap(), b"old-output;mid;");
    let b = s.buf.lock();
    assert!(b.trim.is_none(), "the rewrite is over");
    assert!(
        b.file_len < super::stream::FILE_MAX,
        "retry later, not on every chunk"
    );
}

/// A rewrite that lands replaces the file with the snapshot plus what arrived meanwhile (the old file
/// would have lost it with the unlinked inode), and later output keeps appending to the new file.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_scrollback_rewrite_keeps_the_output_that_arrived_during_it() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let s = quiet_session_with_scrollback(dir.path(), b"old-output;");
    let bin = super::bin_path(dir.path(), &s.id);
    s.buf.lock().trim = Some(b"mid;".to_vec());
    let epoch = s.buf.lock().epoch;
    s.rewrite_scrollback(b"snapshot;", epoch);
    s.buf
        .lock()
        .file
        .as_mut()
        .unwrap()
        .write_all(b"later;")
        .unwrap();
    s.signal("KILL").unwrap();
    assert_eq!(std::fs::read(&bin).unwrap(), b"snapshot;mid;later;");
    assert_eq!(s.buf.lock().file_len, "snapshot;mid;".len() as u64);

    // `clear` during a rewrite: the snapshot is stale and must not come back.
    let s = quiet_session_with_scrollback(dir.path(), b"old-output;");
    let bin = super::bin_path(dir.path(), &s.id);
    let epoch = s.buf.lock().epoch;
    s.buf.lock().trim = Some(Vec::new());
    s.clear();
    s.buf
        .lock()
        .trim
        .as_mut()
        .unwrap()
        .extend_from_slice(b"after;");
    s.rewrite_scrollback(b"stale-snapshot;", epoch);
    s.signal("KILL").unwrap();
    assert_eq!(std::fs::read(&bin).unwrap(), b"after;");
}

/// #301, end to end: output past `FILE_MAX` triggers the rewrite off the lock; the lock is free while it runs
/// and the file ends as the ring tail, never longer than the cap plus what arrived after.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn output_past_the_file_cap_trims_the_scrollback_to_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let s = quiet_session_with_scrollback(dir.path(), b"");
    let bin = super::bin_path(dir.path(), &s.id);
    s.buf.lock().file_len = super::stream::FILE_MAX;
    let _ = s.output(b"fresh-output");
    for _ in 0..200 {
        if s.buf.lock().trim.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    s.signal("KILL").unwrap();
    assert_eq!(std::fs::read(&bin).unwrap(), b"fresh-output");
    assert_eq!(s.buf.lock().file_len, "fresh-output".len() as u64);
}

/// Review of #413: `PtyKill` removes `<id>.bin` while a scrollback rewrite is in flight; the rewrite's rename
/// (and `open_append`) then recreated it and the next hostd start cold-restored a ghost terminal. The rewrite
/// is paused here between its write and its locked finish, which is where the kill lands.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn killing_a_terminal_during_a_scrollback_rewrite_leaves_no_files_behind() {
    let dir = tempfile::tempdir().unwrap();
    let s = quiet_session_with_scrollback(dir.path(), b"old-output;");
    let bin = super::bin_path(dir.path(), &s.id);
    let epoch = {
        let mut b = s.buf.lock();
        b.trim = Some(Vec::new());
        b.epoch
    };
    // The rewrite wrote its file and waits for the lock...
    let written = atlas_core::fs::write_atomic(&bin, b"snapshot;", atlas_core::fs::Mode::Default);
    // ...the kill removes the session's files...
    s.signal("KILL").unwrap();
    s.remove_files();
    // ...and the rewrite finishes.
    s.finish_rewrite("snapshot;".len(), epoch, written);
    assert!(!bin.exists(), "a ghost scrollback file came back");
    let _ = s.output(b"late output");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!bin.exists(), "output after the kill brought it back");
    assert!(!super::meta_path(dir.path(), &s.id).exists());
}

/// Review of #413: a slow rewrite (fsync under writeback pressure) must not buffer the whole stream. Only the
/// last `RING_MAX` bytes matter (cold restore reads the file's tail), so `trim` holds at most that, and what the
/// file ends with is still the end of the stream.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn output_during_a_slow_scrollback_rewrite_is_buffered_up_to_the_ring_size_and_ends_the_file()
{
    let dir = tempfile::tempdir().unwrap();
    let s = quiet_session_with_scrollback(dir.path(), b"");
    let bin = super::bin_path(dir.path(), &s.id);
    let epoch = {
        let mut b = s.buf.lock();
        b.trim = Some(Vec::new());
        b.epoch
    };
    let mut all = Vec::new();
    for i in 0..(3 * super::RING_MAX / 65_536) {
        let chunk = vec![b'a' + (i % 26) as u8; 65_536];
        all.extend_from_slice(&chunk);
        let _ = s.output(&chunk);
        let held = s.buf.lock().trim.as_ref().unwrap().len();
        assert!(held <= super::RING_MAX, "{held} bytes held");
    }
    // Trimmed in 64 KiB steps (not a memmove per chunk): a little under the cap, never a lot.
    let held = s.buf.lock().trim.as_ref().unwrap().len();
    assert!(held > super::RING_MAX - 2 * 65_536, "{held} bytes held");
    s.rewrite_scrollback(b"snapshot;", epoch);
    s.signal("KILL").unwrap();
    let file = std::fs::read(&bin).unwrap();
    assert_eq!(file[file.len() - held..], all[all.len() - held..]);
}
