//! The one door to `api.github.com` for the GitHub App: a request carries either the app's JWT (the `/app/...`
//! routes and the conversion) or an installation token (everything else). Tests point it at a local server
//! (`TEST_API_BASE`); nothing under test reaches github.com.

use std::time::Duration;

use serde_json::Value;

const DEFAULT_BASE: &str = "https://api.github.com";
const TIMEOUT: Duration = Duration::from_secs(20);

/// Test-only: the base URL every `Http::default()` uses (a fake GitHub on loopback). Process-wide: a test that sets
/// it holds [`crate::crew::tests::fixture::lock_agent_bin`] like the other process-wide test seams.
#[cfg(test)]
pub(crate) static TEST_API_BASE: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// A request GitHub answered with a failure status, or never answered (`status` 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failed {
    pub status: u16,
    pub text: String,
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.status == 0 {
            write!(f, "{}", self.text)
        } else {
            write!(f, "GitHub answered {}: {}", self.status, self.text)
        }
    }
}

/// Who the request says it is.
#[derive(Clone, Copy)]
pub(crate) enum Auth<'a> {
    /// The app's JWT.
    App(&'a str),
    /// An installation token.
    Token(&'a str),
    /// Nobody: the manifest conversion is authorised by its one-time code in the path.
    None,
}

#[derive(Clone)]
pub(crate) struct Http {
    client: reqwest::Client,
    base: String,
}

impl Default for Http {
    fn default() -> Self {
        #[cfg(test)]
        if let Some(b) = TEST_API_BASE.lock().clone() {
            return Self::new(&b);
        }
        Self::new(DEFAULT_BASE)
    }
}

impl Http {
    pub(crate) fn new(base: &str) -> Self {
        // The ring provider, so `rustls-no-provider` works (same as the push client).
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            client: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .unwrap_or_default(),
            base: base.trim_end_matches('/').to_string(),
        }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        auth: Auth<'_>,
        body: Option<&Value>,
    ) -> Result<reqwest::Response, Failed> {
        let mut req = self
            .client
            .request(
                method,
                format!("{}/{}", self.base, path.trim_start_matches('/')),
            )
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "atlas-crew");
        match auth {
            Auth::App(t) | Auth::Token(t) => req = req.bearer_auth(t),
            Auth::None => {}
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|e| Failed {
            status: 0,
            text: format!("cannot reach GitHub: {}", e.without_url()),
        })?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        // GitHub's own words (`message`), not the whole body: it can echo what was sent.
        let text = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["message"].as_str().map(str::to_string))
            .unwrap_or_else(|| text.chars().take(200).collect());
        Err(Failed { status, text })
    }

    /// The JSON of a call (`Value::Null` for an empty body).
    pub(crate) async fn json(
        &self,
        method: reqwest::Method,
        path: &str,
        auth: Auth<'_>,
        body: Option<&Value>,
    ) -> Result<Value, Failed> {
        let resp = self.send(method, path, auth, body).await?;
        let bytes = resp.bytes().await.map_err(|e| Failed {
            status: 0,
            text: format!("reading GitHub's answer: {}", e.without_url()),
        })?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| Failed {
            status: 0,
            text: format!("GitHub's answer is not JSON: {e}"),
        })
    }

    /// The text of a call (a job's log).
    pub(crate) async fn text(&self, path: &str, auth: Auth<'_>) -> Result<String, Failed> {
        let resp = self.send(reqwest::Method::GET, path, auth, None).await?;
        resp.text().await.map_err(|e| Failed {
            status: 0,
            text: format!("reading GitHub's answer: {}", e.without_url()),
        })
    }
}
