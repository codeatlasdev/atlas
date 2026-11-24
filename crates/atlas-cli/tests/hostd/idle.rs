//! When the daemon exits by itself: idle with nothing to serve, and its directory disappearing.

use std::path::Path;
use std::time::Duration;

use atlas_core::clock::ManualClock;
use atlas_host::{HostClient, paths};
use serde_json::json;

use crate::common::*;

/// A daemon that runs in this process on a clock only the test moves, and the task that runs it (finished = it
/// exited). Returns once its first look at idleness is done, so each [`ManualClock::tick`] after it is a look of its
/// own. #596: with a real 1 s limit the daemon could idle out before a loaded runner had connected to it.
async fn daemon_that_idles(
    dir: &Path,
    limit: Duration,
) -> (
    HostClient,
    tokio::task::JoinHandle<anyhow::Result<()>>,
    ManualClock,
) {
    let d = dir.to_path_buf();
    let clock = ManualClock::new();
    let opts = atlas_host::server::ServeOpts {
        no_mobile_relay: true,
        mobile_loopback: true,
        idle: Some(limit),
        clock: clock.clock(),
        desktop: Some(atlas_core::desktop_ctl::Endpoint {
            token: dir.join("no-desktop.token"),
            socket: dir.join("no-desktop.sock"),
        }),
        ..Default::default()
    };
    let task = tokio::spawn(async move { atlas_host::server::serve_with(&d, opts).await });
    let c = connect_when_up(dir).await;
    clock.armed_past(1).await;
    (c, task, clock)
}

/// Let `limit` pass on the daemon's clock, one look at a time, up to `looks` times; true when it exited meanwhile.
/// Each look is awaited as the event (the daemon re-arms after handling it), never as a duration.
async fn idle_for(
    clock: &ManualClock,
    task: &mut tokio::task::JoinHandle<anyhow::Result<()>>,
    limit: Duration,
    looks: usize,
) -> bool {
    for _ in 0..looks {
        tokio::select! {
            exited = &mut *task => { exited.unwrap().unwrap(); return true; }
            () = clock.tick(limit) => {}
        }
    }
    task.is_finished()
}

// The real binary reads `ATLAS_HOSTD_IDLE_SECS` and leaves cleanly. Nothing connects first: a connect that a
// loaded runner made after the limit had passed was the old race, and the in-process test below owns "a client
// resets the clock" on a clock it moves.
#[tokio::test]
async fn idle_hostd_with_no_terminal_and_no_client_exits_and_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut d = spawn_with(dir, &[("ATLAS_HOSTD_IDLE_SECS", "1")]);
    wait_for("idle exit", || matches!(d.0.try_wait(), Ok(Some(_)))).await;
    assert!(d.0.try_wait().unwrap().unwrap().success());
    assert!(!paths::socket(dir).exists(), "socket removed");
    assert!(!paths::pid(dir).exists(), "pid file removed");
}

#[tokio::test]
async fn hostd_with_a_connected_client_or_a_live_pty_does_not_idle_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let limit = Duration::from_secs(4);
    let (c, mut task, clock) = daemon_that_idles(dir, limit).await;
    // Connected client: many limits past, still up.
    assert!(
        !idle_for(&clock, &mut task, limit, 5).await,
        "exited with a client connected"
    );

    // A running pty keeps it alive after the client leaves.
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "sleep 30"], "cols": 80, "rows": 24}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    drop(c);
    assert!(
        !idle_for(&clock, &mut task, limit, 5).await,
        "exited with a live pty"
    );

    // Once the pty is gone and the client with it, the clock runs and it exits and cleans up: `limit` after the
    // daemon has seen both go (a look or two later), however long that takes in real time.
    let c = HostClient::connect_dir(dir, "test").await.unwrap();
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
    drop(c);
    tokio::time::timeout(atlas_core::scaling::HANG, async {
        while !idle_for(&clock, &mut task, limit, 1).await {}
    })
    .await
    .expect("idle exit after the pty ended");
    assert!(!paths::socket(dir).exists(), "socket removed");
    assert!(!paths::pid(dir).exists(), "pid file removed");
}

#[tokio::test]
async fn hostd_exits_when_its_dir_disappears() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-dir");
    // A long idle limit, so only the missing dir can end it; a pty keeps it "busy".
    let mut d = spawn_with(&dir, &[("ATLAS_HOSTD_IDLE_SECS", "2")]);
    let c = connect_when_up(&dir).await;
    c.call(
        "pty_create",
        json!({"cwd": tmp.path(), "argv": ["sh", "-c", "sleep 30"], "cols": 80, "rows": 24}),
    )
    .await
    .unwrap();
    // The daemon is alive and may write a file (its session registry, a scrollback) while the
    // tree is removed, so one `remove_dir_all` can meet a new entry ("Directory not empty"),
    // exactly like `rm -rf` on a live dir. Remove until it is gone: what is tested is that hostd
    // then exits, not that it never writes.
    let t0 = std::time::Instant::now();
    while dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "could not remove {dir:?}"
        );
    }
    wait_for("exit after the dir was removed", || {
        matches!(d.0.try_wait(), Ok(Some(_)))
    })
    .await;
}
