//! `POST /github/webhook` (#645) through a real hostd, its hook server and a fake GitHub on loopback: GitHub's
//! deliveries (the recorded payloads of `testdata/github_webhook/`) make the task the poll would; an unsigned or wrongly
//! signed delivery is refused with 401 before anything is read, and audited; a replayed delivery id does nothing; the
//! poll stands down for a repo the webhook covers and takes over again when the hook fails; and the webhook secret is
//! in `github-app.json` and nowhere else.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::fixture::{Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin};
use super::github_fake::FakeGithub;
use super::github_hook_events::payload;
use crate::HostClient;
use crate::crew::github::TEST_GITHUB_API;
use crate::crew::github::app::fake::{self, WEBHOOK_SECRET};
use crate::crew::triggers::TEST_GITHUB_POLL;

const URL: &str = "https://crew.example.com";

/// What a test holds for its whole run: the process-wide seams (the agent binary, the GitHub API, the app's HTTP base)
/// are reset when it is dropped.
struct Rig {
    _bin: tokio::sync::MutexGuard<'static, ()>,
    _tmp: tempfile::TempDir,
    dir: std::path::PathBuf,
    client: HostClient,
    port: u16,
    hook_token: String,
    /// The GitHub the crew's REST reads and writes land on.
    api: Arc<FakeGithub>,
    /// The GitHub of the App (installations, hook config, deliveries).
    gh: fake::Fake,
}

impl Drop for Rig {
    fn drop(&mut self) {
        *TEST_GITHUB_API.lock() = None;
        *TEST_GITHUB_POLL.lock() = None;
        *crate::crew::github::app::TEST_API_BASE.lock() = None;
        crate::crew::runner::set_test_agent_bin(None);
    }
}

fn seeded_api() -> Arc<FakeGithub> {
    Arc::new(FakeGithub::new("nami-bot"))
}

/// A hostd with the App installed on `acme/web` (the fake's only installation), the public URL set (or not) and a
/// crewmate "Nami" with a GitHub trigger per repo of `repos` for `events`.
async fn rig(public_url: Option<&str>, repos: &[&str], extra: Value) -> (Rig, String) {
    rig_polling(public_url, repos, extra, None).await
}

/// [`rig`] with the poll running every `poll` (set before hostd starts: its loop reads it once per round).
async fn rig_polling(
    public_url: Option<&str>,
    repos: &[&str],
    extra: Value,
    poll: Option<Duration>,
) -> (Rig, String) {
    let bin = lock_agent_bin().await;
    *TEST_GITHUB_POLL.lock() = poll;
    let mints = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let in_an_hour = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(1))
        .unwrap();
    let gh = fake::serve(fake::github(mints, in_an_hour.to_string())).await;
    *crate::crew::github::app::TEST_API_BASE.lock() = Some(gh.base.clone());
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    std::fs::create_dir_all(&dir).unwrap();
    fake::install_app(&dir);
    let api = seeded_api();
    *TEST_GITHUB_API.lock() = Some(api.clone());
    // The fake agent finishes at once: these tests are about what starts a task.
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("ok").script(),
    ));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    if let Some(u) = public_url {
        // Alone, as Atlas Desktop's webhook field sends it (#659: absent settings stay as they are).
        client
            .call("crew_settings_set", json!({"public_url": u}))
            .await
            .unwrap();
    }
    let triggers: Vec<Value> = repos
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let mut kind = json!({"type": "github", "repo": repo, "handle": "nami", "assignee": "nami-bot",
                "events": ["mention", "issue_assigned", "pr_review_comment"]});
            for (k, v) in extra.as_object().into_iter().flatten() {
                kind[k] = v.clone();
            }
            json!({"id": format!("gh{i}"), "kind": kind, "enabled": true, "prompt": "Handle this: {{event}}"})
        })
        .collect();
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": triggers}),
        )
        .await
        .unwrap();
    let (port, hook_token) = hook_endpoint(&dir).await;
    (
        Rig {
            _bin: bin,
            _tmp: tmp,
            dir,
            client,
            port,
            hook_token,
            api,
            gh,
        },
        id,
    )
}

async fn hook_endpoint(dir: &std::path::Path) -> (u16, String) {
    let deadline = std::time::Instant::now() + atlas_core::scaling::HANG;
    loop {
        let env = std::fs::read_to_string(dir.join("desktop").join("hook-endpoint.env"))
            .unwrap_or_default();
        let find = |k: &str| {
            env.lines()
                .find_map(|l| l.strip_prefix(k))
                .map(|v| v.trim().to_string())
        };
        if let (Some(p), Some(t)) = (find("ATLAS_HOOK_PORT="), find("ATLAS_HOOK_TOKEN=")) {
            return (p.parse().unwrap_or(0), t);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "hook server did not start"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `sha256=` + the hex HMAC-SHA256 of `body` under `secret`: what GitHub sends.
fn sign(secret: &str, body: &[u8]) -> String {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    format!(
        "sha256={}",
        hex::encode(ring::hmac::sign(&key, body).as_ref())
    )
}

/// One request to `/github/webhook`: the status line's code and the body.
struct Post<'a> {
    event: &'a str,
    delivery: &'a str,
    /// `None`: no `X-Hub-Signature-256`.
    signature: Option<String>,
    body: &'a [u8],
}

impl Rig {
    async fn post(&self, p: Post<'_>) -> (u16, String) {
        let mut head = format!(
            "POST /github/webhook HTTP/1.1\r\nHost: crew.example.com\r\nContent-Type: application/json\r\n\
             X-GitHub-Event: {}\r\nX-GitHub-Delivery: {}\r\nContent-Length: {}\r\n",
            p.event,
            p.delivery,
            p.body.len()
        );
        if let Some(s) = &p.signature {
            head.push_str(&format!("X-Hub-Signature-256: {s}\r\n"));
        }
        // The hook token is the loopback server's own credential: it never authorises this route.
        head.push_str(&format!("X-Atlas-Hook-Token: {}\r\n\r\n", self.hook_token));
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", self.port))
            .await
            .unwrap();
        s.write_all(head.as_bytes()).await.unwrap();
        s.write_all(p.body).await.unwrap();
        let mut resp = String::new();
        let _ = s.read_to_string(&mut resp).await;
        let code = resp
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or_else(|| panic!("no status in {resp:?}"));
        (
            code,
            resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string(),
        )
    }

    /// A correctly signed delivery of fixture `name`, with `edit` applied to the payload first.
    async fn deliver(
        &self,
        event: &str,
        delivery: &str,
        name: &str,
        edit: impl FnOnce(&mut Value),
    ) -> (u16, String) {
        let mut p = payload(name);
        edit(&mut p);
        let body = serde_json::to_vec(&p).unwrap();
        self.post(Post {
            event,
            delivery,
            signature: Some(sign(WEBHOOK_SECRET, &body)),
            body: &body,
        })
        .await
    }

    async fn tasks(&self, id: &str) -> Vec<Value> {
        self.client
            .call("crewmate_tasks", json!({"id": id, "limit": 50}))
            .await
            .unwrap()["tasks"]
            .as_array()
            .unwrap()
            .clone()
    }

    async fn crew_audit(&self) -> Vec<Value> {
        self.client
            .call("crew_audit", json!({"limit": 500}))
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .clone()
    }

    async fn audited(&self, kind: &str) -> Vec<Value> {
        self.crew_audit()
            .await
            .into_iter()
            .filter(|e| e["kind"] == kind)
            .collect()
    }
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + atlas_core::scaling::HANG;
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The webhook secret is in the private app file and nowhere else under the server's directory, and not in any of
/// `replies` (what the test was told over the wire). Links are not followed: `shims/gh` points at the atlas binary
/// (this test binary, which holds the constant), and what a link points at is not a file the server wrote.
fn assert_secret_stays_in_its_file(dir: &std::path::Path, replies: &[String]) {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                walk(&e.path(), out);
            } else if kind.is_file() {
                out.push(e.path());
            }
        }
    }
    let mut files = vec![];
    walk(dir, &mut files);
    for f in files {
        if f.file_name().is_some_and(|n| n == "github-app.json") {
            continue;
        }
        // A socket or a binary is not text: only text files can hold the secret in a way anyone reads.
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        assert!(
            !String::from_utf8_lossy(&bytes).contains(WEBHOOK_SECRET),
            "the webhook secret is in {}",
            f.display()
        );
    }
    for r in replies {
        assert!(
            !r.contains(WEBHOOK_SECRET),
            "a reply carries the secret: {r}"
        );
    }
}

// The leak check reads what the server wrote, not what a link points at. `<crew dir>/shims/gh` is a link to the atlas
// binary, which under test is this test binary, and it holds `WEBHOOK_SECRET` as a constant: following the link failed
// `a_signed_delivery_starts_the_task_and_a_replayed_id_does_nothing` on main's Linux CI whenever the crewmate's task
// made the shim before the walk (a race). A file the server writes with the secret in it must still fail the check.
#[test]
fn the_leak_check_reads_the_servers_files_not_where_a_link_points() {
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("atlas");
    std::fs::write(&target, format!("binary {WEBHOOK_SECRET} bytes")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("shims")).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("shims/gh")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
    assert_secret_stays_in_its_file(dir.path(), &[]);

    std::fs::write(
        dir.path().join("leak.log"),
        format!("token={WEBHOOK_SECRET}"),
    )
    .unwrap();
    let caught = std::panic::catch_unwind(|| assert_secret_stays_in_its_file(dir.path(), &[]));
    assert!(
        caught.is_err(),
        "a file the server wrote with the secret must fail the check"
    );
}

fn task_for(tasks: &[Value], comment_id: u64) -> Option<&Value> {
    tasks
        .iter()
        .find(|t| t["origin"]["comment_id"] == comment_id)
}

// The route end to end: a correctly signed delivery is answered at once (202), becomes the task the poll would make
// (origin and quoted prompt), and the same delivery id sent again is answered and does nothing, with an audit line.
#[tokio::test(flavor = "multi_thread")]
async fn a_signed_delivery_starts_the_task_and_a_replayed_id_does_nothing() {
    let (rig, id) = rig(Some(URL), &["o/r"], json!({})).await;
    let mut replies = vec![];

    let (code, body) = rig
        .deliver(
            "issue_comment",
            "guid-0001",
            "issue_comment_created",
            |_| {},
        )
        .await;
    assert_eq!(code, 202, "{body}");
    replies.push(body);

    let task = until("the delivery's task", async || {
        task_for(&rig.tasks(&id).await, 100).cloned()
    })
    .await;
    assert_eq!(task["origin"]["repo"], "o/r");
    assert_eq!(task["origin"]["kind"], "mention");
    assert_eq!(task["origin"]["number"], 7);
    assert_eq!(task["origin"]["requester"]["login"], "dev");
    assert_eq!(task["no_auto"], true, "text from GitHub runs as ask");
    let prompt = task["prompt"].as_str().unwrap();
    assert!(prompt.contains("@nami please fix the crash"), "{prompt}");
    assert!(prompt.contains(crate::crew::triggers::UNTRUSTED_PREAMBLE_MARK));

    // The replay: refused as a duplicate (GitHub's own redelivery of a delivery we took must not show as a failure),
    // nothing is done, and the audit names it.
    let (code, body) = rig
        .deliver(
            "issue_comment",
            "guid-0001",
            "issue_comment_created",
            |_| {},
        )
        .await;
    assert_eq!((code, body.as_str()), (200, r#"{"duplicate":true}"#));
    replies.push(body);
    let dup = until("the duplicate in the audit", async || {
        Some(rig.audited("github_webhook_duplicate").await).filter(|a| !a.is_empty())
    })
    .await;
    assert_eq!(dup[0]["delivery"], "guid-0001");
    assert_eq!(dup[0]["event"], "issue_comment");
    // A different id for the same payload is a different delivery: GitHub made it, so it is handled.
    let (code, _) = rig
        .deliver("issue_comment", "guid-0002", "issue_comment_created", |p| {
            p["comment"]["id"] = json!(102);
        })
        .await;
    assert_eq!(code, 202);
    until("the second delivery's task", async || {
        task_for(&rig.tasks(&id).await, 102).cloned()
    })
    .await;
    assert_eq!(
        rig.tasks(&id)
            .await
            .iter()
            .filter(|t| t["origin"]["comment_id"] == 100)
            .count(),
        1,
        "the replay made no second task"
    );

    replies.push(
        rig.client
            .call("crew_github_app_status", json!({}))
            .await
            .unwrap()
            .to_string(),
    );
    replies.push(
        rig.client
            .call("crew_settings_get", json!({}))
            .await
            .unwrap()
            .to_string(),
    );
    assert_secret_stays_in_its_file(&rig.dir, &replies);
}

// The refusals: no signature, a wrong one, a signature of another body, a malformed header. Each is 401 before
// anything is read, makes no task, is audited, and does not burn the delivery id (an intruder cannot make GitHub's
// real delivery look like a replay). The loopback hook token, which opens the other routes, opens nothing here.
#[tokio::test(flavor = "multi_thread")]
async fn an_unsigned_or_wrongly_signed_delivery_is_refused_audited_and_does_nothing() {
    let (rig, id) = rig(Some(URL), &["o/r"], json!({})).await;
    let body = serde_json::to_vec(&payload("issue_comment_created")).unwrap();
    let other = serde_json::to_vec(&payload("issue_comment_created_stranger")).unwrap();
    let mut replies = vec![];

    for (what, signature) in [
        ("missing", None),
        ("wrong secret", Some(sign("not-the-secret", &body))),
        (
            "signature of another body",
            Some(sign(WEBHOOK_SECRET, &other)),
        ),
        (
            "sha1 instead of sha256",
            Some(sign(WEBHOOK_SECRET, &body).replace("sha256=", "sha1=")),
        ),
        ("not hex", Some("sha256=zzzz".to_string())),
    ] {
        let (code, reply) = rig
            .post(Post {
                event: "issue_comment",
                delivery: "guid-9999",
                signature,
                body: &body,
            })
            .await;
        assert_eq!(code, 401, "{what}: {reply}");
        replies.push(reply);
    }
    // Nothing was read, claimed or started.
    assert!(rig.tasks(&id).await.is_empty());

    // Audited, with the reason; the line is rate limited per reason and carries how many it stands for (the URL is
    // public: whoever finds it must not be able to fill the log).
    let refused = rig.audited("github_webhook_refused").await;
    let by = |reason: &str| -> Vec<&Value> {
        refused.iter().filter(|e| e["reason"] == reason).collect()
    };
    assert_eq!(by("unsigned").len(), 1, "{refused:?}");
    assert_eq!(by("unsigned")[0]["count"], 1);
    assert_eq!(by("bad_signature").len(), 1, "{refused:?}");
    assert_eq!(by("bad_signature")[0]["count"], 1);
    // The audit says what was refused, never the body or the signature that was sent.
    for e in &refused {
        let text = e.to_string();
        assert!(
            !text.contains("sha256=") && !text.contains("fix the crash"),
            "{text}"
        );
    }

    // The id was not burnt: GitHub's real delivery under the same id is taken.
    let (code, reply) = rig
        .deliver(
            "issue_comment",
            "guid-9999",
            "issue_comment_created",
            |_| {},
        )
        .await;
    assert_eq!(code, 202, "{reply}");
    until("the real delivery's task", async || {
        task_for(&rig.tasks(&id).await, 100).cloned()
    })
    .await;
    replies.push(reply);
    assert_secret_stays_in_its_file(&rig.dir, &replies);
}

// Malformed but correctly signed: GitHub signed it, so it is the request that is wrong (400), and no id is taken.
#[tokio::test(flavor = "multi_thread")]
async fn a_signed_delivery_with_missing_headers_or_a_body_that_is_not_json_is_a_400() {
    let (rig, _) = rig(Some(URL), &["o/r"], json!({})).await;
    let body = b"{\"action\": ";
    let sig = sign(WEBHOOK_SECRET, body);
    for (event, delivery) in [
        ("issue_comment", "guid-1"),
        ("", "guid-2"),
        ("issue_comment", ""),
    ] {
        let (code, _) = rig
            .post(Post {
                event,
                delivery,
                signature: Some(sig.clone()),
                body,
            })
            .await;
        assert_eq!(code, 400, "{event:?} {delivery:?}");
    }
    let (code, _) = rig
        .post(Post {
            event: "issue_comment",
            delivery: "bad id with spaces",
            signature: Some(sign(WEBHOOK_SECRET, b"{}")),
            body: b"{}",
        })
        .await;
    assert_eq!(code, 400);
}

// Without a public URL the route does not exist: the setting is what turns the webhook on.
#[tokio::test(flavor = "multi_thread")]
async fn without_a_public_url_the_route_is_not_there() {
    let (rig, id) = rig(None, &["o/r"], json!({})).await;
    let (code, _) = rig
        .deliver("issue_comment", "guid-1", "issue_comment_created", |_| {})
        .await;
    assert_eq!(code, 404);
    assert!(rig.tasks(&id).await.is_empty());
}

// Paused: not taken (so GitHub shows it failed and the reconciliation redelivers it once the crew is back), and the
// same id works after the resume.
#[tokio::test(flavor = "multi_thread")]
async fn a_paused_crew_does_not_take_the_delivery() {
    let (rig, id) = rig(Some(URL), &["o/r"], json!({})).await;
    rig.client.call("crew_pause", json!({})).await.unwrap();
    let (code, _) = rig
        .deliver("issue_comment", "guid-p", "issue_comment_created", |_| {})
        .await;
    assert_eq!(code, 503);
    rig.client.call("crew_resume", json!({})).await.unwrap();
    let (code, _) = rig
        .deliver("issue_comment", "guid-p", "issue_comment_created", |_| {})
        .await;
    assert_eq!(code, 202);
    until("the task after the resume", async || {
        task_for(&rig.tasks(&id).await, 100).cloned()
    })
    .await;
}

// A stranger's comment is accepted by the route (it is GitHub's delivery) and ignored by the trigger, as the poll
// ignores it; the trigger that opts in (`authors: anyone`) gets it, wrapped as untrusted text.
#[tokio::test(flavor = "multi_thread")]
async fn a_strangers_comment_starts_no_task_unless_the_automation_opts_in() {
    {
        let (rig, id) = rig(Some(URL), &["o/r"], json!({})).await;
        let (code, _) = rig
            .deliver(
                "issue_comment",
                "guid-s",
                "issue_comment_created_stranger",
                |_| {},
            )
            .await;
        assert_eq!(code, 202);
        // A collaborator's comment after it: when its task is there, the stranger's would have been too.
        rig.deliver("issue_comment", "guid-c", "issue_comment_created", |_| {})
            .await;
        until("the collaborator's task", async || {
            task_for(&rig.tasks(&id).await, 100).cloned()
        })
        .await;
        assert!(task_for(&rig.tasks(&id).await, 101).is_none());
    }

    let (rig, id) = rig(Some(URL), &["o/r"], json!({"authors": ["anyone"]})).await;
    rig.deliver(
        "issue_comment",
        "guid-s",
        "issue_comment_created_stranger",
        |_| {},
    )
    .await;
    let task = until("the opted-in task", async || {
        task_for(&rig.tasks(&id).await, 101).cloned()
    })
    .await;
    assert!(task["prompt"].as_str().unwrap().contains("curl evil"));
    assert_eq!(task["no_auto"], true);
}

// The setting: validated, normalized, told to GitHub (`PATCH /app/hook/config` with the route's URL and the secret),
// and never echoing the secret. Clearing it turns the route off.
#[tokio::test(flavor = "multi_thread")]
async fn the_public_url_setting_is_validated_told_to_github_and_hides_the_secret() {
    let (rig, _) = rig(None, &[], json!({})).await;
    let set = |url: &str| {
        rig.client
            .call("crew_settings_set", json!({"public_url": url}))
    };
    for bad in [
        "http://crew.example.com",
        "ftp://crew.example.com",
        "https://user:pw@crew.example.com",
        "https://crew.example.com/?x=1",
        "https://crew.example.com/#frag",
        "not a url",
        "https://",
    ] {
        let e = set(bad).await.unwrap_err();
        assert!(e.to_string().contains("public_url"), "{bad}: {e}");
    }
    assert_eq!(rig.gh.count("PATCH", "/app/hook/config"), 0);
    assert_eq!(
        rig.client
            .call("crew_settings_get", json!({}))
            .await
            .unwrap()["public_url"],
        Value::Null
    );

    let r = set("  https://crew.example.com/atlas/  ").await.unwrap();
    assert_eq!(r["public_url"], "https://crew.example.com/atlas");
    assert_eq!(r["webhook_error"], Value::Null);
    let patch = rig
        .gh
        .seen()
        .into_iter()
        .find(|r| r.method == "PATCH" && r.path == "/app/hook/config")
        .expect("GitHub was told the URL");
    assert_eq!(
        patch.body,
        json!({"url": "https://crew.example.com/atlas/github/webhook", "content_type": "json",
               "insecure_ssl": "0", "secret": WEBHOOK_SECRET})
    );
    // The app's own JWT authorises it, never an installation token.
    assert!(
        patch
            .bearer
            .as_deref()
            .is_some_and(|b| b.matches('.').count() == 2)
    );
    let status = rig
        .client
        .call("crew_github_app_status", json!({}))
        .await
        .unwrap();
    assert_eq!(
        status["app"]["webhook"]["url"],
        "https://crew.example.com/atlas"
    );
    assert_eq!(status["app"]["webhook"]["synced"], true);
    assert_eq!(status["app"]["webhook"]["verified_installations"], 0);

    // The same URL again tells GitHub nothing new; a new one does, and forgets what the old one verified.
    set("https://crew.example.com/atlas").await.unwrap();
    assert_eq!(rig.gh.count("PATCH", "/app/hook/config"), 1);
    set(URL).await.unwrap();
    assert_eq!(rig.gh.count("PATCH", "/app/hook/config"), 2);

    // Off again: an empty string. The route is gone.
    let r = set("").await.unwrap();
    assert_eq!(r["public_url"], Value::Null);
    let (code, _) = rig
        .deliver("issue_comment", "guid-x", "issue_comment_created", |_| {})
        .await;
    assert_eq!(code, 404);

    let replies = vec![
        status.to_string(),
        rig.client
            .call("crew_settings_get", json!({}))
            .await
            .unwrap()
            .to_string(),
    ];
    assert_secret_stays_in_its_file(&rig.dir, &replies);
    // The audit records the URL changes (an URL is no secret) and nothing of the secret.
    assert_eq!(rig.audited("github_webhook_url_set").await.len(), 3);
    assert_eq!(rig.audited("github_webhook_configured").await.len(), 2);
}

// Polling hands over: once an installation has delivered a correctly signed event, the poll stops reading the events
// of the repos it covers (`acme/web`) and keeps reading the others (`o/r`, no installation).
#[tokio::test(flavor = "multi_thread")]
async fn polling_stops_for_a_repo_the_webhook_covers_and_continues_for_another() {
    let (rig, id) = rig_polling(
        Some(URL),
        &["acme/web", "o/r"],
        json!({}),
        Some(Duration::from_millis(50)),
    )
    .await;
    let seen = |trigger: &str| {
        rig.dir
            .join("crew")
            .join(&id)
            .join(format!("{trigger}_seen.jsonl"))
    };
    // Before anything is verified both are polled, and both get their baseline.
    until("both baselines", async || {
        (seen("gh0").exists() && seen("gh1").exists()).then_some(())
    })
    .await;
    assert!(rig.api.reads_of("acme/web") >= 1 && rig.api.reads_of("o/r") >= 1);

    // The App's installation event, signed: the installation (7, on acme/web) now delivers.
    let (code, _) = rig
        .deliver("installation", "guid-i", "installation_created", |_| {})
        .await;
    assert_eq!(code, 202);
    let state: Value =
        serde_json::from_slice(&std::fs::read(rig.dir.join("github-webhook.json")).unwrap())
            .unwrap();
    assert!(state["verified"]["7"].is_number(), "{state}");

    // The poll finishes a tick for the uncovered repo after that (so any tick in flight is over), then neither
    // grows for acme/web while o/r keeps being read.
    let after = rig.api.reads_of("o/r");
    until("o/r read again", async || {
        (rig.api.reads_of("o/r") >= after + 2).then_some(())
    })
    .await;
    let web = rig.api.reads_of("acme/web");
    let other = rig.api.reads_of("o/r");
    until("o/r read twice more", async || {
        (rig.api.reads_of("o/r") >= other + 2).then_some(())
    })
    .await;
    assert_eq!(
        rig.api.reads_of("acme/web"),
        web,
        "the poll kept reading a covered repo"
    );
    // The covered trigger is marked: when the poll comes back it must not answer what the webhook did.
    assert!(
        rig.dir
            .join("crew")
            .join(&id)
            .join("gh0_seen.hooked")
            .exists()
    );
    assert!(
        !rig.dir
            .join("crew")
            .join(&id)
            .join("gh1_seen.hooked")
            .exists(),
        "an uncovered repo is not marked"
    );

    // And the webhook makes the covered repo's tasks.
    let (code, _) = rig
        .deliver("issue_comment", "guid-w", "issue_comment_created", |p| {
            p["repository"]["full_name"] = json!("acme/web");
        })
        .await;
    assert_eq!(code, 202);
    let task = until("the covered repo's task", async || {
        task_for(&rig.tasks(&id).await, 100).cloned()
    })
    .await;
    assert_eq!(task["origin"]["repo"], "acme/web");
}

// The poll takes over again when the hook fails, and answers only what happened after the webhook's last good
// delivery: what came before it was the webhook's (answering it again would start the task twice).
#[tokio::test(flavor = "multi_thread")]
async fn the_poll_takes_over_when_the_hook_fails_and_skips_what_the_webhook_handled() {
    let (rig, id) = rig_polling(
        Some(URL),
        &["acme/web"],
        json!({}),
        Some(Duration::from_millis(50)),
    )
    .await;
    let seen = rig.dir.join("crew").join(&id).join("gh0_seen.jsonl");
    until("the baseline", async || seen.exists().then_some(())).await;
    rig.deliver("installation", "guid-i", "installation_created", |_| {})
        .await;
    let hooked = rig.dir.join("crew").join(&id).join("gh0_seen.hooked");
    until("the poll to stand down", async || {
        hooked.exists().then_some(())
    })
    .await;

    // The hook fails (the reconciliation saw nothing succeed for a while): the poll is back, and the webhook's last
    // good delivery was at `last_ok`.
    let last_ok = jiff::Timestamp::now().as_second();
    let iso = |t: i64| jiff::Timestamp::from_second(t).unwrap().to_string();
    let mention = |ev: &str, comment: u64, at: i64| {
        json!({"id": ev, "type": "IssueCommentEvent", "created_at": iso(at),
               "actor": {"login": "dev", "id": 5001},
               "payload": {"action": "created",
                           "issue": {"number": 7, "title": "Crash on start"},
                           "comment": {"id": comment, "body": "@nami please fix it",
                                       "user": {"login": "dev"}}}})
    };
    rig.api.issues.lock().insert(
        7,
        json!({"number": 7, "title": "Crash on start", "body": "boom",
               "author_association": "MEMBER", "user": {"login": "someone"}}),
    );
    for n in [200u64, 201] {
        rig.api.comments.lock().insert(
            n,
            json!({"id": n, "body": "@nami please fix it", "author_association": "COLLABORATOR",
                   "user": {"login": "dev"}}),
        );
    }
    *rig.api.events.lock() = vec![
        mention("e-new", 201, last_ok + 60),
        mention("e-old", 200, last_ok - 60),
    ];
    std::fs::write(
        rig.dir.join("github-webhook.json"),
        serde_json::to_vec(&json!({
            "synced_url": URL, "verified": {"7": last_ok}, "last_ok": last_ok,
            "failing": true, "healthy_since": last_ok - 3600
        }))
        .unwrap(),
    )
    .unwrap();

    let task = until("the poll's task", async || {
        task_for(&rig.tasks(&id).await, 201).cloned()
    })
    .await;
    assert_eq!(task["origin"]["repo"], "acme/web");
    until("the marker to go", async || {
        (!hooked.exists()).then_some(())
    })
    .await;
    assert!(
        task_for(&rig.tasks(&id).await, 200).is_none(),
        "the poll answered what the webhook had handled"
    );
}
