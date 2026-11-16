//! #285: a launch ran the login-shell probe (an interactive login shell, 0.3 s on fish and 6 s+ when an rc file hangs) twice
//! in the app: `reprobe()` ran the probe, then took the lock on `CURRENT`, which forced its `LazyLock` initializer to run it
//! again. The probe is process-wide state, so this is its own test binary: nothing else in the process may touch it first.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A "shell" that records each probe (the first attempt, `-l -i -c`: a `sh` that rejects those flags, dash on Ubuntu, makes
/// the probe retry with fewer, and that is still one probe), then behaves like `sh`.
fn counting_shell(dir: &Path) -> (String, std::path::PathBuf) {
    let log = dir.join("starts.log");
    let sh = dir.join("counting-sh");
    std::fs::write(
        &sh,
        format!(
            "#!/bin/sh\ncase \"$*\" in \"-l -i -c \"*) echo probe >> '{}' ;; esac\nexec /bin/sh \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (sh.display().to_string(), log)
}

fn starts(log: &Path) -> usize {
    let n = std::fs::read_to_string(log).map_or(0, |t| t.lines().count());
    eprintln!("probes so far: {n}");
    n
}

#[test]
fn callers_racing_for_the_first_probe_share_one() {
    // Under the target dir, not /tmp: a full tmpfs makes the fake shell's `echo >> log` fail silently and the count read 0.
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (shell, log) = counting_shell(dir.path());
    atlas_core::login_env::set_override(Some(shell.clone()));

    // The launch: the desktop warms the probe on a thread while the spawn path (`host::start` -> `atlas_bin` -> `path_var`)
    // and the status commands ask for the environment from others.
    let askers: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                if i == 0 {
                    atlas_core::login_env::warm();
                    atlas_core::login_env::current().shell
                } else {
                    atlas_core::login_env::current().shell
                }
            })
        })
        .collect();
    for a in askers {
        assert_eq!(a.join().unwrap(), shell);
    }
    assert_eq!(
        starts(&log),
        1,
        "one probe for the launch, not one per caller"
    );
}
