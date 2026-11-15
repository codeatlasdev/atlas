//! Install, reinstall, update and uninstall scripts executed for real against a simulated server
//! (`server_sim`): fake `sudo`/`systemctl`/`id` on PATH in a temp root. Each test names the audit gap
//! (CrewOSAudit, 2026-11-15) it guards.

use super::binary;
use super::layout;
use super::server_sim::Sim;

const OLD: &str = "1.0.0";
const NEW: &str = "2.0.0";

fn version_of(sim: &Sim, rel: &str) -> String {
    sim.read(rel)
        .lines()
        .nth(1)
        .unwrap_or_default()
        .trim_start_matches("# ")
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// The host download (the first step of install and update). Found while testing the offline fallback: macOS and
// BSD/busybox `mktemp` refuse a template with a suffix after the X's, so the host download failed for a reason that
// had nothing to do with the network; and a download that failed halfway left its temp file behind on every try.
// ---------------------------------------------------------------------------------------------

/// A `mktemp` like BSD's and busybox's: the X's must end the template; files go under `$ROOT/tmp`.
const STRICT_MKTEMP: &str = r#"
case "$1" in *XXXXXX) ;; *) echo "mktemp: bad template $1" >&2; exit 1;; esac
mkdir -p "$ROOT/tmp"; f="$ROOT/tmp/f$$"; : > "$f"; echo "$f"
"#;

#[test]
fn the_host_download_uses_a_mktemp_template_every_mktemp_accepts() {
    let sim = Sim::new();
    sim.fake("mktemp", STRICT_MKTEMP);
    sim.fake("curl", "echo \"curl $*\" >> \"$ROOT/log/curl\"; exit 6");
    let r = sim.run(
        &binary::download_script("1.0.0", "x86_64-unknown-linux-gnu"),
        b"",
    );
    // curl was reached (and failed like an offline host): mktemp did not stop the script first.
    assert!(
        sim.read("log/curl").contains("releases/download"),
        "{}\n{}",
        r.stdout,
        r.stderr
    );
    assert_eq!(r.code, Some(6), "{}\n{}", r.stdout, r.stderr);
}

#[test]
fn a_failed_host_download_leaves_no_temp_file_behind() {
    let sim = Sim::new();
    sim.fake("mktemp", STRICT_MKTEMP);
    sim.fake("curl", "exit 6");
    let r = sim.run(
        &binary::download_script("1.0.0", "x86_64-unknown-linux-gnu"),
        b"",
    );
    assert_ne!(r.code, Some(0));
    let left = std::fs::read_dir(sim.path("tmp"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 0, "temp files left behind in $ROOT/tmp");
}

// ---------------------------------------------------------------------------------------------
// Gap 3: a reinstall over a running server kept no rollback; a failed new binary destroyed the only good one.
// ---------------------------------------------------------------------------------------------

#[test]
fn reinstalling_over_a_server_keeps_the_old_binary_as_prev_and_rolls_back_a_dead_one() {
    let sim = Sim::new();
    sim.install_atlas("lib/atlas-crew/atlas", OLD, true);
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, false); // the new binary never answers
    sim.write("var/lib/atlas-crew/.keep", b"");

    let r = sim.run(&binary::install_script(1), b"");
    // Rolled back: exit 3, the old binary is the one in place again.
    assert_eq!(r.code, Some(3), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(version_of(&sim, "lib/atlas-crew/atlas"), OLD);
}

#[test]
fn a_fresh_install_has_no_prev_and_keeps_a_healthy_binary() {
    let sim = Sim::new();
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, true);
    sim.write("var/lib/atlas-crew/.keep", b"");

    let r = sim.run(&binary::install_script(1), b"");
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(version_of(&sim, "lib/atlas-crew/atlas"), NEW);
    assert!(!sim.exists("lib/atlas-crew/atlas.prev"), "nothing to keep");
    assert!(r.stdout.contains("update: ok 2.0.0"), "{}", r.stdout);
}

// ---------------------------------------------------------------------------------------------
// Gap 2: install said "ready" without waiting for hostd. A dead binary must fail the script (non-zero),
// and the CLI turns that into a pointer at `atlas crew logs`.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_binary_that_never_answers_fails_the_install_instead_of_reporting_ready() {
    let sim = Sim::new();
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, false);
    sim.write("var/lib/atlas-crew/.keep", b"");
    let r = sim.run(&binary::install_script(1), b"");
    // No previous binary to fall back to: rollback fails, the loud code.
    assert_eq!(r.code, Some(4), "{}\n{}", r.stdout, r.stderr);
}

// ---------------------------------------------------------------------------------------------
// Gap 4: the swap is not idempotent. ssh exit 255 after the remote command ran made `exec` run it again,
// and the second run copied the NEW binary over atlas.prev (the rollback point is gone).
// ---------------------------------------------------------------------------------------------

#[test]
fn running_the_swap_twice_never_overwrites_the_rollback_point() {
    let sim = Sim::new();
    sim.install_atlas("lib/atlas-crew/atlas", OLD, true);
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, true);
    sim.write("var/lib/atlas-crew/.keep", b"");

    let first = sim.run(&binary::swap_script(1), b"");
    assert_eq!(first.code, Some(0), "{}\n{}", first.stdout, first.stderr);
    assert_eq!(version_of(&sim, "lib/atlas-crew/atlas.prev"), OLD);

    // The retry after a dropped link: atlas.new is gone (it was moved), atlas is the new one.
    let second = sim.run(&binary::swap_script(1), b"");
    assert_ne!(second.code, Some(0), "no staged binary: nothing to do");
    assert_eq!(
        version_of(&sim, "lib/atlas-crew/atlas.prev"),
        OLD,
        "the retry must not copy the new binary over atlas.prev"
    );
}

#[test]
fn a_retry_with_a_staged_binary_left_over_still_keeps_the_first_prev() {
    // Link dropped after the cp but before the mv: atlas.prev already holds the old binary.
    let sim = Sim::new();
    sim.install_atlas("lib/atlas-crew/atlas", OLD, true);
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, true);
    sim.write("var/lib/atlas-crew/.keep", b"");
    let first = sim.run(&binary::swap_script(1), b"");
    assert_eq!(first.code, Some(0));
    // The new binary is staged again (a re-run of the whole update).
    sim.install_atlas("lib/atlas-crew/atlas.new", NEW, true);
    let again = sim.run(&binary::swap_script(1), b"");
    assert_eq!(again.code, Some(0));
    // Running the same version again keeps the real previous version, not a copy of itself.
    assert_eq!(version_of(&sim, "lib/atlas-crew/atlas.prev"), OLD);
}

// ---------------------------------------------------------------------------------------------
// Gap 7: one preflight probe listing EVERY missing tool with a package hint, and a real systemd check.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_preflight_lists_every_missing_tool_with_a_package_hint_and_changes_nothing() {
    let sim = Sim::new();
    std::fs::create_dir_all(sim.path("run/systemd/system")).unwrap();
    for t in ["curl", "git", "tar"] {
        sim.remove_fake(t);
    }
    // curl may exist in /usr/bin of the test machine: the fake PATH hides it.
    let r = sim.run(&binary::preflight_script(), b"");
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    let missing = binary::parse_preflight(&r.stdout);
    let tools: Vec<&str> = missing.tools.iter().map(String::as_str).collect();
    for want in ["curl", "git", "tar"] {
        assert!(tools.contains(&want), "{want} not reported: {tools:?}");
    }
    let msg = binary::preflight_error("srv", &missing).to_string();
    assert!(
        msg.contains("curl") && msg.contains("git") && msg.contains("tar"),
        "{msg}"
    );
    assert!(
        msg.contains("apt-get install") || msg.contains("apt install"),
        "{msg}"
    );
    // Nothing was changed: the probe only reads.
    assert!(!sim.exists("log/sudo"), "preflight must not run sudo");
}

#[test]
fn a_host_with_systemctl_but_no_running_systemd_is_refused() {
    // A container image ships /usr/bin/systemctl but PID 1 is not systemd (no /run/systemd/system).
    let sim = Sim::new();
    let r = sim.run(&binary::preflight_script(), b"");
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    let missing = binary::parse_preflight(&r.stdout);
    assert!(!missing.systemd, "{missing:?}");
    let msg = binary::preflight_error("srv", &missing).to_string();
    assert!(msg.contains("systemd"), "{msg}");
}

#[test]
fn a_complete_host_passes_the_preflight_and_reports_its_uname() {
    let sim = Sim::new();
    std::fs::create_dir_all(sim.path("run/systemd/system")).unwrap();
    sim.fake("uname", "echo 'Linux aarch64'");
    let r = sim.run(&binary::preflight_script(), b"");
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    let p = binary::parse_preflight(&r.stdout);
    assert!(p.is_ready(), "{p:?}");
    assert_eq!(p.uname, "Linux aarch64");
}

// ---------------------------------------------------------------------------------------------
// Gap 9: password sudo. With tty_tickets the credential `sudo -v` cached in one ssh call is useless in the
// next. Every privileged call must carry the password itself (stdin, never argv).
// ---------------------------------------------------------------------------------------------

#[test]
fn a_script_with_a_sudo_password_authenticates_every_sudo_it_runs() {
    let sim = Sim::new();
    sim.write("sudo-password", b"hunter2\n");
    let script = binary::with_sudo_password("sudo mkdir -p /tmp/a && sudo true && sudo true");
    let r = sim.run(&script, b"hunter2\n");
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(sim.read("log/sudo").lines().count(), 3);
    // The password is on stdin only: the script text carries none.
    assert!(!script.contains("hunter2"));
}

// A Windows client sends a script that has a `"` as `printf … | bash` (atlas_core::ssh::windows_safe), so bash would
// read its script from the stdin that carries the password and the upload. Whatever the script holds (the swap and
// the key script are full of `"$u"`), the wrapped text must have no `"`: then it travels as plain `bash -c '…'`.
#[test]
fn a_wrapped_script_has_no_double_quote_whatever_the_script_holds() {
    let real_scripts = [
        binary::swap_script(60),
        binary::rollback_script(60),
        layout::uninstall_script(),
        "f=\"$HOME/x\"; sudo tee -a \"$f\" > /dev/null; echo 'it'\\''s' \\ \"q\"".to_owned(),
    ];
    for script in real_scripts {
        let w = binary::with_sudo_password(&script);
        assert!(!w.contains('"'), "double quote in the wrapped text: {w}");
        assert_eq!(atlas_core::ssh::windows_safe(&w), w);
    }
    assert!(!binary::stage_upload_script().contains('"'));
}

// The wrapper runs the script exactly as written (quotes, backslashes, `$`) and hands back its exit code: the swap
// answers 3 for a rollback and 4 for a dead hostd, and the caller tells them apart.
#[test]
fn the_wrapped_script_runs_unchanged_and_keeps_its_exit_code() {
    let sim = Sim::new();
    sim.write("sudo-password", b"hunter2\n");
    let script = "x=\"a b\"; printf '%s|' \"$x\" 'it'\\''s' \\\\ ; sudo true; exit 7";
    let r = sim.run(&binary::with_sudo_password(script), b"hunter2\n");
    assert_eq!(r.code, Some(7), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(r.stdout, "a b|it's|\\|", "{}", r.stderr);
}

// Found by the password tests: an escaped byte followed by a digit (`$0`, `-p 22`) was read by `printf %b` as one
// longer octal number (`\0440` is not `$` and `0`), so the script that ran was not the script that was sent.
#[test]
fn the_script_survives_the_wrapper_byte_for_byte_even_with_digits_after_an_escape() {
    let sim = Sim::new();
    sim.write("sudo-password", b"pw\n");
    for script in [
        "x0=v; y9=w; printf '%s' $x0$y9",
        "printf '%s' \"$1$2\" 'a0\\0b1'",
        "echo '09 $1 \"7\" \\1'",
        "printf '%s' \u{e9}\u{4e2d}'x9'",
    ] {
        let direct = sim.run(script, b"");
        let wrapped = sim.run(&binary::with_sudo_password(script), b"pw\n");
        assert_eq!(wrapped.stdout, direct.stdout, "script: {script}");
        assert_eq!(wrapped.code, direct.code, "script: {script}");
    }
}

// The shim and the askpass helper pass the password and the arguments through exactly: spaces, `$`, quotes.
#[test]
fn a_password_with_spaces_dollars_and_quotes_authenticates() {
    let sim = Sim::new();
    let pw = "p w$d'\"x`y";
    sim.write("sudo-password", format!("{pw}\n").as_bytes());
    let script = binary::with_sudo_password("sudo sh -c 'echo ok $0' a_b");
    let r = sim.run(&script, format!("{pw}\n").as_bytes());
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(
        r.stdout.trim(),
        "ok a_b",
        "stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn the_payload_after_the_password_line_reaches_the_script_untouched() {
    // `sudo tee file < payload`: the first stdin line is the password, the rest is the upload.
    let sim = Sim::new();
    sim.write("sudo-password", b"hunter2\n");
    let script = binary::with_sudo_password(&format!(
        "sudo tee {} > /dev/null",
        sim.path("out.bin").display()
    ));
    sim.fake("tee", "cat > \"$1\"");
    let mut stdin = b"hunter2\n".to_vec();
    stdin.extend_from_slice(&[0, 1, 2, 255, b'\n', b'x']);
    let r = sim.run(&script, &stdin);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(
        std::fs::read(sim.path("out.bin")).unwrap(),
        [0, 1, 2, 255, b'\n', b'x']
    );
}

// A script's earlier `sudo` (mkdir, mv) runs before the one that reads the upload. Its stdin is the same pipe:
// it must not swallow the payload meant for the later `sudo tee`.
#[test]
fn an_earlier_sudo_does_not_swallow_the_upload_that_a_later_one_reads() {
    let sim = Sim::new();
    sim.write("sudo-password", b"hunter2\n");
    sim.fake("tee", "cat > \"$1\"");
    let script = binary::with_sudo_password(&format!(
        "sudo mkdir -p {d} && sudo tee {d}/atlas.new > /dev/null && sudo chmod +x {d}/atlas.new",
        d = sim.path("lib/atlas-crew").display()
    ));
    let mut stdin = b"hunter2\n".to_vec();
    stdin.extend_from_slice(b"#!/bin/sh\nreal binary bytes\n");
    let r = sim.run(&script, &stdin);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(
        sim.read("lib/atlas-crew/atlas.new"),
        "#!/bin/sh\nreal binary bytes\n"
    );
}

#[test]
fn a_wrong_sudo_password_fails_the_call() {
    let sim = Sim::new();
    sim.write("sudo-password", b"hunter2\n");
    let script = binary::with_sudo_password("sudo true");
    let r = sim.run(&script, b"wrong\n");
    assert_ne!(r.code, Some(0));
    assert!(r.stderr.contains("incorrect password"), "{}", r.stderr);
}

// ---------------------------------------------------------------------------------------------
// Gap 13: uninstall removed the unit and the binary, THEN found the user busy. Check first.
// ---------------------------------------------------------------------------------------------

#[test]
fn uninstall_removes_nothing_when_the_crew_user_is_busy() {
    let sim = Sim::new();
    sim.write("lib/atlas-crew/atlas", b"#!/bin/sh\n");
    sim.write("etc/systemd/system/atlas-crew.service", b"[Unit]\n");
    sim.write("var/lib/atlas-crew/.keep", b"");
    // `pgrep -u atlas-crew` finds a running process: the user is busy.
    sim.fake("pgrep", "echo 4242");
    let r = sim.run(&layout::uninstall_script(), b"");
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert!(sim.exists("lib/atlas-crew/atlas"), "binary removed");
    assert!(
        sim.exists("etc/systemd/system/atlas-crew.service"),
        "unit removed"
    );
    assert!(r.stderr.contains("atlas-crew"), "{}", r.stderr);
    let log = sim.read("log/systemctl");
    assert!(
        !log.contains("stop"),
        "the service must keep running: {log}"
    );
}

#[test]
fn uninstall_removes_everything_when_only_the_service_itself_runs_as_the_user() {
    // hostd (the unit) runs as the crew user: stopping the unit comes first, so its own processes do not count.
    let sim = Sim::new();
    sim.write("lib/atlas-crew/atlas", b"#!/bin/sh\n");
    sim.write("etc/systemd/system/atlas-crew.service", b"[Unit]\n");
    sim.write("var/lib/atlas-crew/.keep", b"");
    sim.fake("pgrep", "exit 1"); // after the stop nobody runs as the user
    sim.fake("userdel", "rm -rf \"$ROOT/var/lib/$2\"");
    let r = sim.run(&layout::uninstall_script(), b"");
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(!sim.exists("lib/atlas-crew"));
    assert!(!sim.exists("etc/systemd/system/atlas-crew.service"));
}
