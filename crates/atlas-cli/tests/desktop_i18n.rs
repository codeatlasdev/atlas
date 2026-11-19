//! `atlas desktop` speaks the display language, against the real binary in a throwaway HOME. The expected text is the
//! catalog's own value for the language (`t_in!`), never a typed sentence; the exit codes and what the command
//! leaves on disk are the same in every language.
//!
//! Not here: installing, updating and `--agent` starts, which act on the real `/Applications` and the real display
//! whatever HOME says. The commands below only read or refuse.

#![cfg(unix)]

use atlas_i18n::t_in;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// `atlas <args>` with `lang` as the display language, HOME at `home` and no debug bridge to find.
fn atlas(home: &Path, lang: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("ATLAS_LANG", lang)
        .env("ATLAS_DEBUG_SOCK", home.join("no-such.sock"))
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .stdin(Stdio::null())
        .output()
        .expect("spawn atlas")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `atlas desktop --status` ends with the CLI's own version row in the language, whether or not an app is installed
/// on the machine running the test.
#[test]
fn status_reports_the_cli_version_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["desktop", "--status"]);
        assert!(out.status.success(), "{tag}: {}", text(&out.stderr));
        let want = t_in!(
            tag,
            "cmd_desktop_status_cli",
            version = env!("CARGO_PKG_VERSION")
        );
        let stdout = text(&out.stdout);
        assert!(
            stdout.contains(&want),
            "{tag}: {want:?} missing in:\n{stdout}"
        );
    }
}

/// Stopping an instance that does not exist names it and the next step, in the language, and fails the same way.
#[test]
fn stopping_an_unknown_instance_says_what_to_run() {
    let home = tempfile::tempdir().unwrap();
    for tag in ["en", "pt-BR"] {
        let out = atlas(
            home.path(),
            tag,
            &["desktop", "--agent", "--stop", "--name", "i18n-no-such"],
        );
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let want = t_in!(tag, "cmd_desktop_no_such_instance", name = "i18n-no-such");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&want),
            "{tag}: {want:?} missing in:\n{stderr}"
        );
        assert!(
            stderr.contains(&t_in!(tag, "cli_chrome_error")),
            "{tag}: {stderr}"
        );
    }
}

/// An invalid instance name is refused before anything starts, with the rule in the language.
#[test]
fn an_invalid_instance_name_is_refused_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    for tag in ["en", "pt-BR"] {
        let out = atlas(
            home.path(),
            tag,
            &["desktop", "--agent", "--stop", "--name", "no spaces!"],
        );
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let want = t_in!(tag, "cmd_desktop_name_chars");
        assert!(
            text(&out.stderr).contains(&want),
            "{tag}: {}",
            text(&out.stderr)
        );
    }
}

/// The debug bridge commands say where the socket should be and how to get one; the JSON they print on success is
/// not text and is not translated.
#[test]
fn a_missing_debug_bridge_says_how_to_start_one() {
    let home = tempfile::tempdir().unwrap();
    let socket = home.path().join("no-such.sock");
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["desktop", "debug", "info"]);
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let want = t_in!(
            tag,
            "cmd_desktop_no_bridge",
            path = socket.display().to_string()
        );
        assert!(
            text(&out.stderr).contains(&want),
            "{tag}: {}",
            text(&out.stderr)
        );
    }
}

/// A script that cannot be read fails with the cause chain in the language (`caused by:` included).
#[test]
fn a_missing_capture_script_fails_with_the_cause_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("missing.json");
    for tag in ["en", "pt-BR"] {
        let out = atlas(
            home.path(),
            tag,
            &["desktop", "record", "--script", script.to_str().unwrap()],
        );
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let want = t_in!(
            tag,
            "cmd_install_read_failed",
            path = script.display().to_string()
        );
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&want),
            "{tag}: {want:?} missing in:\n{stderr}"
        );
        assert!(
            stderr.contains(&t_in!(tag, "cli_chrome_caused_by")),
            "{tag}: {stderr}"
        );
    }
}
