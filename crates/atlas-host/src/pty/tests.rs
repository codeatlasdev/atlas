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
    let dir = std::env::temp_dir().join(format!("atlas-host-act-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (cmd, want) in [("printf hello", true), ("sleep 0.2", false)] {
        let s = Session::spawn(sh(cmd), &dir).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        s.attach(1, tx, false, false);
        let t0 = std::time::Instant::now();
        while s.exited().is_none() {
            assert!(t0.elapsed() < Duration::from_secs(10), "{cmd} never exited");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut got = false;
        while let Ok(l) = rx.try_recv() {
            got |= l.contains(r#""ev":"pty_activity""#);
        }
        assert_eq!(got, want, "{cmd}");
    }
    let _ = std::fs::remove_dir_all(&dir);
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
    let (tx, _rx) = mpsc::unbounded_channel();
    let info = s.attach(1, tx, true, true);
    let ev = crate::protocol::title_event(&s.id, &info).expect("a title event");
    assert_eq!(
        (&ev["ev"], &ev["pty"]),
        (&serde_json::json!("pty_title"), &serde_json::json!(s.id))
    );
    // An elevated Windows console (CI runners are) puts "Administrator: " before what the program set.
    let title = ev["title"].as_str().unwrap_or_default();
    assert_eq!(
        title.trim_start_matches("Administrator:").trim_start(),
        "built",
        "{title:?}"
    );
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
    // Far more output than the queues hold (ConPTY renders it, so Windows prints lines).
    let script = if cfg!(windows) {
        "for /L %i in (1,1,20000) do @echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    } else {
        "head -c 20000000 /dev/zero"
    };
    let s = Session::spawn(shell_req(script), &dir).unwrap();
    let (tx, _rx_never_read) = mpsc::channel(RAW_QUEUE);
    s.attach_raw(1, tx, false);
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
    install(probe_with(sh, home.path(), &fixtures::gui_env(home.path())));
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
