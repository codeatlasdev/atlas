//! The GitHub App against a fake GitHub on loopback: conversion and where the key goes, status, removal, the token
//! cache with a fake clock, the `GithubApi` that picks the app or `gh`, and the credential helper.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use super::api::Routed;
use super::fake::{self, Fake, Req};
use super::http::Http;
use super::record::{AppRecord, AppStore};
use super::service::{self, AppService, TokenError};
use super::tokens::Clock;
use crate::crew::audit::Audit;
use crate::crew::github::GithubApi;
use crate::crew::tests::github_fake::FakeGithub;

use super::fake::{CLIENT_SECRET, KEY, WEBHOOK_SECRET as SECRET, conversion};

/// 2023-11-14T22:13:20Z
const T0: i64 = 1_700_000_000;

struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The base64 body of the test key's second line: what must never leave the 0600 file.
fn key_marker() -> &'static str {
    KEY.lines().nth(2).unwrap().trim()
}

fn iso(t: i64) -> String {
    jiff::Timestamp::from_second(t).unwrap().to_string()
}

struct Rig {
    tmp: tempfile::TempDir,
    svc: Arc<AppService>,
    audit: Arc<Audit>,
    clock: Arc<FakeClock>,
    fake: Fake,
    mints: Arc<AtomicUsize>,
}

impl Rig {
    fn dir(&self) -> &std::path::Path {
        self.tmp.path()
    }

    fn audit_text(&self) -> String {
        std::fs::read_to_string(self.dir().join("crew-audit.jsonl")).unwrap_or_default()
    }
}

/// A server that answers like GitHub for an app installed on `acme/web` only (installation 7).
async fn rig() -> Rig {
    let mints = Arc::new(AtomicUsize::new(0));
    // Every token the fake mints expires at the same moment, T0 + 1 h: only its name changes.
    let fake = fake::serve(fake::github(mints.clone(), iso(T0 + 3600))).await;
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(Audit::new(tmp.path()));
    let clock = Arc::new(FakeClock(AtomicI64::new(T0)));
    let svc = Arc::new(AppService::with(
        tmp.path(),
        audit.clone(),
        Http::new(&fake.base),
        clock.clone(),
    ));
    Rig {
        tmp,
        svc,
        audit,
        clock,
        fake,
        mints,
    }
}

/// An app that is already set up (as a conversion would have stored it).
fn install(rig: &Rig) {
    fake::install_app(rig.dir());
}

fn jwt_claims(bearer: &str) -> Value {
    let payload = bearer.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files_under(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// ── #643: conversion, storage, status, removal ───────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn conversion_keeps_the_key_only_in_the_private_file_and_status_never_returns_it() {
    let rig = rig().await;
    let status = rig.svc.convert("goodcode").await.unwrap();

    // The file hostd keeps: the key, the webhook secret, the ids; owner-only.
    let file = rig.dir().join("github-app.json");
    let stored: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(stored["id"], 987654);
    assert_eq!(stored["client_id"], "Iv23liTESTCLIENT");
    assert_eq!(stored["slug"], "atlas-crew-box");
    assert_eq!(stored["owner"], "acme");
    assert_eq!(stored["webhook_secret"], SECRET);
    assert!(stored["pem"].as_str().unwrap().contains(key_marker()));
    // The client secret is of no use to hostd: it is not kept at all.
    assert!(stored.get("client_secret").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }

    // Nowhere else under the server's directory.
    for f in files_under(rig.dir()) {
        if f == file {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for secret in [key_marker(), SECRET, CLIENT_SECRET] {
            assert!(!text.contains(secret), "{} holds a secret", f.display());
        }
    }

    // What a client reads: the app, its identity, its installations; never a secret.
    let again = rig.svc.status().await.unwrap();
    assert_eq!(status, again);
    let text = again.to_string();
    for secret in [
        key_marker(),
        SECRET,
        CLIENT_SECRET,
        "pem",
        "webhook_secret",
        "client_secret",
    ] {
        assert!(!text.contains(secret), "status leaks {secret}");
    }
    let app = &again["app"];
    assert_eq!(app["slug"], "atlas-crew-box");
    assert_eq!(app["owner"], "acme");
    assert_eq!(app["permissions"]["contents"], "write");
    assert_eq!(
        app["identity"],
        json!({"name": "atlas-crew-box[bot]", "email": "4242+atlas-crew-box[bot]@users.noreply.github.com"})
    );
    assert_eq!(
        app["installations"],
        json!([{"id": 7, "account": "acme", "repository_selection": "selected"}])
    );

    // The installations are read with the app's JWT, issued for the client id.
    let call = rig
        .fake
        .seen()
        .into_iter()
        .find(|r| r.path.starts_with("/app/installations"))
        .unwrap();
    let claims = jwt_claims(call.bearer.as_deref().unwrap());
    assert_eq!(claims["iss"], "Iv23liTESTCLIENT");

    // Audited, with who and what, never a secret.
    let audit = rig.audit_text();
    assert!(
        audit.contains("github_app_created") && audit.contains("atlas-crew-box"),
        "{audit}"
    );
    for secret in [key_marker(), SECRET, CLIENT_SECRET] {
        assert!(!audit.contains(secret));
    }
}

// The code is single use: a second conversion on a server that has an app must not spend one.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_conversion_is_refused_before_the_code_is_spent() {
    let rig = rig().await;
    install(&rig);
    let e = rig.svc.convert("goodcode").await.unwrap_err();
    assert_eq!(e.code, service::EXISTS);
    assert_eq!(rig.fake.count("POST", "/app-manifests/"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_used_or_expired_code_says_so_and_stores_nothing() {
    let rig = rig().await;
    let e = rig.svc.convert("oldcode").await.unwrap_err();
    assert_eq!(e.code, service::CODE_EXPIRED);
    assert!(!rig.dir().join("github-app.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversion_github_refuses_carries_its_status() {
    let fake = fake::serve(|_| (503, r#"{"message":"down"}"#.into())).await;
    let tmp = tempfile::tempdir().unwrap();
    let svc = AppService::with(
        tmp.path(),
        Arc::new(Audit::new(tmp.path())),
        Http::new(&fake.base),
        Arc::new(super::tokens::SystemClock),
    );
    let e = svc.convert("abc").await.unwrap_err();
    assert_eq!(e.code, service::CONVERSION_FAILED);
    assert_eq!(e.args["status"], 503);
    assert!(!tmp.path().join("github-app.json").exists());
}

// The code goes into a URL path: anything that is not a code never becomes a request.
#[tokio::test(flavor = "multi_thread")]
async fn a_code_that_is_not_a_code_never_reaches_github() {
    let rig = rig().await;
    for code in ["", "a/b", "a b", "../x", "abc?x=1", &"a".repeat(200)] {
        assert!(rig.svc.convert(code).await.is_err(), "{code:?}");
    }
    assert!(rig.fake.seen().is_empty());
}

// A key GitHub sent that cannot sign is a failed conversion, found now.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_cannot_sign_fails_the_conversion() {
    let mut bad = conversion();
    bad["pem"] = json!("-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----");
    let fake = fake::serve(move |_| (201, bad.to_string())).await;
    let tmp = tempfile::tempdir().unwrap();
    let svc = AppService::with(
        tmp.path(),
        Arc::new(Audit::new(tmp.path())),
        Http::new(&fake.base),
        Arc::new(super::tokens::SystemClock),
    );
    let e = svc.convert("abc").await.unwrap_err();
    assert_eq!(e.code, service::CONVERSION_FAILED);
    assert!(!tmp.path().join("github-app.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn status_without_an_app_is_null() {
    let rig = rig().await;
    assert_eq!(rig.svc.status().await.unwrap(), json!({"app": null}));
}

// GitHub down: the app is still shown, with the reason the list is empty.
#[tokio::test(flavor = "multi_thread")]
async fn status_still_shows_the_app_when_github_cannot_be_asked() {
    let rig = rig().await;
    install(&rig);
    let svc = AppService::with(
        rig.dir(),
        rig.audit.clone(),
        Http::new("http://127.0.0.1:1"),
        rig.clock.clone(),
    );
    let s = svc.status().await.unwrap();
    assert_eq!(s["app"]["slug"], "atlas-crew-box");
    assert_eq!(s["app"]["installations"], json!([]));
    assert!(
        s["app"]["installations_error"]
            .as_str()
            .unwrap()
            .contains("cannot reach GitHub")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_deletes_the_key_forgets_tokens_and_is_audited() {
    let rig = rig().await;
    install(&rig);
    let t = rig.svc.token_for("acme/web").await.unwrap();
    assert!(t.fresh);
    rig.svc.remove().await.unwrap();
    assert!(!rig.dir().join("github-app.json").exists());
    assert!(rig.audit_text().contains("github_app_removed"));
    assert_eq!(rig.svc.status().await.unwrap(), json!({"app": null}));
    // Nothing minted before is handed out again.
    assert_eq!(
        rig.svc.token_for("acme/web").await.unwrap_err(),
        TokenError::NoApp
    );
    assert_eq!(rig.svc.remove().await.unwrap_err().code, service::NONE);
}

// ── #644: tokens ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn a_token_is_scoped_to_the_repo_and_the_minimal_permissions() {
    let rig = rig().await;
    install(&rig);
    let t = rig.svc.token_for("acme/web").await.unwrap();
    assert_eq!((t.value.as_str(), t.expires_at), ("ghs_token_1", T0 + 3600));
    let mint = rig
        .fake
        .seen()
        .into_iter()
        .find(|r| r.path == "/app/installations/7/access_tokens")
        .unwrap();
    assert_eq!(mint.body["repositories"], json!(["web"]));
    assert_eq!(
        mint.body["permissions"],
        json!({"contents": "write", "pull_requests": "write", "issues": "write",
               "checks": "read", "actions": "read", "metadata": "read"})
    );
    // The app proves who it is with a JWT, not with a token of its own.
    let claims = jwt_claims(mint.bearer.as_deref().unwrap());
    assert_eq!(claims["iss"], "Iv23liTESTCLIENT");
    assert_eq!(claims["iat"], T0 - 60);
    assert!(claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap() <= 600);
}

// GitHub refuses a token that asks for more than its app has.
#[tokio::test(flavor = "multi_thread")]
async fn a_token_never_asks_for_a_permission_the_app_lacks() {
    let rig = rig().await;
    let v = conversion();
    let mut perms = v["permissions"].clone();
    perms.as_object_mut().unwrap().remove("actions");
    perms["workflows"] = json!("write");
    AppStore::new(rig.dir())
        .save(&AppRecord {
            id: 1,
            client_id: "Iv23liTESTCLIENT".into(),
            slug: "atlas-crew-box".into(),
            name: String::new(),
            owner: "acme".into(),
            pem: KEY.into(),
            webhook_secret: SECRET.into(),
            permissions: perms,
            created_at: String::new(),
            bot_id: None,
        })
        .unwrap();
    rig.svc.token_for("acme/web").await.unwrap();
    let mint = rig
        .fake
        .seen()
        .into_iter()
        .find(|r| r.path.ends_with("/access_tokens"))
        .unwrap();
    let asked = &mint.body["permissions"];
    assert!(asked.get("actions").is_none(), "{asked}");
    assert_eq!(asked["workflows"], "write");
}

// The token is replaced five minutes before it expires, never later, and not earlier.
#[tokio::test(flavor = "multi_thread")]
async fn a_token_is_cached_and_refreshed_five_minutes_before_it_expires() {
    let rig = rig().await;
    install(&rig);
    let first = rig.svc.token_for("acme/web").await.unwrap();
    assert!(first.fresh);

    rig.clock.0.store(T0 + 10, Ordering::SeqCst);
    let again = rig.svc.token_for("acme/web").await.unwrap();
    assert!(!again.fresh);
    assert_eq!(again.value, first.value);
    assert_eq!(rig.mints.load(Ordering::SeqCst), 1);

    // One second before the margin (expiry T0+3600, margin 300): still the same token.
    rig.clock.0.store(T0 + 3600 - 301, Ordering::SeqCst);
    assert_eq!(
        rig.svc.token_for("acme/web").await.unwrap().value,
        first.value
    );
    assert_eq!(rig.mints.load(Ordering::SeqCst), 1);

    // At the margin: a new one. The test server's second token expires an hour from T0 too, so only its name moves.
    rig.clock.0.store(T0 + 3600 - 300, Ordering::SeqCst);
    let renewed = rig.svc.token_for("acme/web").await.unwrap();
    assert!(renewed.fresh);
    assert_eq!(renewed.value, "ghs_token_2");
    assert_eq!(rig.mints.load(Ordering::SeqCst), 2);
}

// Only the expiry is audited. A token in the audit would sit on disk for as long as the log does.
#[tokio::test(flavor = "multi_thread")]
async fn minting_is_audited_with_the_expiry_and_never_the_token() {
    let rig = rig().await;
    install(&rig);
    rig.svc.token_for("acme/web").await.unwrap();
    rig.svc.token_for("acme/web").await.unwrap();
    let audit = rig.audit_text();
    assert_eq!(
        audit.matches("github_token_minted").count(),
        1,
        "a cache hit is not a mint: {audit}"
    );
    assert!(
        audit.contains("acme/web") && audit.contains(&(T0 + 3600).to_string()),
        "{audit}"
    );
    assert!(!audit.contains("ghs_"), "{audit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repo_no_installation_covers_is_not_covered_and_is_remembered_for_a_minute() {
    let rig = rig().await;
    install(&rig);
    for _ in 0..3 {
        assert_eq!(
            rig.svc.token_for("other/repo").await.unwrap_err(),
            TokenError::NotCovered("other/repo".into())
        );
    }
    assert_eq!(rig.fake.count("GET", "/repos/other/repo/installation"), 1);
    // The owner adds the repo: seen after the short memory, not at once and not never.
    rig.clock.0.store(T0 + 61, Ordering::SeqCst);
    let _ = rig.svc.token_for("other/repo").await;
    assert_eq!(rig.fake.count("GET", "/repos/other/repo/installation"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_app_there_is_no_token_and_a_repo_that_is_not_one_never_becomes_a_url() {
    let rig = rig().await;
    assert_eq!(
        rig.svc.token_for("acme/web").await.unwrap_err(),
        TokenError::NoApp
    );
    install(&rig);
    for repo in [
        "",
        "acme",
        "acme/..",
        "../x/y",
        "a/b/c",
        "a b/c",
        "acme/web?x=1",
    ] {
        assert!(
            matches!(rig.svc.token_for(repo).await, Err(TokenError::Failed(_))),
            "{repo:?}"
        );
    }
    assert_eq!(rig.fake.seen().len(), 0);
}

// ── #644: the GithubApi that chooses ─────────────────────────────────────────

fn routed(rig: &Rig, cli: Arc<FakeGithub>) -> Routed {
    Routed::with(
        rig.svc.clone(),
        rig.audit.clone(),
        Http::new(&rig.fake.base),
        cli,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_covered_repo_goes_through_the_app_and_comments_come_from_the_bot() {
    let rig = rig().await;
    install(&rig);
    let cli = Arc::new(FakeGithub::new("the-gh-user"));
    let api = routed(&rig, cli.clone());

    assert_eq!(api.login().await.unwrap(), "atlas-crew-box[bot]");
    assert_eq!(api.issue("acme/web", 5).await.unwrap()["title"], "hi");
    assert_eq!(
        api.create_comment("acme/web", 5, "hello").await.unwrap(),
        321
    );

    let post = rig
        .fake
        .seen()
        .into_iter()
        .find(|r| r.path == "/repos/acme/web/issues/5/comments")
        .unwrap();
    assert_eq!(
        post.bearer.as_deref(),
        Some("ghs_token_1"),
        "an installation token, not the JWT"
    );
    assert_eq!(post.body, json!({"body": "hello"}));
    // `gh` was never asked.
    assert!(cli.writes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repo_outside_every_installation_falls_back_to_gh_and_the_audit_says_so_once() {
    let rig = rig().await;
    install(&rig);
    let cli = Arc::new(FakeGithub::new("the-gh-user"));
    cli.issues
        .lock()
        .insert(9, json!({"number": 9, "title": "via gh"}));
    let api = routed(&rig, cli.clone());

    assert_eq!(api.issue("other/repo", 9).await.unwrap()["title"], "via gh");
    assert_eq!(
        api.create_comment("other/repo", 9, "hi").await.unwrap(),
        9001
    );
    assert_eq!(cli.writes().len(), 1);
    // No token was minted for it, no call went to GitHub with the app's identity.
    assert_eq!(rig.mints.load(Ordering::SeqCst), 0);
    let audit = rig.audit_text();
    assert_eq!(audit.matches("github_app_fallback").count(), 1, "{audit}");
    assert!(audit.contains("other/repo"));
}

// No `gh` login either: the reader gets gh's own clear failure, not an empty answer.
#[tokio::test(flavor = "multi_thread")]
async fn outside_every_installation_and_without_gh_the_call_fails_with_the_reason() {
    let rig = rig().await;
    install(&rig);
    let api = routed(&rig, Arc::new(FakeGithub::new("")));
    let e = api.issue("other/repo", 404).await.unwrap_err();
    assert!(e.contains("404"), "{e}");
}

// A server without an app behaves as it did before the app existed.
#[tokio::test(flavor = "multi_thread")]
async fn without_an_app_everything_is_gh() {
    let rig = rig().await;
    let cli = Arc::new(FakeGithub::new("the-gh-user"));
    cli.issues
        .lock()
        .insert(5, json!({"number": 5, "title": "gh"}));
    let api = routed(&rig, cli);
    assert_eq!(api.login().await.unwrap(), "the-gh-user");
    assert_eq!(api.issue("acme/web", 5).await.unwrap()["title"], "gh");
    assert!(rig.fake.seen().is_empty());
}

// GitHub could not be asked: the identity is not silently switched to the gh user's login.
#[tokio::test(flavor = "multi_thread")]
async fn when_github_cannot_mint_a_token_the_call_fails_instead_of_using_gh() {
    let fake = fake::serve(|r: &Req| {
        if r.path.ends_with("/installation") {
            (200, r#"{"id":7}"#.into())
        } else {
            (500, r#"{"message":"boom"}"#.into())
        }
    })
    .await;
    let rig = rig().await;
    install(&rig);
    let svc = Arc::new(AppService::with(
        rig.dir(),
        rig.audit.clone(),
        Http::new(&fake.base),
        rig.clock.clone(),
    ));
    let cli = Arc::new(FakeGithub::new("the-gh-user"));
    cli.issues
        .lock()
        .insert(5, json!({"number": 5, "title": "gh"}));
    let api = Routed::with(svc, rig.audit.clone(), Http::new(&fake.base), cli);
    let e = api.issue("acme/web", 5).await.unwrap_err();
    assert!(e.contains("500"), "{e}");
}

// ── #644: the credential helper ──────────────────────────────────────────────

fn env_of<'a>(pairs: &'a [(&'a str, String)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
}

fn helper_env(port: u16) -> Vec<(&'static str, String)> {
    vec![
        ("ATLAS_HOOK_PORT", port.to_string()),
        ("ATLAS_HOOK_TOKEN", "crewmate-own-token".into()),
        ("ATLAS_PANE_KEY", "crew:cm1:t1".into()),
    ]
}

fn run_helper(input: &str, env: &[(&str, String)]) -> (String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    super::helper::get(input.as_bytes(), &mut out, &mut err, env_of(env)).unwrap();
    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_helper_prints_x_access_token_credentials_for_the_repo_git_names() {
    let hook = fake::serve(|_| {
        (
            200,
            json!({"username": "x-access-token", "password": "ghs_for_this_push", "expires_at": 1})
                .to_string(),
        )
    })
    .await;
    let env = helper_env(hook.port);
    let (out, err) = tokio::task::spawn_blocking(move || {
        run_helper(
            "protocol=https\nhost=github.com\npath=acme/web.git\n\n",
            &env,
        )
    })
    .await
    .unwrap();
    assert_eq!(out, "username=x-access-token\npassword=ghs_for_this_push\n");
    assert_eq!(err, "");
    let req = &hook.seen()[0];
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("POST", "/crew/cm1/git_credential")
    );
    assert_eq!(req.hook_token.as_deref(), Some("crewmate-own-token"));
    assert_eq!(
        req.body,
        json!({"pane_key": "crew:cm1:t1", "host": "github.com", "path": "acme/web.git"})
    );
}

// The token of a repo is never offered to another host a URL names, and nothing is asked of hostd for it.
#[tokio::test(flavor = "multi_thread")]
async fn the_helper_answers_github_com_over_https_and_nothing_else() {
    let hook = fake::serve(|_| (200, json!({"username": "u", "password": "p"}).to_string())).await;
    let env = helper_env(hook.port);
    let hook_port = hook.port;
    let outs = tokio::task::spawn_blocking(move || {
        [
            "protocol=https\nhost=evil.example\npath=acme/web.git\n\n",
            "protocol=http\nhost=github.com\npath=acme/web.git\n\n",
            "protocol=https\nhost=github.com.evil.example\n\n",
            "",
        ]
        .map(|i| run_helper(i, &helper_env(hook_port)).0)
        .into_iter()
        .chain([run_helper("protocol=https\nhost=GitHub.com\npath=a/b\n\n", &env).0])
        .collect::<Vec<_>>()
    })
    .await
    .unwrap();
    assert_eq!(&outs[..4], ["", "", "", ""]);
    // Host names compare without case: github.com is github.com.
    assert_eq!(outs[4], "username=u\npassword=p\n");
    assert_eq!(hook.seen().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_helper_prints_nothing_when_hostd_refuses_or_is_not_there() {
    let hook = fake::serve(|_| {
        (
            404,
            json!({"error": "the GitHub App is not installed on acme/web"}).to_string(),
        )
    })
    .await;
    let env = helper_env(hook.port);
    let (out, err) = tokio::task::spawn_blocking(move || {
        run_helper(
            "protocol=https\nhost=github.com\npath=acme/web.git\n\n",
            &env,
        )
    })
    .await
    .unwrap();
    assert_eq!(out, "");
    assert!(err.contains("not installed on acme/web"), "{err}");

    // No endpoint in the environment (git run by hand): silent for git, a line for a human.
    let (out, err) = run_helper("protocol=https\nhost=github.com\n\n", &[]);
    assert_eq!(out, "");
    assert!(err.contains("not running inside a crewmate task"), "{err}");

    // hostd is not listening.
    let (out, err) = run_helper("protocol=https\nhost=github.com\n\n", &helper_env(1));
    assert_eq!(out, "");
    assert!(err.contains("hostd is not reachable"), "{err}");
}

#[test]
fn the_helper_reads_its_endpoint_from_the_file_of_a_terminal_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("hook-endpoint.env");
    std::fs::write(&file, "ATLAS_HOOK_PORT=4242\nATLAS_HOOK_TOKEN=tok\n").unwrap();
    let env = [
        ("ATLAS_HOOK_ENDPOINT", file.to_string_lossy().into_owned()),
        ("ATLAS_PANE_KEY", "crew:cm9:t3".to_string()),
    ];
    let ep = super::helper::Endpoint::from_env(env_of(&env)).unwrap();
    assert_eq!(
        (ep.port, ep.token.as_str(), ep.crewmate.as_str()),
        (4242, "tok", "cm9")
    );
}

#[test]
fn a_repo_is_read_from_the_path_git_sends() {
    use super::helper::repo_of_path as r;
    assert_eq!(r("acme/web.git").as_deref(), Some("acme/web"));
    assert_eq!(r("acme/web").as_deref(), Some("acme/web"));
    assert_eq!(r("/acme/web.git/info/refs").as_deref(), Some("acme/web"));
    assert_eq!(r("acme"), None);
    assert_eq!(r(""), None);
}
