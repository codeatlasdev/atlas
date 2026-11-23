//! The App's webhook (#645): what the hook server's `POST /github/webhook` stands on.
//!
//! - **Signature** ([`check_signature`]): `X-Hub-Signature-256` is `sha256=` + the hex HMAC-SHA256 of the RAW body
//!   under the webhook secret; `ring` compares it in constant time. Nothing is parsed before it passes.
//! - **Replays** ([`DeliveryLog`]): `X-GitHub-Delivery` ids already accepted, kept in `github-deliveries.jsonl` (bounded,
//!   durable before the work starts) so a restart does not forget them.
//! - **Hook state** ([`HookState`], `github-webhook.json`): the URL GitHub was told, which installations delivered a
//!   correctly signed event (only those are "covered": polling stops for their repos), and whether deliveries are
//!   failing (polling resumes).
//! - **GitHub side**: `PATCH /app/hook/config` ([`AppService::ensure_hook`]) and the delivery reconciliation
//!   ([`AppService::reconcile`]): `GET /app/hook/deliveries`, then redeliver the failed ones, bounded and with backoff.
//!
//! The webhook secret lives in `github-app.json` next to the private key (0600) and nowhere else: never in a reply, a
//! log line or an audit entry. Nothing in this file prints it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::http::Auth;
use super::jwt;
use super::service::AppService;

const STATE_FILE: &str = "github-webhook.json";
const DELIVERIES_FILE: &str = "github-deliveries.jsonl";
/// Delivery ids remembered. GitHub's own redelivery window is three days; this holds far more than a busy repo
/// sends in that time.
const KEEP_IDS: usize = 1000;
/// The ids file is cut back to its newest half past this (about 3500 ids of 37 bytes).
const IDS_MAX_BYTES: u64 = 128 * 1024;

/// A refusal of an unauthenticated delivery is audited at most once per reason per this many seconds, with how many
/// were refused since: the URL is public, and whoever finds it must not be able to fill the audit log.
const REFUSAL_WINDOW_SECS: i64 = 60;

/// The newest delivery failed and nothing succeeded for this long: the hook is failing and the poll takes over.
const FAILING_AFTER_SECS: i64 = 30 * 60;
/// GitHub redelivers only what is younger than this.
const REDELIVER_WITHIN_SECS: i64 = 3 * 24 * 3600;
/// Redeliveries of one delivery, and the wait before the next: `BACKOFF_BASE * 2^attempts`.
const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_BASE_SECS: i64 = 5 * 60;
/// Redeliveries made in one pass.
const MAX_PER_PASS: usize = 20;
/// Deliveries read in one pass (one page).
const PAGE: usize = 100;

/// What the signature header says about a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signature {
    /// No `X-Hub-Signature-256`.
    Missing,
    /// Present and wrong, or not `sha256=<hex>`.
    Bad,
    Good,
}

/// Check `header` (`sha256=<hex>`) against the HMAC-SHA256 of `body` under `secret`. An empty secret verifies
/// nothing: HMAC under an empty key is a value anyone can compute.
pub(crate) fn check_signature(secret: &str, header: Option<&str>, body: &[u8]) -> Signature {
    let Some(header) = header else {
        return Signature::Missing;
    };
    if secret.is_empty() {
        return Signature::Bad;
    }
    let Some(sig) = header
        .trim()
        .strip_prefix("sha256=")
        .and_then(|h| hex::decode(h).ok())
    else {
        return Signature::Bad;
    };
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    // `ring` compares in constant time.
    match ring::hmac::verify(&key, body, &sig) {
        Ok(()) => Signature::Good,
        Err(_) => Signature::Bad,
    }
}

/// `X-GitHub-Delivery` is a GUID. Bounded and plain, because it is written to a file and an audit line.
pub(crate) fn valid_delivery_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Why a delivery was not accepted, before anything in it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// No app (or no secret): there is nothing to verify against, so nothing is accepted.
    NotConfigured,
    Unsigned,
    BadSignature,
    /// The app's file cannot be read: a failure on our side, so GitHub sees a failure and redelivers.
    Unreadable,
}

impl Refusal {
    pub(crate) fn status(self) -> &'static str {
        match self {
            Self::NotConfigured => "404 Not Found",
            Self::Unsigned | Self::BadSignature => "401 Unauthorized",
            Self::Unreadable => "500 Internal Server Error",
        }
    }
}

// ─── replays ────────────────────────────────────────────────────────────────────────────────────────────────────────

/// The delivery ids accepted so far.
pub(crate) struct DeliveryLog {
    path: PathBuf,
    ids: Mutex<Seen>,
    /// Ids kept, and the size past which the file is cut back to its newest half ([`KEEP_IDS`], [`IDS_MAX_BYTES`];
    /// smaller in tests, where every claim is an fsync).
    keep: usize,
    max_bytes: u64,
}

#[derive(Default)]
struct Seen {
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl DeliveryLog {
    pub(crate) fn open(dir: &Path) -> Self {
        Self::open_capped(dir, KEEP_IDS, IDS_MAX_BYTES)
    }

    pub(crate) fn open_capped(dir: &Path, keep: usize, max_bytes: u64) -> Self {
        let path = dir.join(DELIVERIES_FILE);
        let mut seen = Seen::default();
        for line in crate::crew::store::jsonl::read_lines(&path) {
            let id = line.trim();
            if valid_delivery_id(id) && seen.set.insert(id.to_string()) {
                seen.order.push_back(id.to_string());
            }
        }
        while seen.order.len() > keep {
            if let Some(old) = seen.order.pop_front() {
                seen.set.remove(&old);
            }
        }
        Self {
            path,
            ids: Mutex::new(seen),
            keep,
            max_bytes,
        }
    }

    /// Take `id`: `Ok(true)` the first time, `Ok(false)` for a replay. The id is on disk before this returns `true`
    /// (a crash after it can lose a delivery's work, never repeat it). An unwritable file is an error, and the
    /// delivery is not taken: GitHub sees the failure and redelivers.
    pub(crate) fn claim(&self, id: &str) -> Result<bool, String> {
        let mut seen = self.ids.lock();
        if seen.set.contains(id) {
            return Ok(false);
        }
        crate::crew::store::jsonl::append_line(&self.path, id, true)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        seen.set.insert(id.to_string());
        seen.order.push_back(id.to_string());
        if seen.order.len() > self.keep
            && let Some(old) = seen.order.pop_front()
        {
            seen.set.remove(&old);
        }
        let _ = crate::crew::store::jsonl::trim_to_tail(
            &self.path,
            self.max_bytes,
            self.max_bytes / 2,
        );
        Ok(true)
    }
}

// ─── hook state ─────────────────────────────────────────────────────────────────────────────────────────────────────

/// `<crew dir>/github-webhook.json`. No secret in it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HookState {
    /// The URL GitHub's hook config was last set to by us. `None`: not synced yet (or the URL changed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_url: Option<String>,
    /// Installation id -> when a correctly signed delivery of it last arrived.
    #[serde(default)]
    pub verified: BTreeMap<u64, i64>,
    /// When the latest correctly signed delivery of any installation arrived: the poll that takes over after a failure
    /// skips what happened before it (the webhook handled it).
    #[serde(default)]
    pub last_ok: i64,
    /// Deliveries are failing: the poll covers every repo until a signed delivery arrives again.
    #[serde(default)]
    pub failing: bool,
    /// When the hook last recovered from `failing`: a failed delivery older than this was covered by the poll and
    /// is never redelivered (it would fire twice).
    #[serde(default)]
    pub healthy_since: i64,
}

pub(crate) struct HookStore {
    path: PathBuf,
    /// Read-modify-write under one lock.
    lock: Mutex<()>,
}

impl HookStore {
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(STATE_FILE),
            lock: Mutex::new(()),
        }
    }

    pub(crate) fn load(&self) -> HookState {
        // tree-access: hostd's own file in the crew dir, never a crewmate's tree
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Change the state and write it when it changed.
    pub(crate) fn update<T>(&self, f: impl FnOnce(&mut HookState) -> T) -> T {
        let _guard = self.lock.lock();
        let mut state = self.load();
        let before = state.clone();
        let out = f(&mut state);
        if state != before
            && let Err(e) = serde_json::to_vec_pretty(&state)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    crate::paths::write_private(&self.path, &b).map_err(|e| e.to_string())
                })
        {
            // i18n-ignore: log
            eprintln!("atlas crew: cannot write {}: {e}", self.path.display());
        }
        out
    }

    pub(crate) fn remove(&self) {
        let _guard = self.lock.lock();
        // tree-access: hostd's own file in the crew dir, never a crewmate's tree
        let _ = std::fs::remove_file(&self.path);
    }
}

// ─── refusals ───────────────────────────────────────────────────────────────────────────────────────────────────────

/// Rate limit of the audit lines for refused (unauthenticated) deliveries.
#[derive(Default)]
pub(crate) struct Refusals {
    /// reason -> (when the last line was audited, refused since)
    by_reason: Mutex<HashMap<&'static str, (i64, u64)>>,
}

impl Refusals {
    /// `Some(n)`: audit a line now saying `n` deliveries were refused for `reason` since the last one. `None`: counted,
    /// and said with the next line.
    pub(crate) fn count(&self, reason: &'static str, now: i64) -> Option<u64> {
        let mut map = self.by_reason.lock();
        let (last, pending) = map.entry(reason).or_insert((i64::MIN, 0));
        *pending += 1;
        if now.saturating_sub(*last) >= REFUSAL_WINDOW_SECS {
            *last = now;
            return Some(std::mem::take(pending));
        }
        None
    }
}

// ─── the service side ───────────────────────────────────────────────────────────────────────────────────────────────

/// What one reconciliation pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Reconciled {
    /// Deliveries redelivered.
    pub redelivered: usize,
    /// Failed deliveries left alone (the hook is failing and the poll covers it, too old, out of attempts, waiting).
    pub skipped: usize,
    /// The hook is failing after this pass.
    pub failing: bool,
}

impl AppService {
    /// Check a delivery before anything in it is read. `Ok(())` only for a correct signature under this app's secret.
    /// An unsigned or wrongly signed one is audited (rate limited) and counted.
    pub(crate) fn verify_delivery(
        &self,
        signature: Option<&str>,
        raw: &[u8],
    ) -> Result<(), Refusal> {
        let rec = match self.record() {
            Ok(Some(r)) if !r.webhook_secret.is_empty() => r,
            Ok(_) => return Err(Refusal::NotConfigured),
            Err(e) => {
                // i18n-ignore: log
                eprintln!("atlas crew: github webhook: {e}");
                return Err(Refusal::Unreadable);
            }
        };
        let (refusal, reason) = match check_signature(&rec.webhook_secret, signature, raw) {
            Signature::Good => return Ok(()),
            Signature::Missing => (Refusal::Unsigned, "unsigned"),
            Signature::Bad => (Refusal::BadSignature, "bad_signature"),
        };
        if let Some(count) = self.refusals.count(reason, self.clock.now()) {
            self.audit.record(
                None,
                "github_webhook_refused",
                json!({"reason": reason, "count": count}),
            );
        }
        Err(refusal)
    }

    /// Take a delivery id (see [`DeliveryLog::claim`]).
    pub(crate) fn claim_delivery(&self, id: &str) -> Result<bool, String> {
        self.deliveries.claim(id)
    }

    /// A signed delivery whose id was taken already: nothing is done, and the audit says so.
    pub(crate) fn audit_duplicate(&self, delivery: &str, event: &str) {
        self.audit.record(
            None,
            "github_webhook_duplicate",
            json!({"delivery": delivery, "event": event}),
        );
    }

    /// A correctly signed delivery of `installation` arrived: its repos are covered by the webhook from now on, and a
    /// failing hook is healthy again.
    pub(crate) fn note_verified(&self, installation: u64) {
        let now = self.clock.now();
        let (first, recovered) = self.hook.update(|s| {
            let first = s.verified.insert(installation, now).is_none();
            let recovered = std::mem::replace(&mut s.failing, false);
            // The first verified delivery starts the clock too: the poll covered everything before it.
            if recovered || s.healthy_since == 0 {
                s.healthy_since = now;
            }
            s.last_ok = now;
            (first, recovered)
        });
        if first {
            self.audit.record(
                None,
                "github_webhook_verified",
                json!({"installation": installation}),
            );
        }
        if recovered {
            self.audit
                .record(None, "github_webhook_recovered", json!({}));
        }
    }

    /// Whether the webhook delivers for `repo`: an installation covers it, that installation has delivered a correctly
    /// signed event, and the hook is not failing. The poll stops for such a repo.
    pub(crate) async fn webhook_covers(&self, repo: &str) -> bool {
        let state = self.hook.load();
        if state.failing || state.verified.is_empty() {
            return false;
        }
        let Ok(Some(rec)) = self.record() else {
            return false;
        };
        match self.installation_for(&rec, repo).await {
            Ok(Some(id)) => state.verified.contains_key(&id),
            _ => false,
        }
    }

    /// The hook state, for the poll that takes over after a failure.
    pub(crate) fn hook_state(&self) -> HookState {
        self.hook.load()
    }

    /// What `crew_github_app_status` says about the webhook. `url` is the setting.
    pub(crate) fn webhook_status(&self, url: Option<&str>) -> Value {
        let s = self.hook.load();
        json!({
            "url": url,
            "synced": url.is_some() && s.synced_url.as_deref() == url,
            "verified_installations": s.verified.len(),
            "failing": s.failing,
        })
    }

    /// The public URL setting changed to `url` (`None`: webhooks off). Everything that was verified under the old one
    /// is forgotten, and GitHub is told the new one. The setting is already saved: a failure here is
    /// retried by the reconciliation loop, and returned for the caller to say.
    pub(crate) async fn webhook_url_changed(&self, url: Option<&str>) -> Result<(), String> {
        self.hook.update(|s| {
            *s = HookState::default();
        });
        match url {
            Some(u) => self.ensure_hook(u).await,
            None => Ok(()),
        }
    }

    /// Point the app's webhook at `url`: `PATCH /app/hook/config` with the secret (made here when the app has none),
    /// JSON payloads, TLS verification on. Without an app there is nothing to do.
    pub(crate) async fn ensure_hook(&self, url: &str) -> Result<(), String> {
        let Some(mut rec) = self.record()? else {
            return Ok(());
        };
        if rec.webhook_secret.is_empty() {
            rec.webhook_secret = crate::paths::random_hex(32).map_err(|e| e.to_string())?;
            // Kept before GitHub hears of it: a secret GitHub has and we lost verifies nothing.
            self.store.save(&rec)?;
        }
        let jwt = jwt::sign(&rec.pem, &rec.client_id, self.clock.now())?;
        let body = json!({
            "url": format!("{url}{}", crate::crew::store::GITHUB_WEBHOOK_PATH),
            "content_type": "json",
            "insecure_ssl": "0",
            "secret": rec.webhook_secret,
        });
        if let Err(e) = self
            .http
            .json(
                Method::PATCH,
                "app/hook/config",
                Auth::App(&jwt),
                Some(&body),
            )
            .await
        {
            let e = e.to_string();
            self.audit.record(
                None,
                "github_webhook_sync_failed",
                json!({"url": url, "error": e}),
            );
            return Err(e);
        }
        let url = url.to_string();
        self.hook.update(|s| s.synced_url = Some(url.clone()));
        self.audit
            .record(None, "github_webhook_configured", json!({"url": url}));
        Ok(())
    }

    /// One reconciliation pass (`url`: the setting; `None` does nothing). Sets the URL when it was not set yet, reads
    /// the recent deliveries, decides whether the hook is failing, and redelivers the failed ones while it is not.
    pub(crate) async fn reconcile(&self, url: Option<&str>) -> Result<Reconciled, String> {
        let Some(url) = url else {
            return Ok(Reconciled::default());
        };
        let Some(rec) = self.record()? else {
            return Ok(Reconciled::default());
        };
        if self.hook.load().synced_url.as_deref() != Some(url) {
            self.ensure_hook(url).await?;
        }
        let now = self.clock.now();
        let jwt = jwt::sign(&rec.pem, &rec.client_id, now)?;
        let list = self
            .http
            .json(
                Method::GET,
                &format!("app/hook/deliveries?per_page={PAGE}"),
                Auth::App(&jwt),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let attempts = Attempts::of(&list);

        let failing = attempts.failing(now);
        let flipped = self.hook.update(|s| {
            let was = std::mem::replace(&mut s.failing, failing);
            if was && !failing {
                s.healthy_since = now;
            }
            was != failing
        });
        if flipped {
            self.audit.record(
                None,
                if failing {
                    "github_webhook_failing"
                } else {
                    "github_webhook_recovered"
                },
                json!({}),
            );
        }
        let mut out = Reconciled {
            failing,
            ..Default::default()
        };
        if failing {
            // The poll covers every repo while the hook is down; a redelivery of what it handled would fire twice.
            out.skipped = attempts.failed(0, now).count();
            return Ok(out);
        }
        let since = self.hook.load().healthy_since;
        if since == 0 {
            // Nothing has been verified yet: the poll covers every repo, so a redelivery would fire twice.
            out.skipped = attempts.failed(0, now).count();
            return Ok(out);
        }
        for d in attempts.failed(since, now) {
            if out.redelivered >= MAX_PER_PASS {
                out.skipped += 1;
                continue;
            }
            let (n, last) = self
                .redeliveries
                .lock()
                .get(&d.guid)
                .copied()
                .unwrap_or((0, 0));
            if n >= MAX_ATTEMPTS || now - last < BACKOFF_BASE_SECS << n {
                out.skipped += 1;
                continue;
            }
            let jwt = jwt::sign(&rec.pem, &rec.client_id, now)?;
            let path = format!("app/hook/deliveries/{}/attempts", d.id);
            // A failed redelivery counts as an attempt too: the backoff applies to it.
            self.redeliveries
                .lock()
                .insert(d.guid.clone(), (n + 1, now));
            match self
                .http
                .json(Method::POST, &path, Auth::App(&jwt), None)
                .await
            {
                Ok(_) => {
                    out.redelivered += 1;
                    self.audit.record(
                        None,
                        "github_webhook_redelivered",
                        json!({"delivery": d.guid, "attempt": n + 1}),
                    );
                }
                Err(e) => {
                    out.skipped += 1;
                    // i18n-ignore: log
                    eprintln!("atlas crew: github webhook: redelivery of {}: {e}", d.guid);
                }
            }
        }
        // The map only needs the deliveries GitHub still lists.
        let live: HashSet<&str> = attempts.guids();
        self.redeliveries
            .lock()
            .retain(|g, _| live.contains(g.as_str()));
        Ok(out)
    }
}

/// One delivery as `GET /app/hook/deliveries` lists it, the attempts of one GUID folded together.
#[derive(Debug, Clone)]
struct Delivery {
    /// The id the redeliver endpoint takes (the newest attempt's).
    id: u64,
    guid: String,
    /// Unix seconds of the newest attempt.
    at: i64,
    ok: bool,
}

struct Attempts(Vec<Delivery>);

impl Attempts {
    fn of(list: &Value) -> Self {
        let mut by_guid: Vec<Delivery> = Vec::new();
        for e in list.as_array().into_iter().flatten() {
            let (Some(id), Some(guid)) = (e["id"].as_u64(), e["guid"].as_str()) else {
                continue;
            };
            let at = e["delivered_at"]
                .as_str()
                .and_then(|s| s.parse::<jiff::Timestamp>().ok())
                .map_or(0, |t| t.as_second());
            let ok = e["status_code"]
                .as_u64()
                .is_some_and(|c| (200..300).contains(&c));
            match by_guid.iter_mut().find(|d| d.guid == guid) {
                Some(d) => {
                    d.ok |= ok;
                    if at > d.at {
                        d.at = at;
                        d.id = id;
                    }
                }
                None => by_guid.push(Delivery {
                    id,
                    guid: guid.to_string(),
                    at,
                    ok,
                }),
            }
        }
        by_guid.sort_by_key(|d| std::cmp::Reverse(d.at));
        Self(by_guid)
    }

    /// The newest delivery failed and nothing succeeded in the last [`FAILING_AFTER_SECS`]. No deliveries at all is
    /// not failing: nothing happened.
    fn failing(&self, now: i64) -> bool {
        let Some(newest) = self.0.first() else {
            return false;
        };
        !newest.ok
            && !self
                .0
                .iter()
                .any(|d| d.ok && now.saturating_sub(d.at) < FAILING_AFTER_SECS)
    }

    /// Failed deliveries newer than `since` and inside GitHub's redelivery window, oldest first (the order things
    /// happened).
    fn failed(&self, since: i64, now: i64) -> impl Iterator<Item = &Delivery> {
        self.0.iter().rev().filter(move |d| {
            !d.ok && d.at > since && now.saturating_sub(d.at) < REDELIVER_WITHIN_SECS
        })
    }

    fn guids(&self) -> HashSet<&str> {
        self.0.iter().map(|d| d.guid.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GitHub's published test vector (docs.github.com, "Validating webhook deliveries").
    const VECTOR_SECRET: &str = "It's a Secret to Everybody";
    const VECTOR_BODY: &[u8] = b"Hello, World!";
    const VECTOR_SIG: &str =
        "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

    #[test]
    fn githubs_published_test_vector_verifies() {
        assert_eq!(
            check_signature(VECTOR_SECRET, Some(VECTOR_SIG), VECTOR_BODY),
            Signature::Good
        );
    }

    #[test]
    fn a_signature_is_good_only_for_its_own_secret_and_its_own_bytes() {
        let sig = Some(VECTOR_SIG);
        // A body that differs by one byte (the signature is over the RAW bytes, trailing newline included).
        assert_eq!(
            check_signature(VECTOR_SECRET, sig, b"Hello, World!\n"),
            Signature::Bad
        );
        assert_eq!(
            check_signature("It's a secret to Everybody", sig, VECTOR_BODY),
            Signature::Bad
        );
        // Missing, and shapes that are not `sha256=<hex>`.
        assert_eq!(
            check_signature(VECTOR_SECRET, None, VECTOR_BODY),
            Signature::Missing
        );
        for bad in [
            "",
            "sha256=",
            "sha256=zz",
            "sha1=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17",
            &VECTOR_SIG[7..],
            // A truncated digest is a different (shorter) tag, never a prefix match.
            "sha256=757107ea0eb2509fc211221cce984b8a",
        ] {
            assert_eq!(
                check_signature(VECTOR_SECRET, Some(bad), VECTOR_BODY),
                Signature::Bad,
                "{bad:?}"
            );
        }
    }

    // HMAC under an empty key is computable by anyone: a server with no secret verifies nothing.
    #[test]
    fn an_empty_secret_verifies_nothing() {
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, b"");
        let tag = ring::hmac::sign(&key, VECTOR_BODY);
        let header = format!("sha256={}", hex::encode(tag.as_ref()));
        assert_eq!(
            check_signature("", Some(&header), VECTOR_BODY),
            Signature::Bad
        );
    }

    #[test]
    fn delivery_ids_are_plain_and_bounded() {
        assert!(valid_delivery_id("72d3162e-cc78-11e3-81ab-4c9367dc0958"));
        for bad in ["", "a b", "a\nb", "../x", &"a".repeat(65)] {
            assert!(!valid_delivery_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_delivery_id_is_taken_once_and_remembered_across_a_restart() {
        let t = tempfile::tempdir().unwrap();
        let log = DeliveryLog::open(t.path());
        assert_eq!(log.claim("d-1"), Ok(true));
        assert_eq!(log.claim("d-1"), Ok(false), "a replay");
        assert_eq!(log.claim("d-2"), Ok(true));
        // A new process reads the file.
        let again = DeliveryLog::open(t.path());
        assert_eq!(again.claim("d-1"), Ok(false));
        assert_eq!(again.claim("d-2"), Ok(false));
        assert_eq!(again.claim("d-3"), Ok(true));
    }

    // The store is bounded: past the cap the oldest id is forgotten (memory and file), the newest are kept. Each
    // claim fsyncs, which costs milliseconds on a Windows runner (1 200 of them took 31 s), so the caps are small:
    // the same eviction and the same trim, crossed a few times over.
    #[test]
    fn the_delivery_log_is_bounded_and_keeps_the_newest() {
        let t = tempfile::tempdir().unwrap();
        let (keep, max_bytes) = (20usize, 400u64);
        let log = DeliveryLog::open_capped(t.path(), keep, max_bytes);
        let total = keep + 50;
        for n in 0..total {
            assert_eq!(log.claim(&format!("id-{n:06}")), Ok(true));
        }
        assert_eq!(log.claim("id-000000"), Ok(true), "the oldest is forgotten");
        assert_eq!(
            log.claim(&format!("id-{:06}", total - 1)),
            Ok(false),
            "the newest is kept"
        );
        let again = DeliveryLog::open_capped(t.path(), keep, max_bytes);
        assert_eq!(again.claim(&format!("id-{:06}", total - 1)), Ok(false));
        let len = std::fs::metadata(t.path().join(DELIVERIES_FILE))
            .unwrap()
            .len();
        // 10 bytes an id: the 71 written would be 710 without the trim.
        assert!(len <= max_bytes + 10, "the file grew to {len}");
    }

    #[test]
    fn refusals_are_audited_once_a_minute_per_reason_with_their_count() {
        let r = Refusals::default();
        assert_eq!(r.count("unsigned", 1000), Some(1));
        assert_eq!(r.count("unsigned", 1001), None);
        assert_eq!(r.count("unsigned", 1059), None);
        // Another reason has its own window.
        assert_eq!(r.count("bad_signature", 1002), Some(1));
        // The line that opens the next window says how many were refused in between (and itself).
        assert_eq!(r.count("unsigned", 1060), Some(3));
        assert_eq!(r.count("unsigned", 1061), None);
    }

    fn deliveries(rows: &[(u64, &str, &str, u64)]) -> Value {
        Value::Array(
            rows.iter()
                .map(|(id, guid, at, status)| {
                    json!({"id": id, "guid": guid, "delivered_at": at, "status_code": status})
                })
                .collect(),
        )
    }

    // A GUID whose redelivery succeeded is delivered; only the ones with no 2xx attempt are failed. The newest
    // attempt's id is the one the redeliver endpoint wants.
    #[test]
    fn attempts_of_one_guid_fold_and_only_never_successful_guids_are_failed() {
        let list = deliveries(&[
            (4, "b", "2026-11-19T10:03:00Z", 502),
            (3, "a", "2026-11-19T10:02:00Z", 200),
            (2, "b", "2026-11-19T10:01:00Z", 502),
            (1, "a", "2026-11-19T10:00:00Z", 500),
        ]);
        let a = Attempts::of(&list);
        let now = "2026-11-19T10:04:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second();
        let failed: Vec<_> = a.failed(0, now).map(|d| (d.guid.clone(), d.id)).collect();
        assert_eq!(failed, vec![("b".to_string(), 4)]);
        // The newest delivery failed but "a" succeeded two minutes ago: the hook is up.
        assert!(!a.failing(now));
        // Nothing succeeded for over half an hour: down.
        assert!(a.failing(now + FAILING_AFTER_SECS + 3 * 60));
        assert!(!Attempts::of(&json!([])).failing(now));
    }
}
