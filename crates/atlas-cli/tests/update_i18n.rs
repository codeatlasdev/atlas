//! `atlas update` speaks the display language, against the real binary in a throwaway HOME. The expected text is the
//! catalog's own value for the language (`t_in!`), never a typed sentence. Nothing here reaches the network: the
//! binary under test is a debug build, which `atlas update` refuses to replace before it looks anything up.
//!
//! The install flows (download, checksum, package managers) are in `update_pkg.rs`, which pins `ATLAS_LANG=en`.

#![cfg(unix)]

use atlas_i18n::t_in;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
const JUDGED: &str = "/opt/atlas-i18n-test/bin/atlas";

/// `atlas <args>` with `lang` as the display language and a stand-in path for "the installed atlas".
fn atlas(home: &Path, lang: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("ATLAS_LANG", lang)
        .env("ATLAS_UPDATE_EXE", JUDGED)
        .env_remove("ATLAS_UPDATE_PROMPT")
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

/// A dev build is never replaced: the refusal names the binary and the way out, after the translated `error:`.
#[test]
fn a_dev_build_refuses_to_update_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["update"]);
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let stderr = text(&out.stderr);
        let want = t_in!(tag, "cmd_update_dev_build", path = JUDGED);
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

/// `--check` is allowed on a dev build, but the header it prints before the lookup is in the language; with no
/// network the lookup fails and says so in the language too.
#[test]
fn the_check_header_and_a_failed_lookup_are_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    for tag in ["en", "pt-BR"] {
        let out = Command::new(BIN)
            .args(["update", "--check"])
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("ATLAS_LANG", tag)
            .env("ATLAS_UPDATE_EXE", JUDGED)
            // No curl to find: the lookup fails at once, offline and deterministic.
            .env("PATH", "/nonexistent")
            .env_remove("LANGUAGE")
            .env_remove("LC_ALL")
            .env_remove("LC_MESSAGES")
            .env_remove("LANG")
            .stdin(Stdio::null())
            .output()
            .expect("spawn atlas");
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let stdout = text(&out.stdout);
        for want in [
            t_in!(tag, "cmd_update_checking"),
            t_in!(
                tag,
                "cmd_update_row_current",
                version = env!("CARGO_PKG_VERSION")
            ),
        ] {
            assert!(
                stdout.contains(&want),
                "{tag}: {want:?} missing in:\n{stdout}"
            );
        }
        let stderr = text(&out.stderr);
        let want = t_in!(tag, "cmd_update_check_failed");
        assert!(
            stderr.contains(&want),
            "{tag}: {want:?} missing in:\n{stderr}"
        );
    }
}
