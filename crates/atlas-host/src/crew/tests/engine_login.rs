//! `crew_engine_login` / `crew_engine_status`: a phone signs an engine in on the crew server through a
//! terminal that runs ONLY that engine's login command, and an expired sign-in is noticed (from a status
//! check or from a task that failed with the engine's own auth error).
//! Fake engines are `/bin/sh` scripts printing the real strings the CLIs print (see `engine_login::ENGINES`).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::crew::engine_login::{self, ENGINES};

fn engine<'a>(status: &'a Value, name: &str) -> &'a Value {
    status["engines"]
        .as_array()
        .unwrap_or_else(|| panic!("no engines array: {status}"))
        .iter()
        .find(|e| e["engine"] == name)
        .unwrap_or_else(|| panic!("no {name} in {status}"))
}

/// A crew server whose engine CLIs are fakes in `<tmp>/bin` first on PATH: `claude auth status` exits `code`.
async fn server_with_claude(tmp: &Path, status_exit: i32) -> crate::HostClient {
    fake_agent(
        tmp,
        "claude",
        &format!(
            r#"if [ "$1 $2" = "auth status" ]; then echo '{{"loggedIn":false}}'; exit {status_exit}; fi
echo "ran:$*""#
        ),
    );
    engine_login::set_test_path(Some(tmp.join("bin")));
    daemon(&tmp.join("hostd")).await
}

// Incident (PhoneCrewAudit gap 2): the action card said "Sign in to claude in Settings" but the phone had no
// way to do it. The command opens a pty that runs the engine's login argv and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn engine_login_opens_a_pty_running_only_that_engines_login_command() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    // Every engine installed (the login terminal needs the program on PATH).
    for e in ENGINES {
        fake_agent(tmp.path(), e.id, "sleep 600");
    }
    for e in ENGINES {
        let v = client
            .call("crew_engine_login", json!({"engine": e.id}))
            .await
            .unwrap_or_else(|err| panic!("{}: {err}", e.id));
        let pty = v["pty"].as_str().expect("pty id").to_string();
        assert_eq!(v["engine"], e.id);
        let list = client.call("pty_list", json!({})).await.unwrap();
        let p = list["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["pty"] == pty)
            .unwrap_or_else(|| panic!("pty {pty} not listed: {list}"));
        let argv: Vec<String> = serde_json::from_value(p["argv"].clone()).unwrap();
        assert_eq!(
            argv, e.login,
            "{}: the pty runs the allowlisted login argv",
            e.id
        );
        // Not an agent pane: a login terminal must never report to the hook server as a crewmate.
        assert!(p["scope"]["pane_key"].is_null(), "{}: {p}", e.id);
        client.call("pty_kill", json!({"pty": pty})).await.unwrap();
    }
}

// The allowlist is the security boundary: a phone with the Act scope must not turn this into a shell.
#[tokio::test(flavor = "multi_thread")]
async fn engine_login_refuses_anything_not_on_the_allowlist() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    for bad in [
        "bash",
        "sh -c id",
        "../claude",
        "CLAUDE",
        "",
        "claude ",
        "gh",
    ] {
        let err = client
            .call("crew_engine_login", json!({"engine": bad}))
            .await
            .expect_err(bad);
        assert!(
            format!("{err:#}").contains("unknown engine"),
            "{bad:?}: {err:#}"
        );
    }
    let list = client.call("pty_list", json!({})).await.unwrap();
    assert!(
        list["ptys"].as_array().unwrap().is_empty(),
        "no pty for a refused engine: {list}"
    );
}

// Real strings: `claude auth status` exits 1 when not logged in (docs: code.claude.com/docs/en/cli-reference).
#[tokio::test(flavor = "multi_thread")]
async fn engine_status_reads_each_engines_own_status_command() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    let s = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(engine(&s, "claude")["state"], "needs_signin", "{s}");
    // The others are not installed under the fake PATH.
    assert_eq!(engine(&s, "codex")["state"], "not_installed", "{s}");
    assert_eq!(s["engines"].as_array().unwrap().len(), ENGINES.len());

    fake_agent(
        tmp.path(),
        "claude",
        r#"if [ "$1 $2" = "auth status" ]; then echo '{"loggedIn":true}'; exit 0; fi"#,
    );
    let s = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(engine(&s, "claude")["state"], "signed_in", "{s}");
}

// The real strings each CLI prints when its sign-in is gone (sources in `engine_login::ENGINES`).
#[test]
fn auth_errors_are_recognised_by_each_engines_own_signature() {
    let cases = [
        (
            "claude",
            "claude: agent error: Please run /login · API Error: 401 OAuth token has expired",
        ),
        (
            "claude",
            "claude: agent error: Not logged in · Please run /login",
        ),
        (
            "claude",
            "claude: agent error: Login expired · Please run /login",
        ),
        (
            "claude",
            "claude: agent error: Invalid API key · Fix external API key",
        ),
        (
            "codex",
            "codex: exited 1: Your access token could not be refreshed because your refresh token has expired. Please log out and sign in again.",
        ),
        (
            "codex",
            "codex: exited 1: Your access token could not be refreshed because your refresh token was revoked. Please log out and sign in again.",
        ),
        ("codex", "codex: exited 1: Not logged in"),
        ("omp", "omp: No API key for provider: anthropic"),
        ("opencode", "opencode: ProviderAuthError: Unauthorized"),
    ];
    for (engine, text) in cases {
        assert!(
            engine_login::is_auth_error(engine, text),
            "{engine}: {text}"
        );
    }
    // A different failure never flips the sign-in, and one engine's text is not another's.
    for (engine, text) in [
        ("claude", "claude: exited 1: ENOSPC no space left on device"),
        ("claude", "claude: agent error: API Error: 529 overloaded"),
        (
            "codex",
            "codex: exited 1: rate limit exceeded, try again later",
        ),
        ("omp", "omp: spawn failed: No such file or directory"),
        ("codex", "claude: agent error: Please run /login"),
        // Shared wording: codex also says "Not logged in", which is a claude signature too. A codex failure
        // flips codex only; claude's own prefix is what makes the text claude's.
        ("claude", "codex: exited 1: Not logged in"),
        ("omp", "opencode: ProviderAuthError: Unauthorized"),
    ] {
        assert!(
            !engine_login::is_auth_error(engine, text),
            "{engine}: {text}"
        );
    }
}

// Incident (CrewOSAudit #12): a sign-in that expired mid-week failed every task with a bare error and the
// settings kept saying "signed in". The task's own failure flips the engine to needs_signin, with no
// status command run (`refresh` is false), and a push tells the phones.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_failing_with_the_engines_auth_error_flips_it_to_needs_signin() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    // `claude -p` prints a result JSON with is_error=true, as the real CLI does when the login expired.
    let script = r#"if [ "$1 $2" = "auth status" ]; then exit 0; fi
echo '{"type":"result","is_error":true,"result":"Please run /login · API Error: 401 OAuth token has expired","session_id":"s"}'"#;
    let bin = fake_agent(tmp.path(), "claude", script);
    engine_login::set_test_path(Some(tmp.path().join("bin")));
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
        Some(("claude".into(), bin.to_string_lossy().into_owned()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Ana", "role": "r", "agent": "claude", "approval": "auto",
                   "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let s = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(
        engine(&s, "claude")["state"],
        "signed_in",
        "starts signed in: {s}"
    );

    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "hi", "source": "test"}),
        )
        .await
        .unwrap();
    let task = t["task"]["id"].as_str().unwrap().to_string();
    let failed = wait_task_state(&client, &id, &task, "failed", atlas_core::scaling::HANG).await;
    assert!(
        failed["error"].as_str().unwrap().contains("/login"),
        "{failed}"
    );

    let s = client.call("crew_engine_status", json!({})).await.unwrap();
    assert_eq!(engine(&s, "claude")["state"], "needs_signin", "{s}");
    assert!(
        engine(&s, "claude")["reason"]
            .as_str()
            .unwrap()
            .contains("/login"),
        "{s}"
    );
    // The person signs in (here: the engine's status command now says so) and the next refresh clears the
    // flag: the row must not stay "Needs sign-in" for a sign-in that is fixed.
    fake_agent(
        tmp.path(),
        "claude",
        r#"if [ "$1 $2" = "auth status" ]; then exit 0; fi"#,
    );
    let s = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(engine(&s, "claude")["state"], "signed_in", "{s}");
    assert!(engine(&s, "claude").get("reason").is_none(), "{s}");
    let s = client.call("crew_engine_status", json!({})).await.unwrap();
    assert_eq!(
        engine(&s, "claude")["state"],
        "signed_in",
        "stays fixed without a refresh: {s}"
    );
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
}

/// Compare `live` with `apps/mobile/test/fixtures/<name>`; `UPDATE_FIXTURES=1` rewrites the file first.
fn fixture(name: &str, live: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures")
        .join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{name} missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::engine_login")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1"
    );
}

// Contract with the phone (`apps/mobile/lib/core/protocol/crew_ops_models.dart` EngineStatus / EngineLogin): the
// replies as a real hostd answers them, read by `test/crew_engine_signin_contract_test.dart`.
#[tokio::test(flavor = "multi_thread")]
async fn engine_fixtures_are_real_payloads() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    // claude signed in, codex signed out (its status exits 1 with "Not logged in"), omp/opencode absent.
    fake_agent(
        tmp.path(),
        "claude",
        r#"if [ "$1 $2" = "auth status" ]; then exit 0; fi"#,
    );
    fake_agent(
        tmp.path(),
        "codex",
        r#"if [ "$1 $2" = "login status" ]; then echo "Not logged in" >&2; exit 1; fi"#,
    );
    engine_login::set_test_path(Some(tmp.path().join("bin")));
    let client = daemon(&tmp.path().join("hostd")).await;

    let status = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(engine(&status, "claude")["state"], "signed_in", "{status}");
    assert_eq!(
        engine(&status, "codex")["state"],
        "needs_signin",
        "{status}"
    );
    fixture("crew_engine_status.json", &status);

    let mut login = client
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .unwrap();
    assert!(
        login["pty"].as_str().is_some_and(|p| !p.is_empty()),
        "{login}"
    );
    // The pty id is random: the phone only passes it on, so the fixture pins its shape, not its value.
    login["pty"] = json!("pty-fixture");
    fixture("crew_engine_login.json", &login);
    client.call("shutdown", json!({})).await.ok();
}

// A phone behind a Mac's relay cannot attach a terminal (terminals are never relayed), so a sign-in pty started
// that way would be an orphan login nobody can reach. The status relays; the login needs the phone paired to the
// crew server itself (`atlas crew pair`), where it is `Act` like the other writes.
#[test]
fn the_login_needs_act_and_never_goes_through_a_relay_but_the_status_does() {
    use crate::mobile::{Scope, required_scope};
    let direct = |cmd: &str, args: Value| -> crate::protocol::HostCmd {
        serde_json::from_value(json!({"cmd": cmd, "args": args})).unwrap()
    };
    assert_eq!(
        required_scope(&direct("crew_engine_login", json!({"engine": "claude"}))),
        Ok(Scope::Act)
    );
    assert_eq!(
        required_scope(&direct("crew_engine_status", json!({"refresh": true}))),
        Ok(Scope::Read)
    );
    let relayed = |cmd: &str, args: Value| -> crate::protocol::HostCmd {
        direct(
            "crew_call",
            json!({"server": "s", "cmd": cmd, "args": args}),
        )
    };
    assert_eq!(
        required_scope(&relayed("crew_engine_status", json!({}))),
        Ok(Scope::Read)
    );
    assert!(
        required_scope(&relayed("crew_engine_login", json!({"engine": "claude"}))).is_err(),
        "a relayed sign-in would start a terminal the phone cannot attach"
    );
}

// An engine that is not installed cannot be signed in: the spawn error comes back as the reply (the phone shows
// it) and no terminal is left behind.
#[tokio::test(flavor = "multi_thread")]
async fn engine_login_for_an_engine_that_is_not_installed_fails_without_leaving_a_pty() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await; // only claude exists
    let err = client
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .expect_err("codex is not installed");
    assert!(format!("{err:#}").contains("codex"), "{err:#}");
    let list = client.call("pty_list", json!({})).await.unwrap();
    assert!(list["ptys"].as_array().unwrap().is_empty(), "{list}");
}

// Incident-in-waiting: the phone's terminal screen only detaches, so every tap on "Sign in" left a pty in
// `pty_list` until hostd restarted. A finished sign-in stays attachable for a while (the phone still shows
// what the CLI said) and is then removed by hostd itself; one still running is never touched.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_sign_in_terminal_is_removed_after_the_grace_and_a_running_one_is_not() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    // codex logs in and exits at once; opencode waits for a code that never comes.
    fake_agent(tmp.path(), "codex", "echo done");
    fake_agent(tmp.path(), "opencode", "sleep 600");
    engine_login::set_test_reap(Some(Duration::from_millis(300)));
    let quick = client
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .unwrap();
    let waiting = client
        .call("crew_engine_login", json!({"engine": "opencode"}))
        .await
        .unwrap();
    let (quick, waiting) = (
        quick["pty"].as_str().unwrap().to_string(),
        waiting["pty"].as_str().unwrap().to_string(),
    );
    let listed = |client: &crate::HostClient| {
        let client = client.clone();
        async move {
            let l = client.call("pty_list", json!({})).await.unwrap();
            l["ptys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["pty"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        }
    };
    let mut gone = false;
    for _ in 0..1200 {
        if !listed(&client).await.contains(&quick) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the finished sign-in terminal was never removed");
    assert!(
        listed(&client).await.contains(&waiting),
        "a sign-in still waiting for its code must stay"
    );
    engine_login::set_test_reap(None);
    client
        .call("pty_kill", json!({"pty": waiting}))
        .await
        .unwrap();
}

// "Installed" means the program resolves on the engine PATH, not that some OS error text matched: a directory
// named like the engine, or a file that is not executable, is not an installed engine.
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_or_a_non_executable_file_named_like_an_engine_is_not_installed() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    std::fs::create_dir(tmp.path().join("bin/codex")).unwrap();
    std::fs::write(tmp.path().join("bin/omp"), "#!/bin/sh\nexit 0\n").unwrap(); // no exec bit
    let s = client
        .call("crew_engine_status", json!({"refresh": true}))
        .await
        .unwrap();
    assert_eq!(engine(&s, "codex")["state"], "not_installed", "{s}");
    assert_eq!(engine(&s, "omp")["state"], "not_installed", "{s}");
}

// A sign-in is a sensitive act on the server: it lands in the crew audit with the engine and who asked, and
// nothing that was typed into the terminal (the phone's `by` is `phone:<node>`, the desktop's `desktop`).
#[tokio::test(flavor = "multi_thread")]
async fn starting_a_sign_in_is_audited_with_who_asked_and_never_what_is_typed() {
    let _g = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let client = server_with_claude(tmp.path(), 1).await;
    fake_agent(tmp.path(), "codex", "cat");
    let v = client
        .call("crew_engine_login", json!({"engine": "codex"}))
        .await
        .unwrap();
    let pty = v["pty"].as_str().unwrap().to_string();
    // A code typed into the terminal (what a person does on a device-code login) must reach no log.
    use base64::Engine as _;
    let typed = base64::engine::general_purpose::STANDARD.encode("ABCD-1234-SECRET-CODE\n");
    client
        .call("pty_write", json!({"pty": pty, "data": typed}))
        .await
        .unwrap();
    let audit = client
        .call("crew_audit", json!({"kind": "engine_login_started"}))
        .await
        .unwrap();
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{audit}");
    assert_eq!(entries[0]["engine"], "codex", "{audit}");
    assert_eq!(
        entries[0]["by"], "desktop",
        "a socket client is the desktop: {audit}"
    );
    assert!(
        !audit.to_string().contains("SECRET-CODE"),
        "keystrokes must never be logged: {audit}"
    );
    let all = client.call("crew_audit", json!({})).await.unwrap();
    assert!(!all.to_string().contains("SECRET-CODE"), "{all}");
    client.call("pty_kill", json!({"pty": pty})).await.unwrap();
    // Refused engines leave no entry.
    let _ = client
        .call("crew_engine_login", json!({"engine": "bash"}))
        .await;
    let audit = client
        .call("crew_audit", json!({"kind": "engine_login_started"}))
        .await
        .unwrap();
    assert_eq!(audit["entries"].as_array().unwrap().len(), 1, "{audit}");
}
