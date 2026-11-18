//! `atlas link` and `atlas <path>` speak the display language, against the real binary in a throwaway
//! HOME. The expected text is the catalog's own value for the language (`t_in!`), never a typed sentence; the files
//! a command leaves behind and the paths it prints are the same in every language.
//!
//! Not here: `atlas link --desktop` and `atlas unlink`, which act on the real `/Applications/Atlas.app` whatever
//! HOME says.

#![cfg(unix)]

use atlas_i18n::t_in;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// `atlas <args>` with `lang` as the display language, HOME at `home`, and no desktop app to find.
fn atlas(home: &Path, lang: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("ATLAS_LANG", lang)
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

/// A Cargo workspace whose `target/debug/atlas` exists, and a HOME that already has a release `atlas` installed.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let ws = t.path().join("repo");
    std::fs::create_dir_all(ws.join("target/debug")).unwrap();
    std::fs::write(ws.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(ws.join("target/debug/atlas"), "dev build").unwrap();
    let home = t.path().join("home");
    std::fs::create_dir_all(home.join(".atlas/bin")).unwrap();
    std::fs::write(home.join(".atlas/bin/atlas"), "release").unwrap();
    (t, ws, home)
}

/// `atlas link --no-build` swaps the installed binary for a link to the debug build and says so in the language;
/// the release binary is kept as `atlas.backup` in every language.
#[test]
fn link_reports_in_the_display_language_and_does_the_same_work() {
    for tag in ["en", "pt-BR"] {
        let (_t, ws, home) = fixture();
        let out = atlas(
            &home,
            tag,
            &["link", "--no-build", "--dir", ws.to_str().unwrap()],
        );
        assert!(out.status.success(), "{tag}: {}", text(&out.stderr));
        let stdout = text(&out.stdout);
        let installed = home.join(".atlas/bin/atlas");
        let backup = home.join(".atlas/bin/atlas.backup");
        for want in [
            t_in!(tag, "cmd_link_title"),
            t_in!(tag, "cmd_link_workspace", dir = ws.display().to_string()),
            t_in!(
                tag,
                "cmd_link_backed_up",
                path = backup.display().to_string()
            ),
            t_in!(
                tag,
                "cmd_link_linked",
                link = installed.display().to_string(),
                target = ws.join("target/debug/atlas").display().to_string()
            ),
            t_in!(tag, "cmd_link_now_runs_debug"),
        ] {
            assert!(
                stdout.contains(&want),
                "{tag}: {want:?} missing in:\n{stdout}"
            );
        }
        assert_eq!(
            std::fs::read_link(&installed).unwrap(),
            ws.join("target/debug/atlas"),
            "{tag}"
        );
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "release",
            "{tag}"
        );
    }
}

/// With nothing to link to, the error names the file and the two ways out, in the language, after the translated
/// `error:` prefix, and the installed binary is untouched.
#[test]
fn link_without_a_debug_binary_says_what_to_run() {
    let (_t, ws, home) = fixture();
    std::fs::remove_file(ws.join("target/debug/atlas")).unwrap();
    for tag in ["en", "pt-BR"] {
        let out = atlas(
            &home,
            tag,
            &["link", "--no-build", "--dir", ws.to_str().unwrap()],
        );
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let stderr = text(&out.stderr);
        let want = t_in!(
            tag,
            "cmd_link_no_debug_binary",
            path = ws.join("target/debug/atlas").display().to_string()
        );
        assert!(
            stderr.contains(&want),
            "{tag}: {want:?} missing in:\n{stderr}"
        );
        assert!(
            stderr.contains(&t_in!(tag, "cli_chrome_error")),
            "{tag}: {stderr}"
        );
        assert_eq!(
            std::fs::read_to_string(home.join(".atlas/bin/atlas")).unwrap(),
            "release",
            "{tag}: nothing was swapped"
        );
    }
}

/// `atlas <path>` for a path that is not there fails in the language.
#[test]
fn open_errors_are_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    let missing = home.path().join("no/such/folder");
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &[missing.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let want = t_in!(
            tag,
            "cmd_link_open_missing",
            path = missing.display().to_string()
        );
        assert!(
            text(&out.stderr).contains(&want),
            "{tag}: {}",
            text(&out.stderr)
        );
    }
}
