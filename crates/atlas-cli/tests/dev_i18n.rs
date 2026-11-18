//! `atlas dev --ci` is read by pipelines and agents, so its output (the JSON and the stderr progress) is the same bytes
//! whatever the display language. Real binary, throwaway HOME and project.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// `secs` is unusual and differs per run: a command line shared with another run or another test (`sleep 30`) can be found and killed by the other one when it stops its own.
fn project(secs: u32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        format!("name: i18n-fixture\nservices:\n  once:\n    command: sleep {secs}\n"),
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

/// The lines of `text`, a line that repeats the one before it kept once.
fn dedup(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        if out.last() != Some(&line) {
            out.push(line);
        }
    }
    out.join("\n")
}

/// The CI run prints the same stderr progress and the same JSON in `en` and `pt-BR`. The two runs go side by side
/// (each has the 10 s stabilisation window of `--ci`), each in its own project.
#[test]
fn ci_output_is_the_same_in_every_language() {
    let run = |lang: &'static str, secs: u32| {
        std::thread::spawn(move || {
            let dir = project(secs);
            let out = atlas(dir.path(), lang, &["dev", "--ci", "--timeout", "30"]);
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                // A watcher may announce the same state twice; when and how often is timing, not language.
                dedup(&String::from_utf8_lossy(&out.stderr)),
            )
        })
    };
    let (en, pt) = (run("en", 4717), run("pt-BR", 4723));
    let (en, pt) = (en.join().unwrap(), pt.join().unwrap());
    assert_eq!(en.0, Some(0), "stdout: {}\nstderr: {}", en.1, en.2);
    assert_eq!(en, pt, "--ci output differs between en and pt-BR");
    assert!(en.1.contains("\"ok\": true"), "{}", en.1);
}
