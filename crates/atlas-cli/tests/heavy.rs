//! `atlas run --heavy` against the real binary: the machine-wide queue lets at most N jobs run, in
//! arrival order, passes exit codes through and frees a slot the instant its holder dies. Each test
//! has its own queue directory (`ATLAS_HEAVY_DIR`), so nothing touches `~/.atlas`.

// The fixtures are `sh` one-liners and signals.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

mod common;
use common::{libc_kill, wait_for};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

fn heavy(dir: &Path, slots: usize) -> Command {
    let mut c = Command::new(BIN);
    c.env("ATLAS_HEAVY_DIR", dir)
        .env("ATLAS_HEAVY_SLOTS", slots.to_string())
        // Off: these tests are about slots, not about how busy (load or memory) the CI machine is;
        // the memory-gate test sets its own `ATLAS_HEAVY_MINMEM` after this.
        .env("ATLAS_HEAVY_MAXLOAD", "0")
        .env("ATLAS_HEAVY_MINMEM", "0")
        .env("HOME", dir.parent().unwrap())
        .stdin(Stdio::null());
    c
}

/// `atlas run --heavy --label <label> -- sh -c <script>`
fn run(dir: &Path, slots: usize, label: &str, script: &str) -> Command {
    let mut c = heavy(dir, slots);
    c.args(["run", "--heavy", "--label", label, "--", "sh", "-c", script]);
    c
}

fn status(dir: &Path, slots: usize) -> Value {
    let out = heavy(dir, slots)
        .args(["heavy", "status", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn labels(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|j| j["label"].as_str().unwrap().to_string())
        .collect()
}

struct Fixture {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    log: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("home/heavy");
    std::fs::create_dir_all(&dir).unwrap();
    let log = tmp.path().join("events.log");
    Fixture {
        dir,
        log,
        _tmp: tmp,
    }
}

/// Peak overlap of the `[start, end]` intervals the jobs logged themselves.
fn peak_overlap(log: &Path) -> (usize, usize) {
    let text = std::fs::read_to_string(log).unwrap();
    let mut events: Vec<(u128, i32)> = Vec::new();
    let mut jobs = 0;
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let t: u128 = f[2].parse().unwrap();
        match f[0] {
            "start" => {
                events.push((t, 1));
                jobs += 1;
            }
            "end" => events.push((t, -1)),
            other => panic!("bad line {other}"),
        }
    }
    // An end sorts before a start at the same instant: touching intervals do not overlap.
    events.sort();
    let (mut cur, mut peak) = (0i32, 0i32);
    for (_, d) in events {
        cur += d;
        peak = peak.max(cur);
    }
    (peak as usize, jobs)
}

fn job_script(log: &Path, id: &str, ms: u32) -> String {
    // `date +%s%N` is not portable (macOS); python3 is a test prerequisite elsewhere in this crate.
    let stamp = "python3 -c 'import time;print(int(time.time()*1000))'";
    // Seconds and milliseconds apart: `sleep 0.{ms}` made 1500 ms a 0.15 s sleep, so the FIFO
    // test's blocker was gone before its waiters queued (CI flake on macOS).
    let (s, m) = (ms / 1000, ms % 1000);
    format!(
        "echo start {id} $({stamp}) >> {log}; sleep {s}.{m:03}; echo end {id} $({stamp}) >> {log}",
        log = log.display()
    )
}

/// Like [`job_script`], but the job holds its slot until the test creates `release`: no timing.
fn held_script(log: &Path, id: &str, release: &Path) -> String {
    format!(
        "echo start {id} >> {log}; while [ ! -e {release} ]; do sleep 0.05; done; echo end {id} >> {log}",
        log = log.display(),
        release = release.display()
    )
}

// Acceptance of the issue: 5 agents start suites at once, nothing is lost, the load stays under the
// cap (here: never more than 2 jobs running at the same time).
#[test]
fn five_concurrent_jobs_with_two_slots_never_run_more_than_two_and_all_finish() {
    require_python!();
    let f = fixture();
    let kids: Vec<Child> = (0..5)
        .map(|i| {
            run(
                &f.dir,
                2,
                &format!("job{i}"),
                &job_script(&f.log, &format!("j{i}"), 700),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
        })
        .collect();

    // While they run, the records (not just the jobs' own log) never show more than 2 running.
    let mut max_running = 0;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut kids = kids;
    let mut codes = vec![None; 5];
    while codes.iter().any(Option::is_none) {
        assert!(Instant::now() < deadline, "jobs did not finish: {codes:?}");
        let st = status(&f.dir, 2);
        max_running = max_running.max(st["running"].as_array().unwrap().len());
        for (i, k) in kids.iter_mut().enumerate() {
            if codes[i].is_none() {
                codes[i] = k.try_wait().unwrap().map(|s| s.code());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        max_running <= 2,
        "records showed {max_running} running with 2 slots"
    );
    assert!(codes.iter().all(|c| *c == Some(Some(0))), "{codes:?}");

    let (peak, jobs) = peak_overlap(&f.log);
    assert_eq!(jobs, 5, "every job ran");
    assert_eq!(
        peak, 2,
        "the jobs' own timestamps: exactly the 2 slots were used, never more"
    );
    let st = status(&f.dir, 2);
    assert!(
        st["running"].as_array().unwrap().is_empty() && st["queued"].as_array().unwrap().is_empty(),
        "nothing is left behind: {st}"
    );
}

#[test]
fn the_exit_code_of_the_command_passes_through_and_its_output_too() {
    let f = fixture();
    let out = run(&f.dir, 1, "x", "echo out; echo err >&2; exit 7")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out");
    assert!(String::from_utf8_lossy(&out.stderr).contains("err"));
    // A command killed by a signal reports 128 + signal, like a shell does.
    let out = run(&f.dir, 1, "x", "kill -TERM $$").output().unwrap();
    assert_eq!(out.status.code(), Some(128 + 15));
    // A job that cannot start is an error, not a success.
    let out = heavy(&f.dir, 1)
        .args([
            "run",
            "--heavy",
            "--no-nice",
            "--",
            "/definitely/not/a/program",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(127), "sh: command not found");
}

// A heavy job's environment caps build parallelism (slots share the cores) unless the user set it.
#[test]
fn the_job_environment_caps_build_parallelism_unless_the_user_set_it() {
    let f = fixture();
    let cores = std::thread::available_parallelism().unwrap().get();
    let show = "echo cargo=$CARGO_BUILD_JOBS uv=$UV_THREADPOOL_SIZE";
    let out = run(&f.dir, 2, "x", show)
        .env_remove("CARGO_BUILD_JOBS")
        .env_remove("UV_THREADPOOL_SIZE")
        .output()
        .unwrap();
    let n = (cores / 2).max(2);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("cargo={n} uv={n}"),
        "max(2, cores / slots) with 2 slots"
    );
    // The user's own value stays; the one they did not set is still filled in.
    let out = run(&f.dir, 2, "x", show)
        .env("CARGO_BUILD_JOBS", "7")
        .env_remove("UV_THREADPOOL_SIZE")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("cargo=7 uv={n}")
    );
}

// The memory gate: with a threshold no machine meets, the job waits ATLAS_HEAVY_WAIT seconds, says
// why, and then runs anyway; with the gate off it runs at once and says nothing.
#[test]
fn the_memory_gate_holds_a_job_then_runs_it_anyway_after_the_wait() {
    let f = fixture();
    let gated = |minmem: &str| {
        run(&f.dir, 1, "x", "echo ran")
            // 101 clamps to 100: "all memory free", which a running machine never has.
            .env("ATLAS_HEAVY_MINMEM", minmem)
            .env("ATLAS_HEAVY_WAIT", "1")
            .output()
            .unwrap()
    };
    let out = gated("101");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ran", "{err}");
    assert!(err.contains("of memory free"), "says why it waits: {err}");
    assert!(
        err.contains("running x anyway"),
        "warns when it gives up: {err}"
    );
    let out = gated("0");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ran");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("memory"),
        "gate off: silent"
    );
}

#[test]
fn a_killed_holder_frees_its_slot_at_once_and_leaves_no_stale_record() {
    let f = fixture();
    let mut holder = run(&f.dir, 1, "holder", "sleep 60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("holder to run", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["running"]) == ["holder"]).then_some(())
    });
    let waiter = run(&f.dir, 1, "waiter", "echo got-it")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("waiter to queue", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["queued"]) == ["waiter"]).then_some(())
    });
    // SIGKILL: no cleanup code runs in the holder. Only the OS can free its slot.
    let killed_at = Instant::now();
    // SAFETY: an exact positive pid of a child we spawned.
    assert_eq!(unsafe { libc_kill(holder.id() as i32, 9) }, 0);
    holder.wait().unwrap();
    let out = waiter.wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "got-it");
    assert!(
        killed_at.elapsed() < Duration::from_secs(5),
        "the waiter took the slot right away"
    );
    let st = status(&f.dir, 1);
    assert!(
        st["running"].as_array().unwrap().is_empty(),
        "no stale holder record: {st}"
    );
    // The orphaned `sleep 60` is the killed job's child, not ours to wait for.
    let _ = Command::new("pkill").args(["-f", "sleep 60"]).status();
}

#[test]
fn waiters_start_in_the_order_they_arrived() {
    require_python!();
    let f = fixture();
    let release = f.dir.parent().unwrap().join("release-blocker");
    let blocker = run(
        &f.dir,
        1,
        "blocker",
        &held_script(&f.log, "blocker", &release),
    )
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    wait_for("blocker to run", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["running"]) == ["blocker"]).then_some(())
    });
    let mut kids = vec![blocker];
    for (n, id) in ["a", "b", "c"].iter().enumerate() {
        kids.push(
            run(&f.dir, 1, id, &job_script(&f.log, id, 100))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        // Each is visibly queued before the next arrives: arrival order is then unambiguous.
        wait_for("waiter to queue", Duration::from_secs(10), || {
            (labels(&status(&f.dir, 1)["queued"]).len() == n + 1).then_some(())
        });
    }
    assert_eq!(labels(&status(&f.dir, 1)["queued"]), ["a", "b", "c"]);
    std::fs::write(&release, b"").unwrap();
    for mut k in kids {
        assert!(k.wait().unwrap().success());
    }
    let order: Vec<String> = std::fs::read_to_string(&f.log)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("start"))
        .map(|l| l.split_whitespace().nth(1).unwrap().to_string())
        .collect();
    assert_eq!(order, ["blocker", "a", "b", "c"]);
}

#[test]
fn ctrl_c_while_queued_leaves_the_queue_and_never_runs_the_command() {
    let f = fixture();
    let mut holder = run(&f.dir, 1, "holder", "sleep 3")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("holder to run", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["running"]) == ["holder"]).then_some(())
    });
    let marker = f.dir.parent().unwrap().join("ran");
    let mut waiter = run(&f.dir, 1, "waiter", &format!("touch {}", marker.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("waiter to queue", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["queued"]) == ["waiter"]).then_some(())
    });
    // SAFETY: an exact positive pid of a child we spawned.
    assert_eq!(unsafe { libc_kill(waiter.id() as i32, 2) }, 0);
    assert_eq!(waiter.wait().unwrap().code(), Some(130));
    assert!(status(&f.dir, 1)["queued"].as_array().unwrap().is_empty());
    holder.wait().unwrap();
    assert!(!marker.exists(), "an interrupted job must not run");
}

#[test]
fn sigterm_to_a_running_job_reaches_its_command_and_frees_the_slot() {
    let f = fixture();
    let marker = f.dir.parent().unwrap().join("got-term");
    let script = format!(
        "trap 'touch {}; exit 5' TERM; echo ready; while :; do sleep 0.1; done",
        marker.display()
    );
    let mut job = run(&f.dir, 1, "t", &script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("job to run", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["running"]) == ["t"]).then_some(())
    });
    // The trap is installed before `ready` is printed: wait for that line, not a fixed sleep
    // (CI: a loaded runner had not reached the trap in 500 ms, so TERM killed the shell, 143).
    let mut out = std::io::BufReader::new(job.stdout.take().unwrap());
    let mut line = String::new();
    std::io::BufRead::read_line(&mut out, &mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    // SAFETY: an exact positive pid of a child we spawned.
    assert_eq!(unsafe { libc_kill(job.id() as i32, 15) }, 0);
    // The runner passes the signal on and returns the command's own exit code.
    assert_eq!(job.wait().unwrap().code(), Some(5));
    assert!(marker.exists(), "the command saw the TERM");
    assert!(status(&f.dir, 1)["running"].as_array().unwrap().is_empty());
}

#[test]
fn status_names_who_holds_the_slot_and_ranks_the_eta_from_history() {
    let f = fixture();
    // Two finished runs teach the label's average.
    for _ in 0..2 {
        assert!(
            run(&f.dir, 1, "suite", "sleep 1")
                .status()
                .unwrap()
                .success()
        );
    }
    // A failing run must not teach it (a crash after 0 s would drag the average down).
    assert_eq!(
        run(&f.dir, 1, "suite", "exit 1").status().unwrap().code(),
        Some(1)
    );
    let mut holder = run(&f.dir, 1, "suite", "sleep 3")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("holder to run", Duration::from_secs(10), || {
        (labels(&status(&f.dir, 1)["running"]) == ["suite"]).then_some(())
    });
    let mut waiter = run(&f.dir, 1, "suite", "true")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("waiter to queue", Duration::from_secs(10), || {
        (!status(&f.dir, 1)["queued"].as_array().unwrap().is_empty()).then_some(())
    });
    let st = status(&f.dir, 1);
    let r = &st["running"][0];
    assert_eq!(r["pid"].as_u64().unwrap(), holder.id() as u64);
    assert_eq!(r["state"], "running");
    assert!(r["cwd"].as_str().unwrap().len() > 1 && r["project"].is_string());
    assert!(r["started_at"].as_u64().unwrap() > 0);
    let avg = r["avg_s"].as_u64().unwrap();
    assert!(
        (1..=2).contains(&avg),
        "average of the two 1 s runs only: {avg}"
    );
    assert!(st["queued"][0]["eta_s"].as_u64().is_some(), "{st}");

    let text = String::from_utf8(
        heavy(&f.dir, 1)
            .args(["heavy", "status"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert!(text.contains("1 of 1 slot busy, 1 waiting"), "{text}");
    assert!(text.contains(&format!("pid {}", holder.id())), "{text}");
    holder.wait().unwrap();
    waiter.wait().unwrap();
}

#[test]
fn slots_come_from_the_env_then_the_config_file_then_half_the_cores() {
    let f = fixture();
    let status_slots = |env: Option<&str>| -> u64 {
        let mut c = Command::new(BIN);
        c.env("ATLAS_HEAVY_DIR", &f.dir)
            .env("HOME", f.dir.parent().unwrap())
            .env_remove("ATLAS_HEAVY_SLOTS")
            .args(["heavy", "status", "--json"]);
        if let Some(e) = env {
            c.env("ATLAS_HEAVY_SLOTS", e);
        }
        serde_json::from_slice::<Value>(&c.output().unwrap().stdout).unwrap()["slots"]
            .as_u64()
            .unwrap()
    };
    let cores = std::thread::available_parallelism().unwrap().get() as u64;
    // The memory term (one slot per 8 GB) depends on the host and is unit-tested with injected
    // inputs; here only what holds on every machine.
    let default = status_slots(None);
    assert!(
        (1..=(cores / 2).max(1)).contains(&default),
        "default {default} for {cores} cores"
    );
    std::fs::create_dir_all(f.dir.parent().unwrap().join(".atlas")).unwrap();
    std::fs::write(
        f.dir.parent().unwrap().join(".atlas/config.json"),
        r#"{"heavy":{"slots":3}}"#,
    )
    .unwrap();
    assert_eq!(status_slots(None), 3, "~/.atlas/config.json");
    assert_eq!(status_slots(Some("5")), 5, "the environment wins");
    assert_eq!(status_slots(Some("0")), 3, "an invalid value is ignored");
}
