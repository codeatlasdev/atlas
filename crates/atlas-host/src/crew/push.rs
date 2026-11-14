//! Push notification sender via the Atlas push relay.
//!
//! `ATLAS_PUSH_RELAY` env (default `https://push.codeatlas.com.br`); `"off"` disables.
//! Failures are logged to stderr, never retried.
//!
//! The relay only notifies a caller that holds the token's *grant* (issue #205): the phone gets it
//! from the relay's `POST /v1/register` and hands `{token, grant}` to `push_register`; every
//! notify here carries it. The relay answers 403 for a grant it does not know and 410 for a token
//! FCM no longer has; both mean the entry can never work again and is dropped.

use std::path::Path;
use std::sync::LazyLock;

use serde_json::json;

use crate::crew::now_s;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PushToken {
    pub node_id: String,
    pub token: String,
    pub platform: String,
    pub registered_at: u64,
    /// The relay's grant for `token`. Empty for a token saved before relay auth existed (the
    /// `tokens.json` of an older hostd, or an app that has not updated): kept, never sent, until
    /// the phone registers again and replaces the entry.
    #[serde(default)]
    pub grant: String,
}

/// Shared HTTP client (initialised once; `None` if build fails or relay is `"off"`).
pub(crate) static HTTP: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
    let relay = std::env::var("ATLAS_PUSH_RELAY").unwrap_or_default();
    if relay == "off" {
        return None;
    }
    // Install the ring provider so rustls-no-provider works.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().build().ok()
});

/// The relay base URL, or `None` when push is off.
fn relay_base() -> Option<String> {
    let v = std::env::var("ATLAS_PUSH_RELAY").unwrap_or_default();
    match v.as_str() {
        "off" => None,
        "" => Some("https://push.codeatlas.com.br".to_string()),
        _ => Some(v.trim_end_matches('/').to_string()),
    }
}

/// Notify every token of `kind`. Returns the tokens the relay will never accept again (caller
/// drops them): `410 Gone` (FCM: unregistered) and `403 Forbidden` (the relay rejects the grant).
pub async fn notify(tokens: &[PushToken], kind: &str) -> Vec<String> {
    let (Some(client), Some(base)) = (HTTP.as_ref(), relay_base()) else {
        return vec![];
    };
    notify_via(client, &base, tokens, kind).await
}

/// [`notify`] against an explicit relay `base` (tests point it at a fake).
pub(crate) async fn notify_via(
    client: &reqwest::Client,
    base: &str,
    tokens: &[PushToken],
    kind: &str,
) -> Vec<String> {
    let url = format!("{base}/v1/notify");
    let mut expired = vec![];
    for t in tokens {
        if t.grant.is_empty() {
            continue; // registered before relay auth: the relay would refuse it; wait for the phone
        }
        let body = json!({"token": t.token, "kind": kind, "grant": t.grant});
        match client.post(&url).json(&body).send().await {
            Ok(r) if matches!(r.status().as_u16(), 403 | 410) => {
                expired.push(t.token.clone());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("atlas crew: push notify error: {e}");
            }
        }
    }
    expired
}

fn tokens_path(dir: &Path) -> std::path::PathBuf {
    dir.join("push").join("tokens.json")
}

/// Load tokens from `<dir>/push/tokens.json`.
pub fn load_tokens(dir: &Path) -> Vec<PushToken> {
    std::fs::read(tokens_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Save tokens to `<dir>/push/tokens.json` atomically, 0600.
pub fn save_tokens(dir: &Path, tokens: &[PushToken]) {
    let path = tokens_path(dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(data) = serde_json::to_vec(tokens) else {
        return;
    };
    if let Err(e) = crate::paths::write_private(&path, &data) {
        eprintln!("atlas crew: push save_tokens error: {e}");
    }
}

/// Build a new `PushToken` with the current timestamp.
pub fn new_token(
    node_id: impl Into<String>,
    token: impl Into<String>,
    platform: impl Into<String>,
    grant: impl Into<String>,
) -> PushToken {
    PushToken {
        node_id: node_id.into(),
        token: token.into(),
        platform: platform.into(),
        registered_at: now_s(),
        grant: grant.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// A fake relay: answers every request with `status` and records `(request line, body)`.
    async fn fake_relay(
        status: u16,
    ) -> (
        String,
        tokio::sync::mpsc::UnboundedReceiver<(String, String)>,
    ) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else {
                    return;
                };
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let mut n = 0;
                    // Read until the whole body arrived (headers + content-length bytes).
                    loop {
                        let Ok(k) = s.read(&mut buf[n..]).await else {
                            return;
                        };
                        if k == 0 {
                            break;
                        }
                        n += k;
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        if let Some((head, body)) = text.split_once("\r\n\r\n") {
                            let want = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                                })
                                .unwrap_or(0);
                            if body.len() >= want {
                                let line = head.lines().next().unwrap_or("").to_string();
                                let _ = tx.send((line, body.to_string()));
                                break;
                            }
                        }
                    }
                    let reply = format!(
                        "HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    );
                    let _ = s.write_all(reply.as_bytes()).await;
                });
            }
        });
        (base, rx)
    }

    /// The production client (installs the rustls provider once).
    fn client() -> reqwest::Client {
        HTTP.as_ref()
            .expect("ATLAS_PUSH_RELAY=off in the test env")
            .clone()
    }

    fn tok(token: &str, grant: &str) -> PushToken {
        new_token("n1", token, "fcm", grant)
    }

    // #205: the relay refuses a notify without the grant the phone got at /v1/register, so hostd
    // must send the stored grant, and to the path the relay serves (it posted to `/push`, which the
    // relay never had: no push ever left a crew server).
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_posts_token_kind_and_grant_to_v1_notify() {
        let (base, mut rx) = fake_relay(204).await;
        let expired = notify_via(&client(), &base, &[tok("fcm-1", "grant-1")], "task_done").await;
        assert!(expired.is_empty());
        let (line, body) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(line, "POST /v1/notify HTTP/1.1");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            v,
            json!({"token": "fcm-1", "kind": "task_done", "grant": "grant-1"})
        );
    }

    // #205: the relay answers 403 for a grant it does not recognise (wrong, or its key rotated
    // away): that token can never be notified, so hostd drops it, like UNREGISTERED today.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_403_from_the_relay_drops_the_token() {
        let (base, _rx) = fake_relay(403).await;
        let expired = notify_via(&client(), &base, &[tok("fcm-1", "bad")], "needs_you").await;
        assert_eq!(expired, vec!["fcm-1".to_string()]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_410_from_the_relay_still_drops_the_token() {
        let (base, _rx) = fake_relay(410).await;
        let expired = notify_via(&client(), &base, &[tok("fcm-1", "g")], "task_done").await;
        assert_eq!(expired, vec!["fcm-1".to_string()]);
    }

    // Edge: a transient relay failure (502, quota) must keep the token.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_502_or_429_keeps_the_token() {
        for status in [502, 429, 500] {
            let (base, _rx) = fake_relay(status).await;
            let expired = notify_via(&client(), &base, &[tok("fcm-1", "g")], "task_done").await;
            assert!(
                expired.is_empty(),
                "status {status} must not drop the token"
            );
        }
    }

    // Transition: a token registered before grants existed has none. Keep it (the phone
    // re-registers on its next launch and replaces it) but do not call a relay that must refuse it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_token_without_a_grant_is_kept_and_not_sent() {
        let (base, mut rx) = fake_relay(403).await;
        let expired = notify_via(&client(), &base, &[tok("old", "")], "task_done").await;
        assert!(expired.is_empty(), "a pre-grant token must not be dropped");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            rx.try_recv().is_err(),
            "nothing may be sent for a token without a grant"
        );
    }

    // Transition, on disk: a tokens.json written before this change has no `grant`.
    #[test]
    fn tokens_file_from_before_grants_still_loads() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("push")).unwrap();
        std::fs::write(
            tmp.path().join("push").join("tokens.json"),
            r#"[{"node_id":"n","token":"t","platform":"fcm","registered_at":1}]"#,
        )
        .unwrap();
        let loaded = load_tokens(tmp.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].grant, "");
    }

    #[test]
    fn the_grant_is_stored_with_the_token() {
        let tmp = tempfile::tempdir().unwrap();
        save_tokens(tmp.path(), &[tok("t", "g")]);
        assert_eq!(load_tokens(tmp.path())[0].grant, "g");
    }
}
