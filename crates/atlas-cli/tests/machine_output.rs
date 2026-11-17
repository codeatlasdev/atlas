//! Machine output is never translated: the same bytes under `ATLAS_LANG=en` and `ATLAS_LANG=pt-BR`.
//!
//! Scripts, agents and the desktop parse `--json`, the MCP server and the JSON Schema; a catalog
//! must never reach them. This drives the real binary over every machine surface listed in
//! [`MACHINE`] in both languages and compares stdout, stderr and the exit code byte for byte.
//! A new `--json` command, or any command whose output a program reads, gets a row here in the
//! same PR. The last test proves the harness has teeth: a human command DOES differ.

#![cfg(unix)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// One machine surface: the arguments and what goes to stdin.
struct Row {
    name: &'static str,
    args: &'static [&'static str],
    stdin: &'static str,
    /// Top-level JSON keys that are live measurements (machine load, free memory) and differ between
    /// two runs of the same command in any language: compared by presence, not by value.
    live: &'static [&'static str],
}

const MCP_SESSION: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"atlas_config","arguments":{}}}"#,
    "\n",
);

const MACHINE: &[Row] = &[
    Row {
        name: "config schema",
        args: &["config", "schema"],
        stdin: "",
        live: &[],
    },
    Row {
        name: "heavy status --json",
        args: &["heavy", "status", "--json"],
        stdin: "",
        live: &["load", "mem_available_pct"],
    },
    Row {
        name: "mcp: initialize, tools/list, atlas_config",
        args: &["mcp", "--dir", "."],
        stdin: MCP_SESSION,
        live: &[],
    },
];

/// Run `atlas` with `lang` as the display language in a throwaway HOME and project.
fn run(home: &Path, args: &[&str], stdin: &str, lang: &str) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("ATLAS_LANG", lang)
        .env("ATLAS_HEAVY_DIR", home.join("heavy"))
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn atlas");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    // stdin is closed by the drop above: a stdio server ends on EOF. A hang is a failure, not a wait.
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "atlas {args:?} did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: i18n-fixture\nservices:\n  web:\n    command: echo hi\n",
    )
    .unwrap();
    dir
}

/// stdout as text, with the live measurements of `live` (top-level JSON keys) blanked: they are not
/// text of a language, and they change between two runs a second apart.
fn stable(stdout: &[u8], live: &[&str]) -> String {
    if live.is_empty() {
        return String::from_utf8_lossy(stdout).into_owned();
    }
    let mut v: serde_json::Value = serde_json::from_slice(stdout).expect("a live row prints JSON");
    for key in live {
        let object = v.as_object_mut().expect("a live row prints a JSON object");
        assert!(
            object.contains_key(*key),
            "`{key}` is missing from the output"
        );
        object.insert((*key).to_owned(), serde_json::Value::Null);
    }
    v.to_string()
}

#[test]
fn machine_output_is_byte_identical_in_every_language() {
    let dir = project();
    for row in MACHINE {
        let en = run(dir.path(), row.args, row.stdin, "en");
        let pt = run(dir.path(), row.args, row.stdin, "pt-BR");
        assert!(!en.stdout.is_empty(), "{}: no output to compare", row.name);
        assert_eq!(
            en.status.code(),
            pt.status.code(),
            "{}: exit code",
            row.name
        );
        assert_eq!(
            stable(&en.stdout, row.live),
            stable(&pt.stdout, row.live),
            "{}: stdout differs between en and pt-BR",
            row.name
        );
        assert_eq!(
            String::from_utf8_lossy(&en.stderr),
            String::from_utf8_lossy(&pt.stderr),
            "{}: stderr differs between en and pt-BR",
            row.name
        );
    }
}

// `config lang --json` reports the language, so its `language` value is the one field that
// follows ATLAS_LANG. Everything else, the keys and the identifiers, is the same in every language.
#[test]
fn config_lang_json_has_stable_keys_and_untranslated_identifiers() {
    let dir = project();
    let json = |lang: &str| -> serde_json::Value {
        let out = run(dir.path(), &["config", "lang", "--json"], "", lang);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let (en, pt) = (json("en"), json("pt-BR"));
    assert_eq!(en["language"], "en");
    assert_eq!(pt["language"], "pt-BR");
    for v in [&en, &pt] {
        assert_eq!(
            v["source"], "env",
            "an identifier, never a translated sentence"
        );
        assert!(v["preference"].is_null());
        assert_eq!(v["supported"], serde_json::json!(["en", "pt-BR"]));
    }
    let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&en), keys(&pt));
}

// The harness must be able to fail: a command meant for humans IS translated.
#[test]
fn the_harness_sees_a_translated_human_command() {
    let dir = project();
    let en = run(dir.path(), &["config", "lang"], "", "en");
    let pt = run(dir.path(), &["config", "lang"], "", "pt-BR");
    let (en, pt) = (
        String::from_utf8_lossy(&en.stdout),
        String::from_utf8_lossy(&pt.stdout),
    );
    assert!(en.contains("Display language: en"), "{en}");
    assert!(pt.contains("Idioma de exibição: pt-BR"), "{pt}");
}

#[test]
fn config_lang_saves_and_unsets_the_preference_file() {
    let dir = project();
    let lang_file = dir.path().join(".atlas").join("language");
    let ok = |args: &[&str], lang: &str| {
        let out = run(dir.path(), args, "", lang);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    // The environment variable wins over the file, and the command says so.
    let set = run(dir.path(), &["config", "lang", "pt_br"], "", "en");
    assert!(set.status.success());
    assert_eq!(std::fs::read_to_string(&lang_file).unwrap(), "pt-BR\n");
    assert!(String::from_utf8_lossy(&set.stderr).contains("ATLAS_LANG=en"));
    let shown = ok(&["config", "lang"], "en");
    assert!(shown.contains("Saved preference: pt-BR"), "{shown}");
    // Without ATLAS_LANG the saved preference is the language: the same command in pt-BR.
    let mut cmd = Command::new(BIN);
    let out = cmd
        .args(["config", "lang", "--json"])
        .env("HOME", dir.path())
        .env_remove("ATLAS_LANG")
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        (v["language"].as_str(), v["source"].as_str()),
        (Some("pt-BR"), Some("file"))
    );
    // Unset removes the file.
    ok(&["config", "lang", "--unset"], "en");
    assert!(!lang_file.exists());
    // An unsupported tag fails and leaves nothing behind.
    let bad = run(dir.path(), &["config", "lang", "klingon"], "", "en");
    assert!(!bad.status.success());
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("Unsupported language"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(!lang_file.exists());
}
