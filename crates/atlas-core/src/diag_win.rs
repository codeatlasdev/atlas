//! DIAGNOSTIC ONLY (branch diag/win-rename, never merged): evidence for the two Windows flakes.
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::*};
use std::time::{Duration, Instant};

fn sh(c: &str) -> String {
    let o = std::process::Command::new("cmd").args(["/d", "/c", c]).output();
    match o {
        Ok(o) => format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => format!("{c}: {e}"),
    }
}

struct Run {
    opens: u64,
    codes: std::collections::BTreeMap<i32, u64>,
    /// (first miss ms, last miss ms, count, writer phase at first miss, dir listing at first miss)
    runs: Vec<(f64, f64, u64, u32, String)>,
}

/// One round: 299 replaces against a raw reader. `phase` is 2n while replace n runs, 2n+1 between replaces.
fn round(dir: &Path, load: usize) -> (Run, Vec<(f64, f64)>) {
    let to = dir.join("status.json");
    std::fs::write(&to, "0").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let phase = Arc::new(AtomicU32::new(1));
    let t0 = Instant::now();
    let loads: Vec<_> = (0..load)
        .map(|_| {
            let s = stop.clone();
            std::thread::spawn(move || {
                let mut x = 1u64;
                while !s.load(Relaxed) {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    let reader = {
        let (to, stop, phase) = (to.clone(), stop.clone(), phase.clone());
        std::thread::spawn(move || {
            let mut r = Run {
                opens: 0,
                codes: Default::default(),
                runs: vec![],
            };
            let mut in_run = false;
            while !stop.load(Relaxed) {
                match File::open(&to) {
                    Ok(_) => {
                        r.opens += 1;
                        in_run = false;
                    }
                    Err(e) => {
                        let code = e.raw_os_error().unwrap_or(-1);
                        *r.codes.entry(code).or_default() += 1;
                        if e.kind() == io::ErrorKind::NotFound {
                            let ms = t0.elapsed().as_secs_f64() * 1e3;
                            if in_run {
                                let l = r.runs.last_mut().unwrap();
                                l.1 = ms;
                                l.2 += 1;
                            } else {
                                in_run = true;
                                let names: Vec<String> = std::fs::read_dir(to.parent().unwrap())
                                    .map(|d| {
                                        d.flatten()
                                            .map(|e| e.file_name().to_string_lossy().into_owned())
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                r.runs
                                    .push((ms, ms, 1, phase.load(Relaxed), names.join(",")));
                            }
                        } else {
                            in_run = false;
                        }
                    }
                }
            }
            r
        })
    };
    let mut spans = vec![];
    for n in 1..300u32 {
        let from = dir.join(format!("status.json.tmp-{n}"));
        std::fs::write(&from, n.to_string()).unwrap();
        phase.store(2 * n, Relaxed);
        let a = t0.elapsed().as_secs_f64() * 1e3;
        crate::sys::replace_file_with(&from, &to, crate::sys::RetryBudget::Full).unwrap();
        let b = t0.elapsed().as_secs_f64() * 1e3;
        phase.store(2 * n + 1, Relaxed);
        spans.push((a, b));
    }
    stop.store(true, Relaxed);
    for l in loads {
        l.join().unwrap();
    }
    (reader.join().unwrap(), spans)
}

#[test]
fn diag_posix_rename_gap() {
    let mut rep = String::new();
    for c in [
        "ver",
        "fsutil fsinfo volumeinfo C:",
        "echo %TEMP%",
        "powershell -NoProfile -Command \"Get-MpComputerStatus | Select RealTimeProtectionEnabled,AntivirusEnabled | Format-List\"",
    ] {
        rep += &format!("$ {c}\n{}\n", sh(c));
    }
    let cpus = std::thread::available_parallelism().unwrap().get();
    rep += &format!("cpus {cpus}\n");
    let mut total_missing = 0u64;
    for (label, load) in [("idle", 0usize), ("load", cpus)] {
        for i in 0..10 {
            let d = tempfile::tempdir().unwrap();
            let (r, spans) = round(d.path(), load);
            let missing: u64 = r.runs.iter().map(|x| x.2).sum();
            total_missing += missing;
            let mut durs: Vec<f64> = spans.iter().map(|(a, b)| b - a).collect();
            durs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            rep += &format!(
                "{label} #{i}: opens {} missing {missing} codes {:?} rename ms p50 {:.3} p99 {:.3} max {:.3}\n",
                r.opens,
                r.codes,
                durs[durs.len() / 2],
                durs[durs.len() * 99 / 100],
                durs[durs.len() - 1]
            );
            for (a, b, n, ph, names) in r.runs.iter().take(6) {
                let inside = spans
                    .iter()
                    .position(|(s, e)| *s - 1.0 <= *a && *b <= *e + 1.0)
                    .map(|p| format!("inside replace #{}", p + 1))
                    .unwrap_or_else(|| "NOT inside a replace call".into());
                rep += &format!(
                    "   miss-run {a:.3}..{b:.3} ms ({n} opens) phase {ph} ({inside}) dir=[{names}]\n"
                );
            }
        }
    }
    rep += &format!("TOTAL missing {total_missing}\n");
    panic!("DIAG REPORT\n{rep}");
}

fn spawn_idle() -> std::process::Child {
    crate::sys::shell("ping -n 31 127.0.0.1 >NUL")
        .spawn()
        .unwrap()
}

#[test]
fn diag_idle_child_tree_settles_late() {
    let mut rep = String::new();
    let mut late = 0;
    for i in 0..20 {
        let d = tempfile::tempdir().unwrap();
        let reg = crate::hygiene::Registry::at(d.path());
        let t0 = Instant::now();
        let mut child = spawn_idle();
        let start = crate::procs::start_time(child.id()).unwrap();
        let spawned = t0.elapsed();
        // when does a descendant other than the root appear?
        let mut seen = None;
        let mut polls = 0;
        while t0.elapsed() < Duration::from_secs(5) {
            let table = crate::procs::table();
            polls += 1;
            let tree = crate::procs::tree(child.id(), &table);
            if tree.len() > 1 {
                seen = Some(t0.elapsed());
                break;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        // and the registry scenario of the test, timed: add_root + first track
        let mut child2 = spawn_idle();
        let t1 = Instant::now();
        reg.add_root(crate::hygiene::Root {
            session: "s".into(),
            owner: crate::hygiene::Owner::current(),
            pid: child2.id(),
            start: crate::procs::start_time(child2.id()).unwrap(),
            kind: crate::hygiene::Kind::Agent,
            label: "agent".into(),
            project: None,
            started_at: 1,
        })
        .unwrap();
        reg.track().unwrap();
        let first = t1.elapsed();
        let m1 = reg.members().len();
        std::thread::sleep(Duration::from_millis(300));
        reg.track().unwrap();
        let m2 = reg.members().len();
        if m2 != m1 {
            late += 1;
        }
        let _ = child2.kill();
        let _ = child2.wait();
        let _ = start;
        rep += &format!(
            "#{i}: spawn returned in {spawned:?}, descendant seen after {seen:?} ({polls} polls); add_root+first track {first:?}: members {m1} -> {m2} after 300 ms\n"
        );
    }
    rep += &format!("registry changed between tracks in {late}/20\n");
    panic!("DIAG REPORT\n{rep}");
}
