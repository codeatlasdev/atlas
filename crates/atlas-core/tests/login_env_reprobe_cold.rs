//! #285: a launch ran the login-shell probe (an interactive login shell, 0.3 s on fish and 6 s+ when an rc file hangs) twice
//! in the app: `reprobe()` ran the probe, then took the lock on `CURRENT`, which forced its `LazyLock` initializer to run it
//! again. The probe is process-wide state, so each of these is its own test binary: nothing else in the process may touch it first.
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
fn a_first_reprobe_is_the_only_probe_of_the_launch() {
    let dir = tempfile::tempdir().unwrap();
    let (shell, log) = counting_shell(dir.path());
    atlas_core::login_env::set_override(Some(shell.clone()));

    let probed = atlas_core::login_env::reprobe();

    assert_eq!(probed.shell, shell, "the override picked the shell");
    assert_eq!(
        starts(&log),
        1,
        "reprobe on a cold cache ran the probe, then forced the lazy initializer to run it again"
    );
    // The result is what the next reader gets: no probe for it.
    assert_eq!(atlas_core::login_env::current().shell, shell);
    let _ = atlas_core::login_env::path_var();
    assert_eq!(
        starts(&log),
        1,
        "reading the cached environment does not probe"
    );
    // After a settings change, a re-probe is one more probe, exactly one.
    atlas_core::login_env::reprobe();
    assert_eq!(starts(&log), 2);
}
