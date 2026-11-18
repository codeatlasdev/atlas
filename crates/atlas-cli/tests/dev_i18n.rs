//! `atlas dev --ci` is read by pipelines and agents, so its output (the JSON and the stderr progress) is the same bytes
//! whatever the display language. Real binary, throwaway HOME and project.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: i18n-fixture\nservices:\n  once:\n    command: sleep 60\n",
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

/// The CI run prints the same stderr progress and the same JSON in `en` and `pt-BR`. The two runs go side by side
/// (each has the 10 s stabilisation window of `--ci`), each in its own project.
#[test]
fn ci_output_is_the_same_in_every_language() {
    let run = |lang: &'static str| {
        std::thread::spawn(move || {
            let dir = project();
            let out = atlas(dir.path(), lang, &["dev", "--ci", "--timeout", "30"]);
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        })
    };
    let (en, pt) = (run("en"), run("pt-BR"));
    let (en, pt) = (en.join().unwrap(), pt.join().unwrap());
    assert_eq!(en.0, Some(0), "stderr: {}", en.2);
    assert_eq!(en, pt, "--ci output differs between en and pt-BR");
    assert!(en.1.contains("\"ok\": true"), "{}", en.1);
}
