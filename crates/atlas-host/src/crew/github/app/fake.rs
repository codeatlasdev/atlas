//! Test-only: a GitHub on a loopback port. Tests never reach github.com; every call the app makes lands here and is
//! recorded, so a test can say exactly what was sent (the Authorization header, the body) and how often.

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One request the fake saw.
#[derive(Debug, Clone)]
pub(crate) struct Req {
    pub method: String,
    /// Path and query, as sent.
    pub path: String,
    /// The bearer token, if any.
    pub bearer: Option<String>,
    /// The `X-Atlas-Hook-Token` header (the hook server's), if any.
    pub hook_token: Option<String>,
    pub body: Value,
}

type Handler = dyn Fn(&Req) -> (u16, String) + Send + Sync;

pub(crate) struct Fake {
    pub base: String,
    pub port: u16,
    log: Arc<Mutex<Vec<Req>>>,
    _task: tokio::task::JoinHandle<()>,
}

impl Fake {
    /// Every request so far, oldest first.
    pub(crate) fn seen(&self) -> Vec<Req> {
        self.log.lock().clone()
    }

    /// How many requests `method` `path_prefix...` got.
    pub(crate) fn count(&self, method: &str, path_prefix: &str) -> usize {
        self.log
            .lock()
            .iter()
            .filter(|r| r.method == method && r.path.starts_with(path_prefix))
            .count()
    }
}

/// Start the fake: `handler` answers `(status, JSON body)` for each request.
pub(crate) async fn serve(handler: impl Fn(&Req) -> (u16, String) + Send + Sync + 'static) -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Arc<Mutex<Vec<Req>>> = Arc::default();
    let handler: Arc<Handler> = Arc::new(handler);
    let task = {
        let log = log.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let (log, handler) = (log.clone(), handler.clone());
                tokio::spawn(async move {
                    let Some(req) = read(&mut s).await else {
                        return;
                    };
                    log.lock().push(req.clone());
                    let (status, body) = handler(&req);
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = s.write_all(resp.as_bytes()).await;
                    let _ = s.shutdown().await;
                });
            }
        })
    };
    Fake {
        base: format!("http://127.0.0.1:{port}"),
        port,
        log,
        _task: task,
    }
}

async fn read(s: &mut tokio::net::TcpStream) -> Option<Req> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_end, len) = loop {
        let n = s.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (i + 4, len);
        }
    };
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let header = |name: &str| {
        head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    };
    Some(Req {
        method,
        path,
        bearer: header("authorization").and_then(|v| v.strip_prefix("Bearer ").map(str::to_string)),
        hook_token: header("x-atlas-hook-token"),
        body: serde_json::from_slice(&buf[head_end..(head_end + len).min(buf.len())])
            .unwrap_or_default(),
    })
}

/// The test key (a throwaway: see `testdata/`).
pub(crate) const KEY: &str = include_str!("testdata/test_key_pkcs1.pem");
pub(crate) const WEBHOOK_SECRET: &str = "whsec_test_webhook_secret_0123456789";
pub(crate) const CLIENT_SECRET: &str = "client_secret_must_never_be_stored_0123";

/// What GitHub answers a manifest conversion with.
pub(crate) fn conversion() -> Value {
    serde_json::json!({
        "id": 987654, "slug": "atlas-crew-box", "name": "atlas-crew-box", "client_id": "Iv23liTESTCLIENT",
        "client_secret": CLIENT_SECRET, "webhook_secret": WEBHOOK_SECRET, "pem": KEY,
        "owner": {"login": "acme", "id": 1, "type": "Organization"},
        "permissions": {"contents": "write", "pull_requests": "write", "issues": "write",
                        "checks": "read", "actions": "read", "metadata": "read"},
        "events": ["issues"], "created_at": "2026-11-19T10:00:00Z"
    })
}

/// A GitHub with this app installed on `acme/web` only (installation 7). `mints` counts the installation
/// tokens it made (`ghs_token_<n>`), each expiring at `expires_at`.
pub(crate) fn github(
    mints: Arc<std::sync::atomic::AtomicUsize>,
    expires_at: String,
) -> impl Fn(&Req) -> (u16, String) + Send + Sync + 'static {
    use std::sync::atomic::Ordering;
    let not_found = || (404, r#"{"message":"Not Found"}"#.to_string());
    move |r: &Req| {
        let p = r.path.as_str();
        match (r.method.as_str(), p) {
            ("POST", "/app-manifests/goodcode/conversions") => (201, conversion().to_string()),
            ("POST", p) if p.starts_with("/app-manifests/") => not_found(),
            ("GET", "/users/atlas-crew-box%5Bbot%5D") => (200, r#"{"id":4242}"#.into()),
            ("GET", "/app/installations?per_page=100") => (
                200,
                serde_json::json!([{"id": 7, "account": {"login": "acme"}, "repository_selection": "selected"}])
                    .to_string(),
            ),
            ("GET", "/repos/acme/web/installation") => (200, r#"{"id":7}"#.into()),
            ("GET", p) if p.ends_with("/installation") => not_found(),
            ("POST", "/app/installations/7/access_tokens") => {
                let n = mints.fetch_add(1, Ordering::SeqCst) + 1;
                (
                    201,
                    serde_json::json!({"token": format!("ghs_token_{n}"), "expires_at": expires_at})
                        .to_string(),
                )
            }
            // The webhook: the hook config is accepted, and no delivery has been made yet.
            ("PATCH", "/app/hook/config") => (
                200,
                r#"{"content_type":"json","insecure_ssl":"0","url":"https://crew.example.com/github/webhook"}"#
                    .into(),
            ),
            ("GET", p) if p.starts_with("/app/hook/deliveries") => (200, "[]".into()),
            ("POST", p) if p.starts_with("/app/hook/deliveries/") => (202, "{}".into()),
            ("GET", "/repos/acme/web/issues/5") => (200, r#"{"number":5,"title":"hi"}"#.into()),
            ("POST", "/repos/acme/web/issues/5/comments") => (201, r#"{"id":321}"#.into()),
            _ => (500, r#"{"message":"unexpected"}"#.into()),
        }
    }
}

/// An app already set up in `dir`, as a conversion would have stored it.
pub(crate) fn install_app(dir: &std::path::Path) {
    super::record::AppStore::new(dir)
        .save(&super::record::AppRecord {
            id: 987654,
            client_id: "Iv23liTESTCLIENT".into(),
            slug: "atlas-crew-box".into(),
            name: "atlas-crew-box".into(),
            owner: "acme".into(),
            pem: KEY.into(),
            webhook_secret: WEBHOOK_SECRET.into(),
            permissions: conversion()["permissions"].clone(),
            created_at: "2026-11-19T10:00:00Z".into(),
            bot_id: Some(4242),
        })
        .unwrap();
}
