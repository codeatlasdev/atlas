//! The problems `atlas config check` finds are Portuguese for a person and English for a program.
//!
//! Real binary, throwaway HOME and project, real YAML (the validators produce the problems; nothing here builds
//! one). A person reads `atlas config check` and the error of `atlas dev`: the catalog's sentence in the display
//! language. A program reads MCP (`atlas_config`) and `atlas dev --ci`: the English bytes the validators wrote
//! before the problems had codes, in every language.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// Two problems: a typo (`helth`) and a dependency on a service that does not exist.
const BROKEN: &str =
    "name: broken\nservices:\n  a:\n    command: x\n    depends_on: [nope]\n    helth: y\n";

/// What the validators printed for [`BROKEN`] before the codes (captured from origin/main).
const OLD_ENGLISH: &str = "2 config errors:\n  atlas.yaml: services.a.helth: unknown field (did you mean 'health'?)\n  atlas.yaml: services.a.depends_on.nope: unknown service 'nope'";

/// A warning, not an error: `lazy` does not start by itself and `web` waits for it.
const WARNS: &str = "name: warns\nservices:\n  lazy:\n    command: run-lazy\n    autostart: false\n  web:\n    command: run-web\n    depends_on: [lazy]\n";

fn project(yaml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
    dir
}

fn run(dir: &Path, lang: &str, args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(BIN)
        .args(args)
        .arg("--dir")
        .arg(dir)
        .env("HOME", dir)
        .env("ATLAS_LANG", lang)
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn atlas");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn config_check_prints_the_problems_in_the_display_language() {
    let dir = project(BROKEN);
    let en = run(dir.path(), "en", &["config", "check"], "");
    assert_eq!(en.status.code(), Some(1));
    assert_eq!(text(&en.stderr), format!("{OLD_ENGLISH}\n"));

    let pt = run(dir.path(), "pt-BR", &["config", "check"], "");
    assert_eq!(pt.status.code(), Some(1));
    assert_eq!(
        text(&pt.stderr),
        "2 erros de configuração:\n  atlas.yaml: services.a.helth: campo desconhecido (você quis dizer 'health'?)\n  atlas.yaml: services.a.depends_on.nope: serviço 'nope' desconhecido\n"
    );
}

#[test]
fn config_check_warnings_follow_the_display_language() {
    let dir = project(WARNS);
    let en = run(dir.path(), "en", &["config", "check"], "");
    let pt = run(dir.path(), "pt-BR", &["config", "check"], "");
    assert!(en.status.success() && pt.status.success());
    let (en, pt) = (text(&en.stderr), text(&pt.stderr));
    assert!(
        en.contains(
            "services.web.depends_on.lazy: 'lazy' has autostart: false, so 'web' waits until 'lazy' is started by hand"
        ),
        "{en}"
    );
    assert!(
        pt.contains(
            "services.web.depends_on.lazy: 'lazy' tem autostart: false, então 'web' espera até que 'lazy' seja iniciado manualmente"
        ),
        "{pt}"
    );
}

/// `atlas dev` fails on the user's terminal with the same list `config check` prints.
#[test]
fn a_failing_atlas_dev_words_the_error_in_the_display_language() {
    let dir = project(BROKEN);
    let pt = run(dir.path(), "pt-BR", &["dev", "--detach"], "");
    assert!(!pt.status.success());
    assert!(
        text(&pt.stderr).contains(
            "2 erros de configuração:\n  atlas.yaml: services.a.helth: campo desconhecido"
        ),
        "{}",
        text(&pt.stderr)
    );
}

const MCP_SESSION: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"atlas_config","arguments":{}}}"#,
    "\n",
);

/// MCP answers a broken config with the old English text and `isError`, byte for byte, in every language.
#[test]
fn mcp_atlas_config_reports_a_broken_config_in_english_in_every_language() {
    let dir = project(BROKEN);
    let answer = |lang: &str| {
        let out = run(dir.path(), lang, &["mcp"], MCP_SESSION);
        text(&out.stdout)
            .lines()
            .find(|l| l.contains("\"id\":3"))
            .unwrap_or_else(|| panic!("no answer to id 3: {}", text(&out.stdout)))
            .to_owned()
    };
    let (en, pt) = (answer("en"), answer("pt-BR"));
    assert_eq!(en, pt, "MCP output differs between languages");
    let v: serde_json::Value = serde_json::from_str(&en).unwrap();
    assert_eq!(v["result"]["isError"], true);
    assert_eq!(v["result"]["content"][0]["text"], OLD_ENGLISH);
}

/// `atlas dev --ci` is read by pipelines: the list under the `error:` chrome is the old English one in every language
/// (the chrome prefix of the process is the process's, not this slice's).
#[test]
fn dev_ci_reports_a_broken_config_in_english_in_every_language() {
    let dir = project(BROKEN);
    let list = |lang: &str| {
        let out = run(dir.path(), lang, &["dev", "--ci", "--timeout", "10"], "");
        assert!(!out.status.success(), "{lang}");
        let err = text(&out.stderr);
        // The chrome prefix is `ESC[1;31merror:ESC[0m ` (the colors are on even when piped).
        let (_, list) = err
            .split_once("\u{1b}[0m ")
            .expect("a chrome prefix before the list");
        list.to_owned()
    };
    let (en, pt) = (list("en"), list("pt-BR"));
    assert_eq!(en, pt, "`--ci` output differs between languages");
    assert_eq!(en.trim_end(), OLD_ENGLISH);
}
