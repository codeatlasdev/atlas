//! Push notification sender via the Atlas push relay.
//!
//! `ATLAS_PUSH_RELAY` env (default `https://push.codeatlas.com.br`); `"off"` disables.
//! Failures are logged to stderr, never retried.

use std::path::Path;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PushToken {
    pub node_id: String,
    pub token: String,
    pub platform: String,
    pub registered_at: u64,
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

fn relay_url() -> Option<String> {
    let v = std::env::var("ATLAS_PUSH_RELAY").unwrap_or_default();
    if v == "off" {
        return None;
    }
    let base = if v.is_empty() {
        "https://push.codeatlas.com.br".to_string()
    } else {
        v
    };
    Some(format!("{base}/push"))
}

/// POST `{token, kind}` to the relay for every token.
/// Returns the tokens that got a `410 Gone` (caller should drop them).
pub async fn notify(tokens: &[PushToken], kind: &str) -> Vec<String> {
    let (Some(client), Some(url)) = (HTTP.as_ref(), relay_url()) else {
        return vec![];
    };
    let mut expired = vec![];
    for t in tokens {
        let body = json!({"token": t.token, "kind": kind});
        match client.post(&url).json(&body).send().await {
            Ok(r) if r.status().as_u16() == 410 => {
                expired.push(t.token.clone());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("atlas bots: push notify error: {e}");
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
        eprintln!("atlas bots: push save_tokens error: {e}");
    }
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Build a new `PushToken` with the current timestamp.
pub fn new_token(
    node_id: impl Into<String>,
    token: impl Into<String>,
    platform: impl Into<String>,
) -> PushToken {
    PushToken {
        node_id: node_id.into(),
        token: token.into(),
        platform: platform.into(),
        registered_at: now_s(),
    }
}
