//! The crew server's GitHub App through a real hostd and a fake GitHub on loopback (#643, #644): the commands as a
//! client sends them (and the payloads the desktop reads, written as fixtures), and what a crewmate's agent gets: a git
//! credential helper and a bot identity in its environment, never a token, and a token from hostd for each git operation.
//! Regenerate the desktop fixture: UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::github_app

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use serde_json::{Value, json};

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::crew::github::app::fake;
use crate::crew::github::app::helper;

/// Points the app's HTTP at a fake GitHub for as long as it lives. Hold [`lock_agent_bin`] too: the seam is process-wide.
struct Base;

impl Base {
    fn set(url: &str) -> Self {
        *crate::crew::github::app::TEST_API_BASE.lock() = Some(url.to_string());
        Self
    }
}

impl Drop for Base {
    fn drop(&mut self) {
        *crate::crew::github::app::TEST_API_BASE.lock() = None;
        // The fake engine of a test lives in its temp dir: left set, the next test would start a path that is gone.
        crate::crew::engine::set_test_engine_bin(None);
    }
}

async fn fake_github() -> (fake::Fake, Arc<AtomicUsize>) {
    let mints = Arc::new(AtomicUsize::new(0));
    let in_an_hour = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(1))
        .unwrap();
    let f = fake::serve(fake::github(mints.clone(), in_an_hour.to_string())).await;
    (f, mints)
}

fn fixture(name: &str, live: &Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/lib/__fixtures__")
        .join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{name} missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::github_app")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1"
    );
}

fn coded(e: &anyhow::Error) -> (&'static str, Value) {
    let c = e
        .downcast_ref::<atlas_core::progress::Coded>()
        .unwrap_or_else(|| panic!("not a coded failure: {e}"));
    (c.code, Value::Object(c.args.clone()))
}

// Contract with the desktop (`apps/desktop/src/lib/__fixtures__/crew-github-app.json`): the manifest it posts to
// GitHub and the status it renders, as a real hostd answers them over the wire; and the coded failures of the flow.
#[tokio::test(flavor = "multi_thread")]
async fn the_app_commands_over_the_wire_and_their_desktop_fixture() {
    let _lock = lock_agent_bin().await;
    let (gh, _) = fake_github().await;
    let _base = Base::set(&gh.base);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    let none = client
        .call("crew_github_app_status", json!({}))
        .await
        .unwrap();
    assert_eq!(none, json!({"app": null}));

    let user = client
        .call(
            "crew_github_app_manifest",
            json!({"owner_kind": "user", "server_name": "my box"}),
        )
        .await
        .unwrap();
    let org = client
        .call(
            "crew_github_app_manifest",
            json!({"owner_kind": "org", "org": "acme", "workflows": true, "server_name": "my box"}),
        )
        .await
        .unwrap();
    assert_eq!(user["post_url"], "https://github.com/settings/apps/new");
    assert_eq!(user["manifest"]["public"], false);
    assert!(
        user["manifest"]["default_permissions"]
            .get("workflows")
            .is_none()
    );
    assert_eq!(org["manifest"]["default_permissions"]["workflows"], "write");
    assert_eq!(org["server_name"], "my box");

    // A bad org is a coded failure the desktop shows in its own words.
    let e = client
        .call(
            "crew_github_app_manifest",
            json!({"owner_kind": "org", "org": "a/b"}),
        )
        .await
        .unwrap_err();
    assert_eq!(coded(&e).0, "hostd.invalid_argument");

    // A code GitHub has already used.
    let e = client
        .call("crew_github_app_convert", json!({"code": "oldcode"}))
        .await
        .unwrap_err();
    assert_eq!(coded(&e), ("crew.github_app.code_expired", json!({})));

    let converted = client
        .call("crew_github_app_convert", json!({"code": "goodcode"}))
        .await
        .unwrap();
    let status = client
        .call("crew_github_app_status", json!({}))
        .await
        .unwrap();
    assert_eq!(converted, status);
    let text = status.to_string();
    for secret in [
        fake::WEBHOOK_SECRET,
        fake::CLIENT_SECRET,
        "PRIVATE KEY",
        "\"pem\"",
    ] {
        assert!(!text.contains(secret), "status leaks {secret}");
    }

    // The key is in the file only the crew user reads, and in no reply.
    let file = dir.join("github-app.json");
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("PRIVATE KEY")
    );

    let e = client
        .call("crew_github_app_convert", json!({"code": "goodcode"}))
        .await
        .unwrap_err();
    assert_eq!(coded(&e).0, "crew.github_app.exists");

    fixture(
        "crew-github-app.json",
        &json!({
            "manifest_user": user,
            "manifest_org_with_workflows": org,
            "status": status,
            "status_without_app": none,
            "errors": {
                "code_expired": {"code": "crew.github_app.code_expired", "args": {}},
                "exists": {"code": "crew.github_app.exists", "args": {}},
            },
        }),
    );

    client
        .call("crew_github_app_remove", json!({}))
        .await
        .unwrap();
    assert!(!file.exists());
    let e = client
        .call("crew_github_app_remove", json!({}))
        .await
        .unwrap_err();
    assert_eq!(coded(&e).0, "crew.github_app.none");
    assert_eq!(
        client
            .call("crew_github_app_status", json!({}))
            .await
            .unwrap(),
        json!({"app": null})
    );
}

// What a crewmate gets: the helper and the bot in its environment, no token anywhere in it; the token comes from
// hostd, through the crewmate's own authenticated route, scoped to the repo git names, one operation at a time.
#[tokio::test(flavor = "multi_thread")]
async fn a_crewmates_env_has_the_helper_and_the_bot_but_never_a_token_and_hostd_answers_the_helper()
{
    let _lock = lock_agent_bin().await;
    let (gh, mints) = fake_github().await;
    let _base = Base::set(&gh.base);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    std::fs::create_dir_all(&dir).unwrap();
    fake::install_app(&dir);

    let env_dump = tmp.path().join("env_dump.txt");
    let script = format!(
        r#"env > "{}"
printf '{{"type":"result","session_id":"s1","result":"done","is_error":false}}\n'
exit 0"#,
        env_dump.display()
    );
    let bin = fake_agent(tmp.path(), "app-engine", &script);
    crate::crew::engine::set_test_engine_bin(Some(&bin.to_string_lossy()));

    let client = daemon(&dir).await;
    let created = client
        .call(
            "crewmate_create",
            json!({"name": "AppMate", "role": "r", "agent": "claude", "approval": "auto",
                   "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    let t = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = t["task"]["id"].as_str().unwrap().to_string();
    let task = wait_task_state(&client, &id, &task_id, "done", atlas_core::scaling::HANG).await;
    assert_eq!(task["state"], "done");

    let dump = std::fs::read_to_string(&env_dump).unwrap();
    let env: BTreeMap<String, String> = dump
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    assert_eq!(env["GIT_AUTHOR_NAME"], "atlas-crew-box[bot]");
    assert_eq!(env["GIT_COMMITTER_NAME"], "atlas-crew-box[bot]");
    assert_eq!(
        env["GIT_AUTHOR_EMAIL"],
        "4242+atlas-crew-box[bot]@users.noreply.github.com"
    );
    assert!(
        env["GIT_CONFIG_VALUE_2"].ends_with(" crew git-credential"),
        "{}",
        env["GIT_CONFIG_VALUE_2"]
    );
    // No token, no password, nothing that was ever minted, in anything the agent was started with.
    assert!(
        !dump.contains("ghs_"),
        "a token is in the agent's environment"
    );
    assert!(!dump.contains("x-access-token"));
    assert_eq!(
        mints.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "no token is made to start a task"
    );

    // git asks the helper: the helper asks hostd over the agent's own hook endpoint.
    let lookup = move |k: &str| env.get(k).cloned();
    let (out, err) = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        helper::get(
            "protocol=https\nhost=github.com\npath=acme/web.git\n\n".as_bytes(),
            &mut out,
            &mut err,
            &lookup,
        )
        .unwrap();
        // A repo the app is not installed on: nothing is printed, and the reason is.
        let mut other = Vec::new();
        let mut other_err = Vec::new();
        helper::get(
            "protocol=https\nhost=github.com\npath=other/repo.git\n\n".as_bytes(),
            &mut other,
            &mut other_err,
            &lookup,
        )
        .unwrap();
        assert!(other.is_empty());
        assert!(
            String::from_utf8_lossy(&other_err).contains("not installed on other/repo"),
            "{}",
            String::from_utf8_lossy(&other_err)
        );
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    })
    .await
    .unwrap();
    assert_eq!(
        out, "username=x-access-token\npassword=ghs_token_1\n",
        "{err}"
    );

    // The token was made for the one repo, and audited without being written.
    let mint = gh
        .seen()
        .into_iter()
        .find(|r| r.path == "/app/installations/7/access_tokens")
        .unwrap();
    assert_eq!(mint.body["repositories"], json!(["web"]));
    let audit = std::fs::read_to_string(dir.join("crew-audit.jsonl")).unwrap();
    assert!(
        audit.contains("github_token_minted") && audit.contains("acme/web"),
        "{audit}"
    );
    assert!(!audit.contains("ghs_"));
}
