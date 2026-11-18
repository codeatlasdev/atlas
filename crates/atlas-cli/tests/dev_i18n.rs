//! `atlas dev --ci` is read by pipelines and agents, so its output (the JSON and the stderr progress) is English
//! whatever the display language. Real binary, throwaway HOME and project.
//!
//! Nothing here depends on timing: the service exits with status 1 at once, so the verdict (`ok: false`, the one
//! service `failed`) is the same on every run; the stderr lines are not compared byte for byte (which transitions a
//! one-second poll sees is timing), each is checked to be English instead.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// A project whose only service exits 1 immediately.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: i18n-fixture\nservices:\n  once:\n    command: exit 1\n",
    )
    .unwrap();
    dir
}

fn atlas(dir: &Path, lang: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .arg("--dir")
        .arg(dir)
        .env("HOME", dir)
        .env("ATLAS_LANG", lang)
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .stdin(Stdio::null())
        .output()
        .expect("spawn atlas")
}

/// The service states `--ci` prints on stderr, spelled as the engine names them.
const STATES: [&str; 8] = [
    "Starting",
    "Running",
    "Failed",
    "Stopped",
    "Unhealthy",
    "Degraded",
    "Disabled",
    "Suspended",
];

/// In `en` and in `pt-BR`: the same exit code, the same JSON on stdout, the header line of stderr as is, and every
/// other line of stderr is a service line in English (`  once: Failed`), never a catalog sentence of the language.
#[test]
fn ci_output_is_english_in_every_language() {
    let run = |lang: &'static str| {
        std::thread::spawn(move || {
            let dir = project();
            let out = atlas(dir.path(), lang, &["dev", "--ci", "--timeout", "60"]);
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        })
    };
    let (en, pt) = (run("en"), run("pt-BR"));
    let (en, pt) = (en.join().unwrap(), pt.join().unwrap());
    for (lang, (code, stdout, stderr)) in [("en", &en), ("pt-BR", &pt)] {
        assert_eq!(
            *code,
            Some(1),
            "{lang}: a failed service fails the run\n{stderr}"
        );
        let mut lines = stderr.lines();
        assert_eq!(
            lines.next(),
            Some("atlas dev --ci  project=i18n-fixture  services=1"),
            "{lang}: header\n{stderr}"
        );
        for line in lines {
            let Some(state) = line.strip_prefix("  once: ") else {
                panic!("{lang}: not an English service line: {line:?}\n{stderr}");
            };
            assert!(
                STATES.contains(&state),
                "{lang}: state {state:?} is not English\n{stderr}"
            );
        }
        assert!(
            stderr.contains("once: Failed"),
            "{lang}: the failure is reported\n{stderr}"
        );
        assert!(stdout.contains("\"ok\": false"), "{lang}: {stdout}");
    }
    assert_eq!(en.1, pt.1, "the JSON differs between en and pt-BR");
}
