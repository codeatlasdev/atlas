//! The GitHub App of this crew server: set up from GitHub's manifest conversion, kept in one 0600 file, and used to
//! mint the installation tokens crewmates and hostd reach GitHub with.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use reqwest::Method;
use serde_json::{Value, json};

use atlas_core::progress::Coded;

use super::http::{Auth, Failed, Http};
use super::jwt;
use super::manifest;
use super::record::{AppRecord, AppStore, Identity};
use super::tokens::{Clock, SystemClock, Token, TokenCache};
use crate::crew::audit::Audit;
use crate::protocol::invalid_argument;

pub(crate) const EXISTS: &str = "crew.github_app.exists";
pub(crate) const CODE_EXPIRED: &str = "crew.github_app.code_expired";
pub(crate) const CONVERSION_FAILED: &str = "crew.github_app.conversion_failed";
pub(crate) const NONE: &str = "crew.github_app.none";
pub(crate) const FORBIDDEN: &str = "crew.github_app.forbidden";

pub(crate) fn exists() -> Coded {
    Coded::new(
        EXISTS,
        json!({}),
        "a GitHub App is already set up on this server: remove it first",
    )
}

pub(crate) fn code_expired() -> Coded {
    Coded::new(
        CODE_EXPIRED,
        json!({}),
        "the code is used or older than an hour: start the setup again",
    )
}

pub(crate) fn conversion_failed(status: u16, message: impl Into<String>) -> Coded {
    Coded::new(CONVERSION_FAILED, json!({"status": status}), message)
}

pub(crate) fn none() -> Coded {
    Coded::new(NONE, json!({}), "no GitHub App is set up on this server")
}

pub(crate) fn forbidden() -> Coded {
    Coded::new(
        FORBIDDEN,
        json!({}),
        "only the owner sets up the GitHub App",
    )
}

/// How long an installation lookup is believed: found (the owner may remove the app from the repo) and not found
/// (the owner may add it).
const INSTALL_TTL_SECS: i64 = 10 * 60;
const NO_INSTALL_TTL_SECS: i64 = 60;

/// Why no token was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenError {
    /// No app is set up on this server.
    NoApp,
    /// An app is set up and no installation of it covers `repo`.
    NotCovered(String),
    Failed(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoApp => write!(f, "no GitHub App is set up on this server"),
            Self::NotCovered(r) => write!(
                f,
                "the GitHub App is not installed on {r}: add the repository to its installation"
            ),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// `owner/name` of a repository, safe to put in a URL path: GitHub's own alphabet, and not `.` or `..`.
pub(crate) fn valid_repo(repo: &str) -> Option<(&str, &str)> {
    let (owner, name) = repo.split_once('/')?;
    let part = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    (part(owner) && part(name)).then_some((owner, name))
}

pub(crate) struct AppService {
    store: AppStore,
    http: Http,
    clock: Arc<dyn Clock>,
    audit: Arc<Audit>,
    tokens: TokenCache,
    /// `owner/name` (lower case) -> (installation id, when it was looked up).
    installs: Mutex<HashMap<String, (Option<u64>, i64)>>,
}

impl AppService {
    pub(crate) fn new(dir: &std::path::Path, audit: Arc<Audit>) -> Self {
        Self::with(dir, audit, Http::default(), Arc::new(SystemClock))
    }

    pub(crate) fn with(
        dir: &std::path::Path,
        audit: Arc<Audit>,
        http: Http,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            store: AppStore::new(dir),
            http,
            clock,
            audit,
            tokens: TokenCache::default(),
            installs: Mutex::new(HashMap::new()),
        }
    }

    /// The app, `None` when none is set up. An unreadable file is an error: never "no app".
    pub(crate) fn record(&self) -> Result<Option<AppRecord>, String> {
        self.store.load()
    }

    /// `crew_github_app_manifest`
    pub(crate) fn manifest(
        &self,
        owner_kind: &str,
        org: Option<&str>,
        workflows: bool,
        server_name: &str,
    ) -> Result<Value, Coded> {
        manifest::build(owner_kind, org, workflows, server_name)
    }

    /// `crew_github_app_convert`: trade the one-time `code` for the app's credentials and keep them here.
    pub(crate) async fn convert(&self, code: &str) -> Result<Value, Coded> {
        // The code goes into a URL path: GitHub's codes are hex, so anything else is not one.
        if code.is_empty()
            || code.len() > 128
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid_argument(
                "code",
                "invalid",
                "code: not a manifest code",
            ));
        }
        // Before the call: the code is single use, and one spent on a server that has an app would be lost.
        if self.record().map_err(failed_text)?.is_some() {
            return Err(exists());
        }
        let v = self
            .http
            .json(
                Method::POST,
                &format!("app-manifests/{code}/conversions"),
                Auth::None,
                None,
            )
            .await
            .map_err(|e| match e.status {
                // GitHub answers a spent or too old code with 404 (and 422 when it is malformed).
                404 | 422 => code_expired(),
                status => conversion_failed(status, format!("GitHub refused the conversion: {e}")),
            })?;
        let broken = |what: &str| {
            conversion_failed(200, format!("GitHub's conversion answer has no {what}"))
        };
        let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
        let mut rec = AppRecord {
            id: v["id"].as_u64().ok_or_else(|| broken("id"))?,
            client_id: text("client_id").ok_or_else(|| broken("client_id"))?,
            slug: text("slug").ok_or_else(|| broken("slug"))?,
            name: text("name").unwrap_or_default(),
            owner: v["owner"]["login"].as_str().unwrap_or_default().to_string(),
            pem: text("pem").ok_or_else(|| broken("key"))?,
            webhook_secret: text("webhook_secret").unwrap_or_default(),
            permissions: v["permissions"].clone(),
            created_at: text("created_at").unwrap_or_default(),
            bot_id: None,
        };
        // Make sure the key is one we can sign with before it replaces anything: a key we cannot use is a
        // conversion that failed, found now and not at the first task.
        jwt::sign(&rec.pem, &rec.client_id, self.clock.now())
            .map_err(|e| conversion_failed(200, e))?;
        rec.bot_id = self.lookup_bot_id(&rec.slug).await;
        self.store.save(&rec).map_err(failed_text)?;
        self.audit.record(
            None,
            "github_app_created",
            json!({"slug": rec.slug, "owner": rec.owner}),
        );
        Ok(self.status_of(&rec).await)
    }

    /// `crew_github_app_status`
    pub(crate) async fn status(&self) -> Result<Value, Coded> {
        let Some(mut rec) = self.record().map_err(failed_text)? else {
            return Ok(json!({"app": null}));
        };
        if rec.bot_id.is_none() {
            // A lookup that failed after the conversion is repeated here, once it can work.
            rec.bot_id = self.lookup_bot_id(&rec.slug).await;
            if rec.bot_id.is_some() {
                let _ = self.store.save(&rec);
            }
        }
        Ok(self.status_of(&rec).await)
    }

    async fn status_of(&self, rec: &AppRecord) -> Value {
        let id = rec.identity();
        let mut app = rec.public();
        app["identity"] = json!({"name": id.name, "email": id.email});
        match self.installations(rec).await {
            Ok(list) => app["installations"] = Value::Array(list),
            // The app is there; only the list could not be read (offline, GitHub down). Said, not hidden.
            Err(e) => {
                app["installations"] = json!([]);
                app["installations_error"] = json!(e.to_string());
            }
        }
        json!({ "app": app })
    }

    async fn installations(&self, rec: &AppRecord) -> Result<Vec<Value>, Failed> {
        let jwt = jwt::sign(&rec.pem, &rec.client_id, self.clock.now())
            .map_err(|e| Failed { status: 0, text: e })?;
        let v = self
            .http
            .json(
                Method::GET,
                "app/installations?per_page=100",
                Auth::App(&jwt),
                None,
            )
            .await?;
        Ok(v.as_array()
            .into_iter()
            .flatten()
            .map(|i| {
                json!({
                    "id": i["id"],
                    "account": i["account"]["login"],
                    "repository_selection": i["repository_selection"],
                })
            })
            .collect())
    }

    /// `crew_github_app_remove`: the key is deleted and nothing minted before is handed out again.
    pub(crate) async fn remove(&self) -> Result<Value, Coded> {
        if !self.store.remove().map_err(failed_text)? {
            return Err(none());
        }
        self.tokens.clear().await;
        self.installs.lock().clear();
        self.audit.record(None, "github_app_removed", json!({}));
        Ok(json!({}))
    }

    async fn lookup_bot_id(&self, slug: &str) -> Option<u64> {
        // `[` and `]` of `<slug>[bot]`, escaped.
        self.http
            .json(
                Method::GET,
                &format!("users/{slug}%5Bbot%5D"),
                Auth::None,
                None,
            )
            .await
            .ok()?["id"]
            .as_u64()
    }

    /// The commit identity of the app, `None` without one.
    pub(crate) async fn identity(&self) -> Option<Identity> {
        let mut rec = self.record().ok()??;
        if rec.bot_id.is_none() {
            rec.bot_id = self.lookup_bot_id(&rec.slug).await;
            if rec.bot_id.is_some() {
                let _ = self.store.save(&rec);
            }
        }
        Some(rec.identity())
    }

    /// The installation that covers `repo`, `None` when the app is not installed on it.
    async fn installation_for(
        &self,
        rec: &AppRecord,
        repo: &str,
    ) -> Result<Option<u64>, TokenError> {
        let key = repo.to_ascii_lowercase();
        let now = self.clock.now();
        if let Some((id, at)) = self.installs.lock().get(&key).copied() {
            let ttl = if id.is_some() {
                INSTALL_TTL_SECS
            } else {
                NO_INSTALL_TTL_SECS
            };
            if now - at < ttl {
                return Ok(id);
            }
        }
        let jwt = jwt::sign(&rec.pem, &rec.client_id, now).map_err(TokenError::Failed)?;
        let id = match self
            .http
            .json(
                Method::GET,
                &format!("repos/{repo}/installation"),
                Auth::App(&jwt),
                None,
            )
            .await
        {
            Ok(v) => v["id"].as_u64(),
            Err(e) if e.status == 404 => None,
            Err(e) => return Err(TokenError::Failed(e.to_string())),
        };
        self.installs.lock().insert(key, (id, now));
        Ok(id)
    }

    /// Whether the app is set up and installed on `repo`. A failure to ask is `false` here (callers that must tell
    /// the owner why use [`AppService::token_for`]).
    pub(crate) async fn covers(&self, repo: &str) -> bool {
        self.token_for(repo).await.is_ok()
    }

    /// The installation token for `repo`, scoped to that repository and to the app's own minimal permissions;
    /// cached until five minutes before it expires.
    pub(crate) async fn token_for(&self, repo: &str) -> Result<Token, TokenError> {
        let Some((_, name)) = valid_repo(repo) else {
            return Err(TokenError::Failed(format!("{repo:?} is not owner/repo")));
        };
        let rec = self
            .record()
            .map_err(TokenError::Failed)?
            .ok_or(TokenError::NoApp)?;
        let now = self.clock.now();
        let token = self
            .tokens
            .get_or_mint(repo, now, || async {
                let Some(inst) = self.installation_for(&rec, repo).await? else {
                    return Err(TokenError::NotCovered(repo.to_string()));
                };
                let jwt = jwt::sign(&rec.pem, &rec.client_id, now).map_err(TokenError::Failed)?;
                let body = json!({"repositories": [name], "permissions": wanted(&rec.permissions)});
                let v = self
                    .http
                    .json(
                        Method::POST,
                        &format!("app/installations/{inst}/access_tokens"),
                        Auth::App(&jwt),
                        Some(&body),
                    )
                    .await
                    .inspect_err(|e| {
                        // The installation is gone: look it up again next time.
                        if e.status == 404 {
                            self.installs.lock().remove(&repo.to_ascii_lowercase());
                        }
                    })
                    .map_err(|e| TokenError::Failed(e.to_string()))?;
                let value = v["token"]
                    .as_str()
                    .ok_or_else(|| TokenError::Failed("GitHub's answer has no token".into()))?;
                let exp = v["expires_at"]
                    .as_str()
                    .and_then(|s| s.parse::<jiff::Timestamp>().ok())
                    .ok_or_else(|| {
                        TokenError::Failed("GitHub's answer has no expires_at".into())
                    })?;
                Ok((value.to_string(), exp.as_second()))
            })
            .await?;
        if token.fresh {
            // Only the expiry: never the token.
            self.audit.record(
                None,
                "github_token_minted",
                json!({"repo": repo, "expires_at": token.expires_at}),
            );
        }
        Ok(token)
    }
}

/// What a token asks for: the crew's minimal set, only where the app has it (GitHub refuses a token that asks
/// for more than its app), plus Workflows when the owner opted in.
fn wanted(app: &Value) -> Value {
    let has = |k: &str| app.get(k).is_some() || !app.is_object();
    let mut perms: serde_json::Map<String, Value> = manifest::PERMISSIONS
        .iter()
        .filter(|(k, _)| has(k))
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    if app.get("workflows").is_some() {
        perms.insert("workflows".into(), json!("write"));
    }
    Value::Object(perms)
}

/// A failure of the server's own file (unreadable, not writable): no GitHub status to carry.
fn failed_text(e: String) -> Coded {
    conversion_failed(0, e)
}
