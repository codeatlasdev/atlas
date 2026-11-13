//! Real processes and real files: a tagged tree with a daemonized grandchild, an owner that
//! dies, registered dirs that must not grow. Unix only where a shell one-liner builds the tree.

use super::*;
#[cfg(unix)]
use std::time::Instant;

fn registry() -> (tempfile::TempDir, Registry) {
    let d = tempfile::tempdir().unwrap();
    let r = Registry::at(d.path());
    (d, r)
}

fn proc_row(pid: u32, ppid: u32, start: u64) -> Proc {
    Proc {
        pid,
        ppid,
        start,
        name: format!("p{pid}"),
    }
}

fn root(session: &str, owner: Owner, pid: u32, start: u64) -> Root {
    Root {
        session: session.into(),
        owner,
        pid,
        start,
        kind: Kind::Agent,
        label: "agent".into(),
        project: Some("/p".into()),
        started_at: 1,
    }
}

const DEAD: Owner = Owner {
    pid: 0x7fff_fff0,
    start: 42,
};

#[test]
fn a_tag_of_a_session_with_a_dead_owner_is_an_orphan_and_a_live_owners_is_not() {
    let me = Owner::current();
    let roots = vec![root("alive", me, 10, 5), root("dead", DEAD, 20, 5)];
    let tagged = vec![
        (proc_row(11, 1, 6), "alive".to_string()),
        (proc_row(21, 1, 6), "dead".to_string()),
        // A tag nobody registered: unknown session, nobody to wait for.
        (proc_row(31, 1, 6), "ghost".to_string()),
        (proc_row(21, 1, 6), "dead".to_string()),
    ];
    let scan = classify(&tagged, &roots, &[], &[], 1_000_000);
    assert_eq!(scan.live, 1);
    let pids: Vec<u32> = scan.orphans.iter().map(|o| o.pid).collect();
    assert_eq!(pids, vec![21, 31], "deduplicated, sorted");
}

// A daemon spawns the child, THEN registers it: a sweep in that window must not end a terminal
// that is still being created. Only a tag nobody ever registered gets the grace.
#[test]
fn a_tag_nobody_registered_is_an_orphan_only_after_the_registration_grace() {
    let now = 1_000_000;
    let fresh = (proc_row(1, 1, now - 5), "new".to_string());
    let old = (
        proc_row(2, 1, now - REGISTRATION_GRACE_SECS),
        "old".to_string(),
    );
    let scan = classify(&[fresh, old], &[], &[], &[], now);
    assert_eq!(
        scan.orphans.iter().map(|o| o.pid).collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(scan.live, 1);
    // A REGISTERED session whose owner died is an orphan at once, however young the process.
    let roots = vec![root("dead", DEAD, 9, 1)];
    let young = (proc_row(3, 1, now - 1), "dead".to_string());
    assert_eq!(classify(&[young], &roots, &[], &[], now).orphans.len(), 1);
}

#[test]
fn an_owner_is_a_stranger_when_its_pid_was_reused_by_a_process_that_started_later() {
    let mut me = Owner::current();
    assert!(me.alive());
    me.start += 1000;
    assert!(!me.alive(), "same pid, different start: pid reuse");
    assert!(!DEAD.alive());
}

#[cfg(unix)]
#[test]
fn a_registered_root_whose_owner_died_brings_its_parent_chain_in_even_without_a_readable_tag() {
    // Windows' case: no environment to read, only the parent chain of the registered root.
    let mut owner_proc = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let start = procs::start_time(owner_proc.id()).unwrap();
    let dead_root = Owner {
        pid: owner_proc.id(),
        start,
    };
    let _ = owner_proc.kill();
    let _ = owner_proc.wait();
    // The "root" is this very test process (alive), its tree is our children.
    let mut kid = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let me = Owner::current();
    let roots = vec![root("s", dead_root, me.pid, me.start)];
    let table = procs::table();
    let scan = classify(&[], &roots, &[], &table, now_secs());
    let _ = kid.kill();
    let _ = kid.wait();
    assert!(
        scan.orphans.iter().any(|o| o.pid == kid.id()),
        "child of the registered root is an orphan: {:?}",
        scan.orphans
    );
    assert!(
        scan.orphans.iter().all(|o| o.pid != std::process::id()),
        "never ourselves"
    );
}

#[cfg(unix)]
fn wait_gone(pids: &[u32], within: Duration) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        if !pids
            .iter()
            .any(|&p| procs::start_time(p).is_some_and(|_| !zombie(p)))
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Run `f` and fail the test, never hang, if it does not finish within `within` (a `child.wait()`
/// on a process that was never signalled would block the suite for good).
#[cfg(unix)]
fn bounded<T: Send + 'static>(within: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(within)
        .expect("timed out: something waited on a process that never ended")
}

/// Wait for `child` to exit, bounded; kills it on timeout so nothing leaks.
#[cfg(unix)]
fn reaped(child: &mut std::process::Child, within: Duration) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

/// The root `tree_root` is registered under `session` and `track` has seen its tree, including
/// the daemonized grandchild while it was still below the shell (the macOS discovery path).
#[cfg(unix)]
fn tracked_tree(reg: &Registry, session: &str, owner: Owner) -> (std::process::Child, u32) {
    // The shell waits for its subshell, which has not yet daemonized away when `track` runs: the
    // script holds the grandchild under the shell for a moment, then lets go (the `&` subshell
    // exits, the sleep is re-parented to launchd/init).
    let marker = tempfile::NamedTempFile::new().unwrap();
    let path = marker.path().to_path_buf();
    // The subshell lingers 1.5 s: the `sleep` is seen below the shell, then (the subshell exits)
    // re-parented to init. A process that detaches before ever being seen is the documented
    // macOS window, not something a test can make deterministic.
    let script = format!(
        "( sleep 300 & echo $! > {}; sleep 1.5 ) & wait; exec sleep 300",
        path.display()
    );
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(&script);
    tag_command(&mut cmd, session);
    let child = cmd.spawn().unwrap();
    let start = procs::start_time(child.id()).unwrap();
    reg.add_root(root(session, owner, child.id(), start))
        .unwrap();
    let mut grandchild = 0u32;
    for _ in 0..200 {
        // Record while the tree exists: the grandchild is seen under the shell if `track` runs
        // before the subshell exits; either way it is recorded as soon as it is below a root.
        reg.track().unwrap();
        if let Ok(s) = std::fs::read_to_string(&path)
            && let Ok(p) = s.trim().parse()
        {
            grandchild = p;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(grandchild > 1, "grandchild never started");
    // Wait until it left the parent chain: from here only the record names it (macOS, Windows).
    let end = Instant::now() + Duration::from_secs(10);
    while procs::tree(child.id(), &procs::table()).contains(&grandchild) {
        assert!(
            Instant::now() < end,
            "the grandchild never left the parent chain"
        );
        reg.track().unwrap();
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        reg.members().iter().any(|m| m.pid == grandchild),
        "premise: it was recorded while below the shell"
    );
    (child, grandchild)
}

// Acceptance (issue #127): a tree with a daemonizing grandchild, the owner dies, no process of
// the session is left within 10 s. On macOS the tag cannot be read, so this runs on what the
// owner recorded while it lived.
#[cfg(unix)]
#[test]
fn killing_the_owner_leaves_no_session_process_even_a_daemonized_grandchild_within_10s() {
    let (_d, reg) = registry();
    let session = new_session_id();
    let mut owner_proc = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let owner = Owner {
        pid: owner_proc.id(),
        start: procs::start_time(owner_proc.id()).unwrap(),
    };
    let (mut tree_root, grandchild) = tracked_tree(&reg, &session, owner);
    // Owner alive: nothing is an orphan.
    assert!(scan(&reg).orphans.iter().all(|o| o.session != session));
    assert!(!reg.has_orphaned_root());

    let _ = owner_proc.kill();
    let _ = owner_proc.wait();

    let started = Instant::now();
    assert!(reg.has_orphaned_root(), "the dead owner must be noticed");
    let s = sweep(&reg, false);
    assert!(
        s.orphans.iter().any(|o| o.pid == grandchild),
        "sweep missed the grandchild: {:?}",
        s.orphans
    );
    assert!(
        s.survivors.is_empty(),
        "survived SIGKILL: {:?}",
        s.survivors
    );
    assert!(
        reaped(&mut tree_root, Duration::from_secs(10)),
        "the shell was never ended"
    );
    assert!(
        wait_gone(&[tree_root.id(), grandchild], Duration::from_secs(10)),
        "session processes left"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        reg.members().iter().all(|m| m.session != session),
        "records dropped"
    );
}

// The session itself closing (hostd closes a terminal, `atlas dev` shuts down) reaps the same
// tree, without waiting for the owner to die; another session's process is left alone.
#[cfg(unix)]
#[test]
fn reap_session_ends_the_whole_tree_including_the_daemonized_grandchild() {
    let (_d, reg) = registry();
    let session = new_session_id();
    let (mut tree_root, grandchild) = tracked_tree(&reg, &session, Owner::current());
    let other = new_session_id();
    let mut bystander = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let bstart = procs::start_time(bystander.id()).unwrap();
    reg.add_root(root(&other, Owner::current(), bystander.id(), bstart))
        .unwrap();
    reg.track().unwrap();

    let left = reg.reap_session(&session, Duration::from_secs(2));
    assert!(left.is_empty(), "{left:?}");
    assert!(reaped(&mut tree_root, Duration::from_secs(10)));
    assert!(wait_gone(
        &[tree_root.id(), grandchild],
        Duration::from_secs(10)
    ));
    assert!(
        crate::sys::pid_alive(bystander.id()),
        "another session's process must not be touched"
    );
    assert!(reg.roots().iter().all(|r| r.session != session));
    assert!(
        reg.roots().iter().any(|r| r.session == other),
        "the other session keeps its record"
    );
    let _ = bystander.kill();
    let _ = bystander.wait();
}

// A process that traps SIGTERM only dies at SIGKILL: the grace must elapse first, then it goes.
#[cfg(unix)]
#[test]
fn a_process_that_ignores_sigterm_is_killed_after_the_grace() {
    let (_d, reg) = registry();
    let session = new_session_id();
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 1; done")
        .spawn()
        .unwrap();
    let start = procs::start_time(child.id()).unwrap();
    reg.add_root(root(&session, Owner::current(), child.id(), start))
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let r = reg.clone();
    let s = session.clone();
    let left = bounded(Duration::from_secs(30), move || {
        r.reap_session(&s, Duration::from_millis(500))
    });
    assert!(left.is_empty(), "{left:?}");
    assert!(
        reaped(&mut child, Duration::from_secs(5)),
        "SIGKILL never landed"
    );
    assert!(
        t.elapsed() >= Duration::from_millis(500),
        "SIGTERM was ignored: the grace must elapse"
    );
    assert!(t.elapsed() < Duration::from_secs(10));
}

// Linux reads the tag of every process: a daemonized grandchild is found with NO prior tracking.
#[cfg(target_os = "linux")]
#[test]
fn on_linux_a_daemonized_grandchild_is_found_by_its_tag_without_ever_being_tracked() {
    let (_d, reg) = registry();
    let session = new_session_id();
    let marker = tempfile::NamedTempFile::new().unwrap();
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(format!(
        "( sleep 300 & echo $! > {} ) & wait; exec sleep 300",
        marker.path().display()
    ));
    tag_command(&mut cmd, &session);
    let mut child = cmd.spawn().unwrap();
    let start = procs::start_time(child.id()).unwrap();
    reg.add_root(root(&session, Owner::current(), child.id(), start))
        .unwrap();
    let mut gc = 0u32;
    for _ in 0..200 {
        if let Ok(s) = std::fs::read_to_string(marker.path())
            && let Ok(p) = s.trim().parse()
        {
            gc = p;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gc > 1);
    assert!(
        reg.members().iter().all(|m| m.pid != gc),
        "premise: never tracked"
    );
    reg.reap_session(&session, Duration::from_secs(2));
    assert!(reaped(&mut child, Duration::from_secs(10)));
    assert!(wait_gone(&[gc], Duration::from_secs(10)));
}

// A member is only ever signalled while it is still the process that was recorded.
#[cfg(unix)]
#[test]
fn a_recorded_member_whose_pid_was_reused_is_never_signalled() {
    let (_d, reg) = registry();
    let mut bystander = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let real_start = procs::start_time(bystander.id()).unwrap();
    // The record claims an earlier incarnation of this pid.
    reg.update(|s| {
        s.members.push(Member {
            session: "gone".into(),
            pid: bystander.id(),
            start: real_start.saturating_sub(5000),
            name: "sleep".into(),
        })
    })
    .unwrap();
    assert!(!reg.has_orphaned_root(), "a stranger is not an orphan");
    assert!(scan(&reg).orphans.is_empty());
    assert!(
        reg.reap_session("gone", Duration::from_millis(100))
            .is_empty()
    );
    assert!(
        crate::sys::pid_alive(bystander.id()),
        "the reused pid was signalled"
    );
    let _ = bystander.kill();
    let _ = bystander.wait();
}

// `track` records a live root's whole tree once, keeps it across calls, and forgets what exited.
#[cfg(unix)]
#[test]
fn track_records_the_tree_of_live_roots_and_forgets_members_that_exited() {
    let (_d, reg) = registry();
    let mut shell = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 300 & wait")
        .spawn()
        .unwrap();
    let start = procs::start_time(shell.id()).unwrap();
    reg.add_root(root("s", Owner::current(), shell.id(), start))
        .unwrap();
    let mut got = Vec::new();
    for _ in 0..200 {
        reg.track().unwrap();
        got = reg.members();
        if got.iter().any(|m| m.name.contains("sleep")) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        got.iter().any(|m| m.pid == shell.id()),
        "the root itself: {got:?}"
    );
    assert!(
        got.iter().any(|m| m.name.contains("sleep")),
        "its child: {got:?}"
    );
    reg.track().unwrap();
    assert_eq!(
        reg.members().len(),
        got.len(),
        "no duplicates on a second pass"
    );
    reg.reap_session("s", Duration::from_secs(2));
    let _ = shell.wait();
    reg.track().unwrap();
    assert!(reg.members().is_empty());
}

#[test]
fn a_reused_pid_is_never_signalled() {
    // A record naming our own pid with a start time that is not ours: it is a stranger.
    let me = std::process::id();
    let o = Orphan {
        pid: me,
        start: procs::start_time(me).unwrap() + 999,
        name: "x".into(),
        session: "s".into(),
        kind: None,
        label: None,
        project: None,
    };
    // Would kill the test runner if the start check were missing.
    assert!(reap(&[o], Duration::from_millis(100)).is_empty());
    assert!(crate::sys::pid_alive(me));
}

#[test]
fn the_registry_survives_concurrent_writers_and_replaces_a_repeated_root_or_dir() {
    let (d, reg) = registry();
    let threads: Vec<_> = (0..8u32)
        .map(|i| {
            let r = reg.clone();
            std::thread::spawn(move || {
                for j in 0..10u32 {
                    r.add_root(root(
                        &format!("s{i}"),
                        Owner::current(),
                        1000 + i * 100 + j,
                        1,
                    ))
                    .unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(reg.roots().len(), 80, "no lost update under the lock");
    // The same (pid, start) again replaces, never duplicates.
    reg.add_root(root("s0", Owner::current(), 1000, 1)).unwrap();
    assert_eq!(reg.roots().len(), 80);
    let dir = Dir {
        path: d.path().join("t"),
        cap_bytes: 10,
        kind: DirKind::Temp,
        session: None,
        owner: None,
        project: None,
        created_at: 1,
    };
    for _ in 0..5 {
        reg.add_dir(dir.clone()).unwrap();
    }
    assert_eq!(reg.dirs().len(), 1);
    reg.remove_session("s0").unwrap();
    assert!(reg.roots().iter().all(|r| r.session != "s0"));
}

fn fill(dir: &Path, name: &str, bytes: usize, age_secs: u64) {
    let p = dir.join(name);
    std::fs::write(&p, vec![b'x'; bytes]).unwrap();
    let t = SystemTime::now() - Duration::from_secs(age_secs);
    std::fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

#[test]
fn a_registered_dir_over_its_cap_loses_its_oldest_entries_first_and_stops_at_the_cap() {
    let (d, reg) = registry();
    let tmp = d.path().join("agent-tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    fill(&tmp, "old", 400, 300);
    fill(&tmp, "mid", 400, 200);
    fill(&tmp, "new", 400, 100);
    reg.add_dir(Dir {
        path: tmp.clone(),
        cap_bytes: 800,
        kind: DirKind::Temp,
        session: None,
        owner: None,
        project: None,
        created_at: 1,
    })
    .unwrap();
    let dry = enforce_caps(&reg, true);
    assert_eq!((dry.entries_removed, dry.bytes), (1, 400));
    assert!(tmp.join("old").exists(), "dry run deletes nothing");
    let freed = enforce_caps(&reg, false);
    assert_eq!(freed.entries_removed, 1);
    assert!(!tmp.join("old").exists());
    assert!(tmp.join("mid").exists() && tmp.join("new").exists());
    assert_eq!(enforce_caps(&reg, false), Freed::default(), "idempotent");
}

// Acceptance (issue #127): repeated runs do not grow registered dirs past their cap.
#[test]
fn repeated_runs_never_grow_a_registered_dir_past_its_cap() {
    let (d, reg) = registry();
    let tmp = d.path().join("browser-profiles");
    std::fs::create_dir_all(&tmp).unwrap();
    let cap = 1000u64;
    for run in 0..25usize {
        // Each "run" registers (idempotently), writes a trace and sweeps.
        reg.add_dir(Dir {
            path: tmp.clone(),
            cap_bytes: cap,
            kind: DirKind::BrowserProfile,
            session: None,
            owner: None,
            project: None,
            created_at: 1,
        })
        .unwrap();
        fill(&tmp, &format!("run-{run}"), 300, (100 - run) as u64);
        enforce_caps(&reg, false);
        assert!(
            dir_size(&tmp) <= cap,
            "run {run}: {} > {cap}",
            dir_size(&tmp)
        );
    }
    assert_eq!(reg.dirs().len(), 1, "the registry does not grow either");
}

#[test]
fn a_dir_of_a_dead_session_is_removed_whole_and_a_live_sessions_is_kept() {
    let (d, reg) = registry();
    let (dead, live) = (d.path().join("dead"), d.path().join("live"));
    for p in [&dead, &live] {
        std::fs::create_dir_all(p).unwrap();
        std::fs::write(p.join("f"), b"1234").unwrap();
    }
    let me = Owner::current();
    reg.add_root(root("live", me, me.pid, me.start)).unwrap();
    for (p, s, o) in [(&dead, "dead", DEAD), (&live, "live", me)] {
        reg.add_dir(Dir {
            path: p.clone(),
            cap_bytes: DEFAULT_DIR_CAP,
            kind: DirKind::Temp,
            session: Some(s.into()),
            owner: Some(o),
            project: None,
            created_at: 1,
        })
        .unwrap();
    }
    let freed = enforce_caps(&reg, false);
    assert_eq!(freed.dirs_removed, 1);
    assert!(!dead.exists());
    assert!(live.exists());
    assert_eq!(reg.dirs().len(), 1, "the dead dir left the registry");
}

#[test]
fn a_path_outside_the_registry_is_never_touched() {
    let (d, reg) = registry();
    let user = d.path().join("user-files");
    std::fs::create_dir_all(&user).unwrap();
    fill(&user, "thesis.tex", 5000, 9999);
    enforce_caps(&reg, false);
    assert!(user.join("thesis.tex").exists());
}

#[test]
fn trim_logs_drops_old_rotated_generations_and_never_the_live_log() {
    let d = tempfile::tempdir().unwrap();
    fill(d.path(), "api.log", 500, 10);
    fill(d.path(), "api.prev.log", 500, 500);
    fill(d.path(), "web.prev.log", 500, 400);
    fill(d.path(), "web.log", 500, 5);
    let freed = trim_logs(d.path(), 1500, false);
    assert_eq!(freed.entries_removed, 1);
    assert!(
        !d.path().join("api.prev.log").exists(),
        "oldest generation goes first"
    );
    assert!(d.path().join("web.prev.log").exists());
    // Far below the cap: still only .prev logs are ever candidates.
    let freed = trim_logs(d.path(), 0, false);
    assert_eq!(freed.entries_removed, 1);
    assert!(d.path().join("api.log").exists() && d.path().join("web.log").exists());
}

#[test]
fn project_disk_reports_the_heavy_directories_and_worktrees() {
    let d = tempfile::tempdir().unwrap();
    for (sub, n) in [("target", 100), ("node_modules/x", 200), (".logs", 30)] {
        let p = d.path().join(sub);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("f"), vec![0u8; n]).unwrap();
    }
    let wt = d.path().join(".atlas/worktrees/feat-a");
    std::fs::create_dir_all(&wt).unwrap();
    std::fs::write(wt.join("f"), vec![0u8; 50]).unwrap();
    let r = project_disk(d.path());
    assert_eq!((r.target, r.node_modules, r.logs), (100, 200, 30));
    assert_eq!(r.worktrees, vec![("feat-a".to_string(), 50)]);
}

#[test]
fn session_ids_are_unique_and_hex() {
    let a = new_session_id();
    assert_eq!(a.len(), 16);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(a, new_session_id());
}

#[test]
fn a_registry_file_that_is_corrupt_reads_as_empty_instead_of_failing_every_caller() {
    let (_d, reg) = registry();
    std::fs::create_dir_all(&reg.dir).unwrap();
    std::fs::write(reg.file(), b"{ not json").unwrap();
    assert!(reg.roots().is_empty());
    reg.add_root(root("s", Owner::current(), 5, 5)).unwrap();
    assert_eq!(reg.roots().len(), 1);
}

// Incident class: hostd's live upgrade hands its terminals to a successor. The registry still
// named the retiring daemon as owner, so the successor's first sweep would have taken every live
// terminal for an orphan. `set_owner` moves them to the new daemon.
#[cfg(unix)]
#[test]
fn a_session_handed_to_a_new_owner_is_not_an_orphan_when_the_old_owner_dies() {
    let (_d, reg) = registry();
    let session = new_session_id();
    let mut old = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let old_owner = Owner {
        pid: old.id(),
        start: procs::start_time(old.id()).unwrap(),
    };
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("300");
    tag_command(&mut cmd, &session);
    let mut term = cmd.spawn().unwrap();
    let term_start = procs::start_time(term.id()).unwrap();
    reg.add_root(root(&session, old_owner, term.id(), term_start))
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    reg.set_owner(std::slice::from_ref(&session), Owner::current())
        .unwrap();
    let _ = old.kill();
    let _ = old.wait();

    let s = sweep(&reg, false);
    assert!(
        s.orphans.iter().all(|o| o.session != session),
        "{:?}",
        s.orphans
    );
    assert!(
        crate::sys::pid_alive(term.id()),
        "the handed-off terminal was killed"
    );
    let _ = term.kill();
    let _ = term.wait();
}

#[cfg(unix)]
#[test]
fn a_dry_run_sweep_reports_the_orphans_and_the_caps_but_changes_nothing() {
    let (d, reg) = registry();
    let session = new_session_id();
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("300");
    tag_command(&mut cmd, &session);
    let mut orphan = cmd.spawn().unwrap();
    let start = procs::start_time(orphan.id()).unwrap();
    reg.add_root(root(&session, DEAD, orphan.id(), start))
        .unwrap();
    let tmp = d.path().join("t");
    std::fs::create_dir_all(&tmp).unwrap();
    fill(&tmp, "a", 500, 10);
    reg.add_dir(Dir {
        path: tmp.clone(),
        cap_bytes: 100,
        kind: DirKind::Temp,
        session: None,
        owner: None,
        project: None,
        created_at: 1,
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let dry = sweep(&reg, true);
    assert!(!dry.applied);
    assert!(dry.orphans.iter().any(|o| o.pid == orphan.id()));
    assert_eq!(dry.freed.bytes, 500);
    assert!(
        crate::sys::pid_alive(orphan.id()),
        "a dry run kills nothing"
    );
    assert!(tmp.join("a").exists(), "a dry run deletes nothing");
    assert_eq!(reg.roots().len(), 1, "a dry run keeps the registry");

    let real = sweep(&reg, false);
    assert!(real.applied && real.survivors.is_empty());
    let _ = orphan.wait();
    assert!(!tmp.join("a").exists());
    assert!(reg.roots().is_empty(), "the dead root left the registry");
}

fn write_projects(base: &Path, roots: &[(&Path, Option<&str>, Option<&str>)]) {
    let list: Vec<serde_json::Value> = roots
        .iter()
        .map(|(r, parent, host)| {
            serde_json::json!({
                "id": r.file_name().unwrap().to_string_lossy(),
                "root": r,
                "parent": parent,
                "host": host,
            })
        })
        .collect();
    std::fs::create_dir_all(base.join("desktop")).unwrap();
    std::fs::write(
        base.join("desktop/projects.json"),
        serde_json::to_vec(&list).unwrap(),
    )
    .unwrap();
}

#[test]
fn projects_come_from_the_desktop_registry_skipping_ssh_hosts_and_missing_folders() {
    let d = tempfile::tempdir().unwrap();
    let (a, wt, gone) = (
        d.path().join("a"),
        d.path().join("a-wt"),
        d.path().join("gone"),
    );
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&wt).unwrap();
    write_projects(
        d.path(),
        &[
            (&a, None, None),
            (&wt, Some("a"), None),
            (&gone, None, None),
            (&a, None, Some("ssh:x")),
        ],
    );
    let p = projects_in(d.path());
    assert_eq!(p.len(), 2);
    assert!(!p[0].is_worktree && p[1].is_worktree);
    assert!(projects_in(&d.path().join("none")).is_empty());
}

// The one thing `--fix` must never do: touch what the user owns. `target/`, `node_modules`,
// worktrees and the live logs are reported, never removed.
#[test]
fn fix_frees_only_what_atlas_owns_and_reports_the_rest_untouched() {
    let d = tempfile::tempdir().unwrap();
    let base = d.path().join("home");
    let proj = d.path().join("proj");
    for sub in ["target", "node_modules", ".logs", ".atlas/worktrees/feat"] {
        std::fs::create_dir_all(proj.join(sub)).unwrap();
    }
    std::fs::write(proj.join("target/big"), vec![0u8; 4000]).unwrap();
    std::fs::write(proj.join("node_modules/dep"), vec![0u8; 3000]).unwrap();
    std::fs::write(proj.join(".atlas/worktrees/feat/code.rs"), b"fn main(){}").unwrap();
    std::fs::write(proj.join(".logs/api.log"), vec![0u8; 100]).unwrap();
    std::fs::write(proj.join(".logs/api.prev.log"), vec![0u8; 100]).unwrap();
    write_projects(&base, &[(&proj, None, None)]);

    let reg = Registry::at(&base);
    // A dead session's temp dir: Atlas's own, goes.
    let dead_tmp = d.path().join("agent-tmp");
    std::fs::create_dir_all(&dead_tmp).unwrap();
    std::fs::write(dead_tmp.join("trace"), vec![0u8; 500]).unwrap();
    reg.add_dir(Dir {
        path: dead_tmp.clone(),
        cap_bytes: DEFAULT_DIR_CAP,
        kind: DirKind::Temp,
        session: Some("dead".into()),
        owner: Some(DEAD),
        project: None,
        created_at: 1,
    })
    .unwrap();

    let r = report(&reg, &base);
    assert_eq!(r.projects.len(), 1);
    assert_eq!(
        (r.projects[0].target, r.projects[0].node_modules),
        (4000, 3000)
    );
    assert!(r.dirs[0].orphaned);
    assert_eq!(r.freeable.dirs_removed, 1);
    assert!(dead_tmp.exists(), "report changes nothing");

    let f = fix(&reg, &base);
    assert_eq!(f.freed.dirs_removed, 1);
    assert!(!dead_tmp.exists());
    // Under the logs cap nothing of the project is removed.
    assert!(proj.join(".logs/api.prev.log").exists());
    for keep in [
        "target/big",
        "node_modules/dep",
        ".atlas/worktrees/feat/code.rs",
        ".logs/api.log",
    ] {
        assert!(proj.join(keep).exists(), "{keep} is the user's");
    }
}

// Incident (2026-11-13): the Mac froze while Spotlight indexed several fresh worktree `target/`
// trees (~100k files each). Atlas-created heavy dirs get the never-index marker; the marker is
// the only thing `--fix` writes for it.
#[cfg(target_os = "macos")]
mod spotlight {
    use super::*;

    #[test]
    fn marking_writes_the_marker_once_only_in_an_existing_dir_and_never_touches_contents() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("keep"), b"data").unwrap();
        assert!(mark_never_index(d.path()));
        assert!(d.path().join(NEVER_INDEX).is_file());
        assert_eq!(std::fs::read(d.path().join("keep")).unwrap(), b"data");
        assert!(
            !mark_never_index(d.path()),
            "already marked: nothing written"
        );
        assert!(
            !mark_never_index(&d.path().join("missing")),
            "a missing dir is not created"
        );
        assert!(!d.path().join("missing").exists());
    }

    #[test]
    fn a_marker_on_an_ancestor_covers_every_checkout_below_it() {
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("worktrees");
        let target = ws.join("repo/feat/target");
        std::fs::create_dir_all(&target).unwrap();
        assert!(spotlight_indexes(&target));
        assert!(mark_never_index(&ws));
        assert!(
            !spotlight_indexes(&target),
            "the worktree root's marker covers target/"
        );
        assert!(
            spotlight_indexes(d.path()),
            "the marker does not cover its parent"
        );
    }

    #[test]
    fn registering_a_dir_marks_it_before_anything_is_written_into_it() {
        let (d, reg) = registry();
        let cache = d.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        reg.add_dir(Dir {
            path: cache.clone(),
            cap_bytes: DEFAULT_DIR_CAP,
            kind: DirKind::Cache,
            session: None,
            owner: None,
            project: None,
            created_at: 1,
        })
        .unwrap();
        assert!(cache.join(NEVER_INDEX).is_file());
    }

    #[test]
    fn the_report_lists_indexed_heavy_dirs_and_fix_adds_markers_only() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().join("home");
        let proj = d.path().join("proj");
        std::fs::create_dir_all(proj.join("target")).unwrap();
        std::fs::create_dir_all(proj.join("node_modules")).unwrap();
        std::fs::create_dir_all(proj.join(".atlas/worktrees/feat/target")).unwrap();
        std::fs::write(proj.join("target/big"), vec![0u8; 100]).unwrap();
        write_projects(&base, &[(&proj, None, None)]);
        let reg = Registry::at(&base);

        let r = report(&reg, &base);
        let mut got = r.projects[0].indexed.clone();
        got.sort();
        let mut want: Vec<String> = ["target", "node_modules", ".atlas/worktrees/feat/target"]
            .iter()
            .map(|s| proj.join(s).to_string_lossy().into_owned())
            .collect();
        want.sort();
        assert_eq!(got, want);
        assert!(
            !proj.join("target").join(NEVER_INDEX).exists(),
            "report changes nothing"
        );

        let f = fix(&reg, &base);
        assert_eq!(f.marked.len(), 3);
        for h in ["target", "node_modules", ".atlas/worktrees/feat/target"] {
            assert!(proj.join(h).join(NEVER_INDEX).is_file(), "{h}");
        }
        assert_eq!(
            std::fs::read(proj.join("target/big")).unwrap().len(),
            100,
            "no data touched"
        );
        assert!(
            report(&reg, &base).projects[0].indexed.is_empty(),
            "nothing left to fix"
        );
        assert!(fix(&reg, &base).marked.is_empty(), "idempotent");
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn off_macos_nothing_is_marked_and_nothing_is_reported_as_indexed() {
    let d = tempfile::tempdir().unwrap();
    assert!(!mark_never_index(d.path()));
    assert!(!d.path().join(NEVER_INDEX).exists());
    assert!(!spotlight_indexes(d.path()));
}

// The poll hostd runs every few seconds: cheap, and true only while a registered tree runs under
// an owner that is gone (the signal to run the full sweep within the 10 s budget of #127).
#[cfg(unix)]
#[test]
fn an_orphaned_root_is_noticed_only_while_its_tree_runs_under_a_dead_owner() {
    let (_d, reg) = registry();
    assert!(!reg.has_orphaned_root(), "empty registry");
    let mut child = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let start = procs::start_time(child.id()).unwrap();
    let me = Owner::current();

    reg.add_root(root("live", me, child.id(), start)).unwrap();
    assert!(!reg.has_orphaned_root(), "owner alive");

    reg.add_root(root("dead", DEAD, child.id(), start)).unwrap();
    assert!(reg.has_orphaned_root(), "owner gone, tree running");

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        !reg.has_orphaned_root(),
        "owner gone but nothing left to reap"
    );
}

// Incident-in-waiting (#127 review): an agent instance's directory is its HOME and daemon dir. A
// cap trim of a live instance would delete files the app is using, so an `Instance` dir is only
// ever removed whole, once its owner is gone.
#[test]
fn an_instance_dir_is_never_trimmed_while_its_owner_lives_and_goes_whole_when_it_dies() {
    let (d, reg) = registry();
    let inst = d.path().join("atlas-agent-a1");
    std::fs::create_dir_all(inst.join("home")).unwrap();
    fill(&inst.join("home"), "settings.json", 5000, 100);
    fill(&inst, "agent.log", 5000, 50);
    let add = |owner: Owner| {
        reg.add_dir(Dir {
            path: inst.clone(),
            cap_bytes: 100,
            kind: DirKind::Instance,
            session: Some("inst".into()),
            owner: Some(owner),
            project: None,
            created_at: 1,
        })
        .unwrap()
    };

    add(Owner::current());
    let f = enforce_caps(&reg, false);
    assert_eq!((f.entries_removed, f.dirs_removed), (0, 0), "{f:?}");
    assert!(inst.join("home/settings.json").exists());
    assert!(inst.join("agent.log").exists());

    add(DEAD);
    let f = enforce_caps(&reg, false);
    assert_eq!(f.dirs_removed, 1);
    assert!(!inst.exists());
}

// A long-lived registered dir: its marker ends up the OLDEST entry, and a cap trim removes oldest
// first. The marker is not scratch; losing it would let Spotlight index the dir again.
#[test]
fn a_cap_trim_never_removes_the_never_index_marker() {
    let (d, reg) = registry();
    let tmp = d.path().join("cache");
    std::fs::create_dir_all(&tmp).unwrap();
    fill(&tmp, NEVER_INDEX, 0, 9999);
    fill(&tmp, "a", 400, 200);
    fill(&tmp, "b", 400, 100);
    reg.add_dir(Dir {
        path: tmp.clone(),
        cap_bytes: 500,
        kind: DirKind::Cache,
        session: None,
        owner: None,
        project: None,
        created_at: 1,
    })
    .unwrap();
    let freed = enforce_caps(&reg, false);
    assert_eq!(freed.entries_removed, 1);
    assert!(tmp.join(NEVER_INDEX).exists(), "the marker is kept");
    assert!(!tmp.join("a").exists() && tmp.join("b").exists());
}

// `track` runs every couple of seconds in every owner; an idle registry must not be rewritten and
// fsynced each time (disk churn, and a main daemon's tick showed up as a "write" in the lab test
// that snapshots the main dir).
#[test]
fn an_update_that_changes_nothing_does_not_rewrite_the_registry_file() {
    let (d, reg) = registry();
    // A live root with no children: `track` records the root itself once, then has nothing new.
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let start = procs::start_time(child.id()).unwrap();
    reg.add_root(root("s", Owner::current(), child.id(), start))
        .unwrap();
    reg.track().unwrap();
    let file = d.path().join("hygiene/registry.json");
    let old = SystemTime::now() - Duration::from_secs(1000);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    reg.track().unwrap();
    let after = std::fs::metadata(&file).unwrap().modified().unwrap();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(after, old, "an idle track rewrote the file");
}

// An idle daemon ticks `track` every 2 s: with nothing registered it must not create the registry
// or its lock (a lab snapshot test saw `hygiene/registry.lock` appear in the main dir).
#[test]
fn tracking_an_empty_registry_creates_no_files() {
    let d = tempfile::tempdir().unwrap();
    let reg = Registry::at(d.path());
    reg.track().unwrap();
    assert!(!reg.has_orphaned_root());
    assert!(
        !d.path().join("hygiene").exists(),
        "nothing registered: nothing written"
    );
}

// Production danger found in review of #127: `enforce_caps` runs from EVERY process's sweep
// (hostd, each `atlas dev`, the CLI, the desktop) over one shared registry. A dir is registered
// before its session's first root, so another process sweeping in that gap took a brand-new dir
// for a dead session's leftover and deleted it whole.
#[test]
fn a_dir_registered_a_moment_ago_is_not_deleted_by_another_processes_sweep() {
    let (d, reg) = registry();
    let fresh = d.path().join("fresh-session-tmp");
    let old = d.path().join("old-session-tmp");
    for p in [&fresh, &old] {
        std::fs::create_dir_all(p).unwrap();
        std::fs::write(p.join("f"), b"x").unwrap();
    }
    for (p, created) in [
        (&fresh, now_secs()),
        (&old, now_secs() - REGISTRATION_GRACE_SECS),
    ] {
        reg.add_dir(Dir {
            path: p.clone(),
            cap_bytes: DEFAULT_DIR_CAP,
            kind: DirKind::Temp,
            // Its session has no root in the registry yet (and an owner that is not alive).
            session: Some("not-yet-rooted".into()),
            owner: Some(DEAD),
            project: None,
            created_at: created,
        })
        .unwrap();
    }
    let f = enforce_caps(&reg, false);
    assert_eq!(f.dirs_removed, 1, "{f:?}");
    assert!(
        fresh.exists(),
        "the new dir was deleted by a sweep racing its registration"
    );
    assert!(
        !old.exists(),
        "a dir past the grace whose owner is gone still goes"
    );
}

// Whole-directory deletion is only ever applied to registered paths, so the registry refuses the
// ones that must never be deleted: relative, the filesystem root, shallow system dirs, the home.
#[test]
fn the_registry_refuses_a_path_that_must_never_be_deleted_whole() {
    let (_d, reg) = registry();
    let home = dirs::home_dir().unwrap();
    let mk = |p: PathBuf| Dir {
        path: p,
        cap_bytes: 1,
        kind: DirKind::Temp,
        session: Some("s".into()),
        owner: Some(DEAD),
        project: None,
        created_at: 1,
    };
    for bad in [
        PathBuf::from("relative/dir"),
        PathBuf::from("/"),
        PathBuf::from("/tmp"),
        home.clone(),
        home.parent().unwrap().to_path_buf(),
    ] {
        assert!(
            reg.add_dir(mk(bad.clone())).is_err(),
            "{} was accepted",
            bad.display()
        );
    }
    assert!(reg.dirs().is_empty());
    assert!(
        reg.add_dir(mk(home.join(".atlas/agent-tmp"))).is_ok(),
        "a scratch dir below home is fine"
    );
}

// Incident (found by `hostd_exits_when_its_dir_disappears`, 1 run in 6): hostd records and
// checks its registry every 2 s. When its dir is removed, a tick that RE-CREATED
// `<dir>/hygiene/registry.json` raced the removal ("Directory not empty") and, worse, brought the
// dir back for a daemon that must exit because it is gone. Maintenance never creates anything.
#[test]
fn maintenance_never_recreates_a_registry_folder_that_was_removed() {
    let (d, reg) = registry();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let start = procs::start_time(child.id()).unwrap();
    reg.add_root(root("s", Owner::current(), child.id(), start))
        .unwrap();
    std::fs::remove_dir_all(d.path().join("hygiene")).unwrap();

    reg.track().unwrap();
    reg.prune().unwrap();
    reg.remove_session("s").unwrap();
    reg.set_owner(&["s".to_string()], Owner::current()).unwrap();
    let _ = enforce_caps(&reg, false);
    let _ = sweep(&reg, false);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        !d.path().join("hygiene").exists(),
        "a tick resurrected the removed folder"
    );
    // Registering is the only thing that creates it again.
    reg.add_root(root("s", Owner::current(), 1, 1)).unwrap();
    assert!(d.path().join("hygiene/registry.json").is_file());
}

// CI flake (`hostd_exits_when_its_dir_disappears`, "Directory not empty"): a terminal spawned while
// hostd's dir was being removed recreated it through the registry write. A daemon's registry never
// brings back a removed base; a plain one still creates it on first use.
#[test]
fn a_daemon_registry_never_recreates_its_removed_base() {
    let d = tempfile::tempdir().unwrap();
    let base = d.path().join("hostd-dir");
    let reg = Registry::in_existing(&base);
    let err = reg
        .add_root(root("s", Owner::current(), 1, 1))
        .expect_err("a missing base is an error, not a resurrection");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(!base.exists(), "the removed dir came back");

    std::fs::create_dir(&base).unwrap();
    reg.add_root(root("s", Owner::current(), 1, 1)).unwrap();
    assert_eq!(reg.roots().len(), 1, "an existing base works as usual");

    let fresh = d.path().join("never-made");
    Registry::at(&fresh)
        .add_root(root("s", Owner::current(), 1, 1))
        .unwrap();
    assert!(
        fresh.join("hygiene").is_dir(),
        "a plain registry creates its base"
    );
}
