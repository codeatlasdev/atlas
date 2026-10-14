//! The editor's whole language-server round trip, driven through [`Manager`] against the
//! fake server in semantic mode (`FAKE_SEMANTIC`): open → diagnostics → completion →
//! definition → hover → references → rename → edit → diagnostics again, plus the
//! explainability surface (reasons, tail, custom command, settings overrides, restart,
//! uninstall, catalog). No network, no real server: this is the CI-safe twin of `real.rs`.

// The fake server is launched through `/bin/sh -c` (it sets the fake's env per test).
#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atlas_lsp::install::{Failure, Launch, Progress, Source};
use atlas_lsp::manager::{Config, Manager, Overrides, Push, Resolver, State};
use atlas_lsp::registry::Server;
use serde_json::{Value, json};
use tokio::sync::mpsc;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-lsp");

/// Resolver that launches the fake in semantic mode, or fails with a chosen [`Failure`].
struct SemanticFake {
    fail: Option<Failure>,
    env: Vec<(String, String)>,
}

impl Resolver for SemanticFake {
    fn resolve<'a>(
        &'a self,
        _server: &'static Server,
        progress: &'a dyn Progress,
    ) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<Launch>> + Send + 'a>> {
        Box::pin(async move {
            progress.set("Downloading fake");
            if let Some(f) = &self.fail {
                return Err(anyhow::Error::new(f.clone()));
            }
            let exports: String = self
                .env
                .iter()
                .map(|(k, v)| format!("export {k}='{v}'; "))
                .collect();
            let mut l = Launch::custom(
                "/bin/sh",
                vec![
                    "-c".into(),
                    format!("export FAKE_SEMANTIC=1; {exports}exec '{FAKE}'"),
                ],
            );
            l.source = Source::Managed;
            l.version = Some("9.9.9".into());
            Ok(l)
        })
    }
}

struct Rig {
    m: Manager,
    rx: mpsc::Receiver<Push>,
    base: PathBuf,
    project: PathBuf,
    _tmp: tempfile::TempDir,
}

fn cfg() -> Config {
    Config {
        idle_shutdown: Duration::from_millis(300),
        init_timeout: Duration::from_secs(10),
        max_restarts: 3,
        restart_window: Duration::from_secs(60),
        restart_backoff: Duration::from_millis(30),
        request_timeout: Duration::from_secs(5),
    }
}

fn rig(fail: Option<Failure>, env: &[(&str, &str)]) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("Cargo.toml"), "[package]\nname=\"p\"\n").unwrap();
    let (tx, rx) = mpsc::channel(512);
    let resolver = Arc::new(SemanticFake {
        fail,
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    });
    let base = tmp.path().to_path_buf();
    let m = Manager::with_resolver(&base, cfg(), tx, resolver);
    Rig {
        m,
        rx,
        base,
        project,
        _tmp: tmp,
    }
}

impl Rig {
    fn file(&self, name: &str) -> String {
        let p = self.project.join("src").join(name);
        std::fs::write(&p, "").unwrap();
        p.to_string_lossy().into_owned()
    }

    async fn diagnostics(&mut self, path: &str, pred: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match self.rx.recv().await.expect("push channel closed") {
                    Push::Diagnostics {
                        path: p,
                        diagnostics,
                        ..
                    } if p == path && pred(&diagnostics) => return diagnostics,
                    _ => {}
                }
            }
        })
        .await
        .expect("diagnostics did not arrive")
    }

    async fn req(&self, path: &str, method: &str, params: Value) -> Value {
        self.m.request(path, method, params, None).await.unwrap()
    }
}

fn td(path: &str) -> Value {
    json!({"uri": format!("file://{path}")})
}

fn at(line: u64, character: u64) -> Value {
    json!({"line": line, "character": character})
}

const SRC: &str = "fn add(a: i32) {}\nfn main() {\n    add(1);\n    let x = add(2); // TODO\n}\n";

#[tokio::test]
async fn open_diagnostics_completion_definition_hover_references_rename() {
    let mut r = rig(None, &[]);
    let f = r.file("lib.rs");
    let rep = r.m.open(&f, "rust", SRC.into(), 1, None).await;
    assert_eq!(rep.status, "ready");
    assert_eq!(rep.capabilities["definitionProvider"], true);
    assert!(rep.reason.is_none());

    // diagnostics: the TODO on line 3
    let d = r
        .diagnostics(&f, |d| d.as_array().is_some_and(|a| !a.is_empty()))
        .await;
    assert_eq!(d[0]["message"], "TODO left in code");
    assert_eq!(d[0]["range"]["start"]["line"], 3);

    // completion after typing `ad` on a fresh line: the declared `add` matches the prefix
    let comp = r
        .req(
            &f,
            "textDocument/completion",
            json!({"textDocument": td(&f), "position": at(2, 7)}),
        )
        .await;
    let labels: Vec<_> = comp["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    assert!(labels.contains(&"add".to_string()), "{labels:?}");

    // go to definition of the `add` call on line 2 lands on `fn add` (line 0, col 3..6)
    let def = r
        .req(
            &f,
            "textDocument/definition",
            json!({"textDocument": td(&f), "position": at(2, 5)}),
        )
        .await;
    assert_eq!(def[0]["range"]["start"], at(0, 3));
    assert_eq!(def[0]["range"]["end"], at(0, 6));

    // hover on the declaration
    let hov = r
        .req(
            &f,
            "textDocument/hover",
            json!({"textDocument": td(&f), "position": at(0, 4)}),
        )
        .await;
    assert!(
        hov["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("fn add")
    );

    // references: declaration + two calls
    let refs = r
        .req(
            &f,
            "textDocument/references",
            json!({"textDocument": td(&f), "position": at(0, 4), "context": {"includeDeclaration": true}}),
        )
        .await;
    assert_eq!(refs.as_array().unwrap().len(), 3);

    // rename: one edit per occurrence, all in this file
    let ws = r
        .req(
            &f,
            "textDocument/rename",
            json!({"textDocument": td(&f), "position": at(2, 5), "newName": "plus"}),
        )
        .await;
    let edits = ws["changes"][format!("file://{f}")].as_array().unwrap();
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|e| e["newText"] == "plus"));
}

#[tokio::test]
async fn edits_flow_to_the_server_and_change_its_answers() {
    let mut r = rig(None, &[]);
    let f = r.file("lib.rs");
    r.m.open(&f, "rust", "fn one() {}\n".into(), 1, None).await;
    // The server must see the incremental edit: insert `fn two() {}` after line 0.
    r.m.change(
        &f,
        2,
        vec![json!({
            "range": {"start": at(1, 0), "end": at(1, 0)},
            "text": "fn two() {} // TODO\n",
        })],
    )
    .await
    .unwrap();
    let d = r
        .diagnostics(&f, |d| d.as_array().is_some_and(|a| !a.is_empty()))
        .await;
    assert_eq!(
        d[0]["range"]["start"]["line"], 1,
        "diagnostic on the new line"
    );
    let syms = r
        .req(
            &f,
            "textDocument/documentSymbol",
            json!({"textDocument": td(&f)}),
        )
        .await;
    let names: Vec<_> = syms
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["one", "two"]);
    // Fixing the TODO clears the diagnostic.
    r.m.change(
        &f,
        3,
        vec![json!({
            "range": {"start": at(1, 12), "end": at(1, 19)},
            "text": "",
        })],
    )
    .await
    .unwrap();
    r.diagnostics(&f, |d| d.as_array().is_some_and(Vec::is_empty))
        .await;
}

#[tokio::test]
async fn a_typed_install_failure_reaches_the_reply_and_the_status() {
    let failure = Failure::new("no_go", "gopls is not installed and `go` was not found")
        .hint("Install Go.")
        .command("brew install go");
    let r = rig(Some(failure.clone()), &[]);
    let f = r.project.join("main.go");
    std::fs::write(r.project.join("go.mod"), "module x\n").unwrap();
    std::fs::write(&f, "").unwrap();
    let f = f.to_string_lossy().into_owned();
    let rep = r.m.open(&f, "go", String::new(), 1, None).await;
    assert_eq!(rep.status, "missing");
    assert_eq!(rep.server, Some("gopls"));
    let reason = rep.reason.expect("reason");
    assert_eq!(reason.code, "no_go");
    assert_eq!(reason.command.as_deref(), Some("brew install go"));
    let st = r.m.status();
    assert_eq!(st[0].state, State::Missing);
    assert_eq!(st[0].reason.as_ref().unwrap().code, "no_go");
    assert!(st[0].log.ends_with("lsp/logs/gopls.log"), "{}", st[0].log);
}

#[tokio::test]
async fn unsupported_language_lists_what_is_supported() {
    let r = rig(None, &[]);
    let f = r.file("a.rs");
    let rep = r.m.open(&f, "brainfuck", String::new(), 1, None).await;
    assert_eq!(rep.status, "disabled");
    assert!(rep.server.is_none());
    assert!(rep.supported.contains(&"TypeScript"));
    assert!(rep.supported.contains(&"Rust"));
    assert!(rep.supported.contains(&"Go"));
}

#[tokio::test]
async fn disabled_override_never_starts_a_server() {
    let r = rig(None, &[]);
    let f = r.file("a.rs");
    let o = Overrides {
        disabled: true,
        ..Default::default()
    };
    let rep = r.m.open_with(&f, "rust", String::new(), 1, None, &o).await;
    assert_eq!(
        (rep.server, rep.status),
        (Some("rust-analyzer"), "disabled")
    );
    assert_eq!(r.m.instance_count(), 0);
}

#[tokio::test]
async fn custom_command_replaces_the_registry_server_and_reports_custom() {
    let mut r = rig(
        Some(Failure::new("install_failed", "registry must not be used")),
        &[],
    );
    let f = r.file("a.rs");
    // The "custom server" is the fake itself, launched through sh by absolute path.
    let o = Overrides {
        command: Some("/bin/sh".into()),
        args: vec!["-c".into(), format!("FAKE_SEMANTIC=1 exec '{FAKE}'")],
        ..Default::default()
    };
    let rep =
        r.m.open_with(&f, "rust", "fn z() {} // TODO".into(), 1, None, &o)
            .await;
    assert_eq!(rep.status, "ready", "{:?}", rep.reason);
    let st = r.m.status();
    assert_eq!(st[0].source, Some(Source::Custom));
    r.diagnostics(&f, |d| d.as_array().is_some_and(|a| !a.is_empty()))
        .await;
}

#[tokio::test]
async fn a_missing_custom_command_says_so_and_names_the_command() {
    let r = rig(None, &[]);
    let f = r.file("a.rs");
    let o = Overrides {
        command: Some("definitely-not-a-real-server-xyz".into()),
        ..Default::default()
    };
    let rep = r.m.open_with(&f, "rust", String::new(), 1, None, &o).await;
    assert_eq!(rep.status, "missing");
    let reason = rep.reason.unwrap();
    assert_eq!(reason.code, "custom_not_found");
    assert!(reason.message.contains("definitely-not-a-real-server-xyz"));
    assert!(reason.hint.unwrap().contains("Settings"));
}

#[tokio::test]
async fn custom_and_default_servers_for_one_root_do_not_share_an_instance() {
    let r = rig(None, &[]);
    let (a, b) = (r.file("a.rs"), r.file("b.rs"));
    let o = Overrides {
        command: Some("/bin/sh".into()),
        args: vec!["-c".into(), format!("FAKE_SEMANTIC=1 exec '{FAKE}'")],
        ..Default::default()
    };
    r.m.open(&a, "rust", String::new(), 1, None).await;
    r.m.open_with(&b, "rust", String::new(), 1, None, &o).await;
    assert_eq!(r.m.instance_count(), 2);
}

#[tokio::test]
async fn initialization_options_override_is_sent_in_initialize() {
    // `test/config` makes the fake ask the client for `workspace/configuration`; the user's
    // `settings` must be merged over Atlas's defaults in the answer.
    let r = rig(None, &[]);
    let f = r.file("a.rs");
    let o = Overrides {
        settings: Some(json!({"rust-analyzer": {"check": {"command": "check"}}})),
        ..Default::default()
    };
    r.m.open_with(&f, "rust", String::new(), 1, None, &o).await;
    let cfg = r.req(&f, "test/config", Value::Null).await;
    assert_eq!(cfg[0]["check"]["command"], "check", "{cfg}");
    assert_eq!(cfg[0]["checkOnSave"], true, "defaults survive the merge");
}

#[tokio::test]
async fn crashed_server_status_carries_its_stderr_tail() {
    let mut r = rig(None, &[("FAKE_EXIT_ON_START", "1")]);
    let f = r.file("a.rs");
    // Pre-seed the server log: the fake exits before writing, so stand in for a real stderr.
    let log = r.base.join("lsp/logs/rust-analyzer.log");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(
        &log,
        "booting\nerror: cannot find rustc\nfatal: giving up\n",
    )
    .unwrap();
    let rep = r.m.open(&f, "rust", String::new(), 1, None).await;
    assert_eq!(rep.status, "crashed");
    assert!(rep.reason.is_some());
    // Wait for the settled status push (restart budget exhausted).
    let st = loop {
        let p = tokio::time::timeout(Duration::from_secs(10), r.rx.recv())
            .await
            .expect("status push")
            .expect("open channel");
        if let Push::Status(v) = p
            && v.iter()
                .any(|s| s.state == State::Crashed && !s.tail.is_empty())
        {
            break v;
        }
    };
    let s = &st[0];
    assert_eq!(s.tail.last().map(String::as_str), Some("fatal: giving up"));
    assert!(s.tail.iter().any(|l| l.contains("cannot find rustc")));
    let (path, text) = r.m.log("rust-analyzer", 2);
    assert_eq!(path, log);
    assert_eq!(text, "error: cannot find rustc\nfatal: giving up");
}

#[tokio::test]
async fn restart_brings_a_crashed_server_back_and_resets_the_budget() {
    let mut r = rig(None, &[]);
    let f = r.file("a.rs");
    r.m.open(&f, "rust", "fn a() {}".into(), 1, None).await;
    let before = r.m.pids();
    assert_eq!(before.len(), 1);
    assert!(r.m.restart("rust-analyzer", None).await);
    // New process, documents replayed: a semantic query works again.
    let mut ok = false;
    for _ in 0..200 {
        if r.m.pids().first().is_some_and(|p| *p != before[0]) {
            let q =
                r.m.request(
                    &f,
                    "textDocument/definition",
                    json!({"textDocument": td(&f), "position": at(0, 4)}),
                    None,
                )
                .await;
            if q.is_ok() {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(ok, "server did not come back");
    r.diagnostics(&f, |_| true).await;
    assert!(!r.m.restart("gopls", None).await, "nothing to restart");
}

#[tokio::test]
async fn catalog_lists_every_server_with_detection_and_never_installs() {
    let r = rig(None, &[]);
    let cat = r.m.catalog().await;
    let names: Vec<_> = cat.iter().map(|c| c.name).collect();
    for n in [
        "rust-analyzer",
        "gopls",
        "typescript-language-server",
        "basedpyright",
    ] {
        assert!(names.contains(&n), "{names:?}");
    }
    let ts = cat
        .iter()
        .find(|c| c.name == "typescript-language-server")
        .unwrap();
    assert!(ts.languages.contains(&"typescript"));
    assert!(ts.language_labels.contains(&"TypeScript"));
    assert_eq!(ts.install, "npm");
    // Nothing was downloaded as a side effect.
    assert!(!r.base.join("lsp").exists());
}

/// Regression (1.3.0 crash "t.entry.languageLabels[i]"): the desktop reads `lsp_catalog` with the
/// field names of `apps/desktop/src/lib/lsp/types.ts` (`CatalogEntry`). Serde's default snake_case
/// sent `language_labels`, so every real build crashed while the TS tests, fed camelCase mocks,
/// passed. This pins the serialized JSON itself: every key the frontend reads, at every level,
/// and no snake_case key anywhere in the payload.
#[tokio::test]
async fn catalog_json_uses_the_field_names_the_desktop_reads() {
    let r = rig(None, &[]);
    let json = serde_json::to_value(r.m.catalog().await).unwrap();
    let entries = json.as_array().expect("catalog is an array");
    assert!(!entries.is_empty());
    for e in entries {
        let o = e.as_object().unwrap();
        for k in [
            "name",
            "label",
            "languages",
            "languageLabels",
            "install",
            "version",
            "detected",
        ] {
            assert!(o.contains_key(k), "missing `{k}` in {e}");
        }
        // One label per language id, in the same order (the UI indexes one by the other).
        assert_eq!(
            o["languages"].as_array().unwrap().len(),
            o["languageLabels"].as_array().unwrap().len(),
            "{e}"
        );
        assert!(o["detected"].is_object(), "{e}");
    }
    fn no_snake(v: &serde_json::Value, at: &str) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    assert!(!k.contains('_'), "snake_case key `{k}` at {at}");
                    no_snake(v, &format!("{at}.{k}"));
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| no_snake(v, at)),
            _ => {}
        }
    }
    no_snake(&json, "catalog");
}

#[tokio::test]
async fn uninstall_removes_only_the_managed_copy_and_stops_instances() {
    let r = rig(None, &[]);
    let f = r.file("a.rs");
    r.m.open(&f, "rust", String::new(), 1, None).await;
    let managed = r.base.join("lsp/rust-analyzer/9.9.9");
    std::fs::create_dir_all(&managed).unwrap();
    std::fs::write(managed.join("rust-analyzer"), "x").unwrap();
    assert!(r.m.uninstall("rust-analyzer").await.unwrap());
    assert!(!r.base.join("lsp/rust-analyzer").exists());
    assert_eq!(r.m.instance_count(), 0);
    assert!(r.m.pids().is_empty());
    // A second uninstall has nothing to remove; an unknown name is an error.
    assert!(!r.m.uninstall("rust-analyzer").await.unwrap());
    assert!(r.m.uninstall("nope").await.is_err());
}

#[tokio::test]
async fn explicit_install_reports_progress_and_surfaces_failure() {
    let mut r = rig(
        Some(
            Failure::new("offline", "Could not download rust-analyzer")
                .hint("Check the connection."),
        ),
        &[],
    );
    let err = r.m.install("rust-analyzer", false).await.unwrap_err();
    assert_eq!(err.code, "offline");
    // A `downloading` status with progress was pushed while it ran, then removed.
    let mut saw = false;
    while let Ok(p) = r.rx.try_recv() {
        if let Push::Status(v) = p
            && v.iter().any(|s| {
                s.name == "rust-analyzer"
                    && s.state == State::Downloading
                    && s.progress.as_deref() == Some("Downloading fake")
            })
        {
            saw = true;
        }
    }
    assert!(saw, "no downloading status with progress");
    assert!(r.m.status().iter().all(|s| s.name != "rust-analyzer"));
    assert_eq!(
        r.m.install("zzz", false).await.unwrap_err().code,
        "install_failed"
    );
}

#[tokio::test]
async fn reinstall_removes_the_managed_copy_before_resolving_again() {
    let r = rig(None, &[]);
    let managed = r.base.join("lsp/rust-analyzer/old");
    std::fs::create_dir_all(&managed).unwrap();
    std::fs::write(managed.join("rust-analyzer"), "stale").unwrap();
    r.m.install("rust-analyzer", true).await.unwrap();
    assert!(!managed.exists(), "stale managed copy survived a reinstall");
    // A plain install never deletes anything.
    std::fs::create_dir_all(&managed).unwrap();
    r.m.install("rust-analyzer", false).await.unwrap();
    assert!(managed.exists());
}

// --- go to definition across files and outside the project root --------------------------------
//
// The user's report: "⌘-click does not take me to the file". Same-file definitions were covered; a
// definition in *another* file, in a file *outside the root* (cargo registry, rustup std, a global
// node_modules) and as a `LocationLink` (what typescript-language-server sends) were not. These pin
// the host side of the round trip: what the desktop receives from `lsp_request` is the server's reply
// untouched, with absolute file URIs it can resolve to a tab.

fn uri_path(uri: &str) -> String {
    atlas_lsp::caps::path_from_uri(uri).expect("a file: URI")
}

#[tokio::test]
async fn definition_in_another_file_of_the_project_is_returned_with_its_exact_range() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("util.rs");
    std::fs::write(&other, "// helpers\n\nfn helper(x: i32) {}\n").unwrap();
    let mut r = rig(None, &[("FAKE_XFILE_DIR", dir.path().to_str().unwrap())]);
    let f = r.file("main.rs");
    let rep =
        r.m.open(
            &f,
            "rust",
            "fn main() {\n    helper(1);\n}\n".into(),
            1,
            None,
        )
        .await;
    assert_eq!(rep.status, "ready");
    r.diagnostics(&f, |_| true).await;

    let def = r
        .req(
            &f,
            "textDocument/definition",
            json!({"textDocument": td(&f), "position": at(1, 6)}),
        )
        .await;
    let link = &def[0];
    // A LocationLink: the *name* is the selection range, the whole item the target range.
    assert_eq!(
        uri_path(link["targetUri"].as_str().unwrap()),
        other.to_string_lossy()
    );
    assert_eq!(link["targetSelectionRange"]["start"], at(2, 3));
    assert_eq!(link["targetSelectionRange"]["end"], at(2, 9));
    assert_eq!(link["targetRange"]["start"], at(2, 0));
}

#[tokio::test]
async fn definition_outside_the_project_root_is_not_dropped_by_the_host() {
    // The target lives in a directory that is not under the project root at all (rustup's std sources,
    // ~/.cargo/registry, a global node_modules). The host must hand it through unchanged.
    let outside = tempfile::tempdir().unwrap();
    let std_src = outside.path().join("lib/rustlib/src/rust/library/core/src");
    std::fs::create_dir_all(&std_src).unwrap();
    let option = std_src.join("option.rs");
    std::fs::write(
        &option,
        "pub enum Option<T> {}\nfn unwrap_or_default() {}\n",
    )
    .unwrap();
    let mut r = rig(None, &[("FAKE_XFILE_DIR", std_src.to_str().unwrap())]);
    let f = r.file("main.rs");
    r.m.open(
        &f,
        "rust",
        "fn main() {\n    unwrap_or_default();\n}\n".into(),
        1,
        None,
    )
    .await;
    r.diagnostics(&f, |_| true).await;

    let def = r
        .req(
            &f,
            "textDocument/definition",
            json!({"textDocument": td(&f), "position": at(1, 8)}),
        )
        .await;
    let target = uri_path(def[0]["targetUri"].as_str().unwrap());
    assert_eq!(target, option.to_string_lossy());
    assert!(
        !target.starts_with(r.project.to_str().unwrap()),
        "must be outside the project root"
    );
    assert!(
        std::path::Path::new(&target).is_file(),
        "the desktop can read it"
    );
    assert_eq!(def[0]["targetSelectionRange"]["start"], at(1, 3));
}

#[tokio::test]
async fn definition_with_a_space_and_unicode_in_the_path_round_trips_percent_encoded() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("My Pro\u{e1}ject");
    std::fs::create_dir_all(&odd).unwrap();
    std::fs::write(odd.join("c\u{f3}digo.rs"), "fn saudacao() {}\n").unwrap();
    let mut r = rig(None, &[("FAKE_XFILE_DIR", odd.to_str().unwrap())]);
    let f = r.file("main.rs");
    r.m.open(&f, "rust", "fn main() { saudacao(); }\n".into(), 1, None)
        .await;
    r.diagnostics(&f, |_| true).await;
    let def = r
        .req(
            &f,
            "textDocument/definition",
            json!({"textDocument": td(&f), "position": at(0, 13)}),
        )
        .await;
    let uri = def[0]["targetUri"].as_str().unwrap();
    assert!(
        uri.contains("My%20Pro"),
        "spaces must be encoded in the URI: {uri}"
    );
    assert_eq!(uri_path(uri), odd.join("c\u{f3}digo.rs").to_string_lossy());
}

#[tokio::test]
async fn no_definition_is_null_not_an_error() {
    let mut r = rig(None, &[]);
    let f = r.file("a.rs");
    r.m.open(&f, "rust", "fn main() { nowhere(); }\n".into(), 1, None)
        .await;
    r.diagnostics(&f, |_| true).await;
    let def = r
        .req(
            &f,
            "textDocument/definition",
            json!({"textDocument": td(&f), "position": at(0, 13)}),
        )
        .await;
    assert!(
        def.is_null() || def.as_array().is_some_and(Vec::is_empty),
        "{def}"
    );
}

// --- dependency and std sources reuse the server of the project that jumped there ----------------
//
// Found validating ⌘-click in the desktop app: opening Rust's std `path.rs` (or a `node_modules`
// `.d.ts`) in an editor started a *second* rust-analyzer / tsserver rooted at the dependency, one per
// opened library file. It is slow, wasteful, and the second server knows nothing of the project.

#[tokio::test]
async fn a_dependency_file_is_served_by_the_projects_server_not_a_new_one_rooted_there() {
    let mut r = rig(None, &[]);
    let f = r.file("main.rs");
    r.m.open(&f, "rust", "fn main() {}\n".into(), 1, None).await;
    r.diagnostics(&f, |_| true).await;
    assert_eq!(r.m.status().len(), 1);

    // A registry crate, outside the project, with its own Cargo.toml (the usual layout).
    let dep = tempfile::tempdir().unwrap();
    let crate_dir = dep
        .path()
        .join(".cargo/registry/src/index.crates.io-1/serde-1.0.200");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();
    std::fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"serde\"\n").unwrap();
    let lib = crate_dir.join("src/lib.rs");
    std::fs::write(&lib, "").unwrap();
    let lib = lib.to_string_lossy().into_owned();

    let rep =
        r.m.open(&lib, "rust", "pub fn serialize() {}\n".into(), 1, None)
            .await;
    assert_eq!(rep.status, "ready");
    let st = r.m.status();
    assert_eq!(st.len(), 1, "no second server for a dependency: {st:#?}");
    assert_eq!(st[0].root, r.project.to_string_lossy());
    assert_eq!(r.m.open_doc_count(), 2);
}

#[tokio::test]
async fn a_dependency_file_with_no_running_server_still_gets_its_own() {
    // Opened first (a restored tab at startup): there is no project server to borrow, so it must still work.
    let r = rig(None, &[]);
    let dep = tempfile::tempdir().unwrap();
    let crate_dir = dep.path().join(".cargo/registry/src/x/serde-1.0.200");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();
    std::fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"serde\"\n").unwrap();
    let lib = crate_dir.join("src/lib.rs").to_string_lossy().into_owned();
    let rep =
        r.m.open(&lib, "rust", "pub fn a() {}\n".into(), 1, None)
            .await;
    assert_eq!(rep.status, "ready");
    assert_eq!(r.m.status().len(), 1);
}

#[tokio::test]
async fn a_dependency_of_another_language_does_not_borrow_a_server_of_the_wrong_kind() {
    let mut r = rig(None, &[]);
    let f = r.file("main.rs");
    r.m.open(&f, "rust", "fn main() {}\n".into(), 1, None).await;
    r.diagnostics(&f, |_| true).await;
    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();
    r.m.open(&ts, "typescript", "export {}\n".into(), 1, None)
        .await;
    // Rust's server never serves TypeScript: a second server (the TS one) is right here.
    assert_eq!(r.m.status().len(), 2);
}

// The real startup order: restored tabs open at once, a dependency tab usually *before* the project
// file has finished starting its server. The client says which worktree opened each document
// (`lsp_open.root`); a dependency file is then served from that worktree's root, so whichever file
// arrives first, the dependency never roots a server of its own.

#[tokio::test]
async fn dependency_and_project_files_opened_concurrently_share_one_server() {
    let r = rig(None, &[]);
    let f = r.file("main.rs");
    let dep = tempfile::tempdir().unwrap();
    let crate_dir = dep.path().join(".cargo/registry/src/x/serde-1.0.200");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();
    std::fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"serde\"\n").unwrap();
    let lib = crate_dir.join("src/lib.rs").to_string_lossy().into_owned();

    // Dependency first, as tabs restore; both carry the worktree that opened them.
    let (a, b) = tokio::join!(
        r.m.open(&lib, "rust", "pub fn a() {}\n".into(), 1, Some(&r.project)),
        async {
            tokio::task::yield_now().await;
            r.m.open(&f, "rust", "fn main() {}\n".into(), 1, Some(&r.project))
                .await
        }
    );
    assert_eq!((a.status, b.status), ("ready", "ready"));
    let st = r.m.status();
    assert_eq!(st.len(), 1, "one server for both files: {st:#?}");
    assert_eq!(st[0].root, r.project.to_string_lossy());
    assert_eq!(r.m.open_doc_count(), 2);
}

#[tokio::test]
async fn a_dependency_file_never_roots_a_server_in_the_dependency_when_a_worktree_is_known() {
    let r = rig(None, &[]);
    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();
    r.m.open(&ts, "typescript", "export {}\n".into(), 1, Some(&r.project))
        .await;
    let st = r.m.status();
    assert_eq!(st.len(), 1);
    assert_eq!(
        st[0].root,
        r.project.to_string_lossy(),
        "rooted at the worktree, not at {}",
        pkg.display()
    );
}

// A worktree root is not always where a language's project lives (a monorepo: `package.json` in
// `apps/desktop`). A dependency file opened with the worktree as fallback must join the project's
// server found *under* that worktree, not start a second one at the worktree root.

#[tokio::test]
async fn a_dependency_joins_the_server_of_a_project_under_the_worktree_root() {
    let mut r = rig(None, &[]);
    // Worktree = r.project; the TS project lives in a subfolder with its own package.json.
    let app = r.project.join("apps/web");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    let f = app.join("src/a.ts");
    std::fs::write(&f, "").unwrap();
    let f = f.to_string_lossy().into_owned();
    r.m.open(
        &f,
        "typescript",
        "export const a = 1;\n".into(),
        1,
        Some(&r.project),
    )
    .await;
    r.diagnostics(&f, |_| true).await;

    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();
    r.m.open(&ts, "typescript", "export {}\n".into(), 1, Some(&r.project))
        .await;

    let st = r.m.status();
    assert_eq!(
        st.len(),
        1,
        "one TS server for the project and its dependency: {st:#?}"
    );
    assert_eq!(st[0].root, app.to_string_lossy());
}

#[tokio::test]
async fn a_dependency_opened_first_moves_to_the_project_server_once_one_exists() {
    let mut r = rig(None, &[]);
    let app = r.project.join("apps/web");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    let f = app.join("src/a.ts");
    std::fs::write(&f, "").unwrap();
    let f = f.to_string_lossy().into_owned();

    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();

    // The dependency tab restores first: nothing runs, so it is served from the worktree root...
    r.m.open(&ts, "typescript", "export {}\n".into(), 1, Some(&r.project))
        .await;
    assert_eq!(r.m.status().len(), 1);
    // ...then the project's own file opens: its server (at the package root) takes the dependency over,
    // and the worktree-root instance, now without documents, goes away.
    r.m.open(
        &f,
        "typescript",
        "export const a = 1;\n".into(),
        1,
        Some(&r.project),
    )
    .await;
    r.diagnostics(&f, |_| true).await;

    tokio::time::sleep(Duration::from_millis(900)).await; // past `idle_shutdown` (300 ms in the rig)
    let st: Vec<_> =
        r.m.status()
            .into_iter()
            .filter(|s| s.state == State::Ready)
            .collect();
    assert_eq!(st.len(), 1, "one live TS server: {st:#?}");
    assert_eq!(st[0].root, app.to_string_lossy());
    assert_eq!(r.m.open_doc_count(), 2, "both documents stay open");
}

// Restored tabs open in any order. The project file may open *before* the dependency that was open
// when the window closed: the dependency then finds the project's server already running and must be
// served by it, whichever root the worktree fallback would have given it.

#[tokio::test]
async fn a_dependency_opened_after_the_project_file_uses_the_project_server_even_while_it_starts() {
    let r = rig(None, &[]);
    let app = r.project.join("apps/web");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    let f = app.join("src/a.ts");
    std::fs::write(&f, "").unwrap();
    let f = f.to_string_lossy().into_owned();
    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();

    // Both in flight at once; the project file wins the race to create its instance.
    let (a, b) = tokio::join!(
        r.m.open(
            &f,
            "typescript",
            "export const a = 1;\n".into(),
            1,
            Some(&r.project)
        ),
        async {
            tokio::task::yield_now().await;
            r.m.open(&ts, "typescript", "export {}\n".into(), 1, Some(&r.project))
                .await
        }
    );
    assert_eq!((a.status, b.status), ("ready", "ready"));
    tokio::time::sleep(Duration::from_millis(900)).await;
    let st = r.m.status();
    assert_eq!(st.len(), 1, "one TS server: {st:#?}");
    assert_eq!(st[0].root, app.to_string_lossy());
    assert_eq!(r.m.open_doc_count(), 2);
}

#[tokio::test]
async fn a_dependency_arriving_while_the_project_server_is_still_starting_joins_it() {
    // A resolver that takes a moment (a first-time download): the instance exists but is not live yet.
    let r = rig(None, &[("FAKE_INIT_DELAY_MS", "400")]);
    let app = r.project.join("apps/web");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    let f = app.join("src/a.ts");
    std::fs::write(&f, "").unwrap();
    let f = f.to_string_lossy().into_owned();
    let dep = tempfile::tempdir().unwrap();
    let pkg = dep.path().join("node_modules/pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), "{}").unwrap();
    let ts = pkg.join("index.d.ts").to_string_lossy().into_owned();

    let (a, b) = tokio::join!(
        r.m.open(
            &f,
            "typescript",
            "export const a = 1;\n".into(),
            1,
            Some(&r.project)
        ),
        async {
            tokio::time::sleep(Duration::from_millis(120)).await; // project instance exists, still starting
            r.m.open(&ts, "typescript", "export {}\n".into(), 1, Some(&r.project))
                .await
        }
    );
    assert_eq!((a.status, b.status), ("ready", "ready"));
    tokio::time::sleep(Duration::from_millis(900)).await;
    let st = r.m.status();
    assert_eq!(st.len(), 1, "one TS server: {st:#?}");
    assert_eq!(st[0].root, app.to_string_lossy());
}
