use std::time::Duration;

use tokio::sync::mpsc;

use super::title::TitleParser;
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
/// and none from a program that stays silent.
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

/// A raw reader that never reads must not freeze the program: it is dropped
/// after `RAW_STALL` and the output (far more than the queues hold) completes.
#[tokio::test(flavor = "multi_thread")]
async fn stuck_raw_reader_does_not_block_the_program() {
    let dir = std::env::temp_dir().join(format!("atlas-host-raw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let req = PtyCreate {
        cwd: "/".into(),
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "head -c 20000000 /dev/zero".into(),
        ],
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Default::default(),
        title: None,
        mode: Default::default(),
    };
    let s = Session::spawn(req, &dir).unwrap();
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
