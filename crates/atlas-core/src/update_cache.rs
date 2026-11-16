//! The update-check cache (`~/.atlas/.update_check`): what the last check of the release channel
//! found. `atlas` (CLI) writes it and decides from it (the exit banner, the pre-flight prompt, the
//! background check); hostd reads it to tell a phone which version is on offer without a network
//! call. One definition, so the writer and the readers cannot disagree on the shape.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::channel::{self, Action, Channel, Decision};

/// File name inside `~/.atlas`.
pub const FILE: &str = ".update_check";

/// Hit GitHub at most every 6h (cached result is reused in between).
pub const CHECK_INTERVAL_SECS: u64 = 6 * 3600;
/// After a failed live check (offline), do not try again for this long.
pub const FAILED_BACKOFF_SECS: u64 = 30 * 60;
/// How long a "No" to the pre-flight prompt silences that version.
pub const DISMISS_SECS: u64 = 24 * 3600;

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct UpdateCache {
    pub last_checked: u64,
    pub available_version: Option<String>,
    /// "Not now" answer to the pre-flight prompt: that version is not offered again
    /// until `until` (unix secs). Absent in caches written by older atlas versions.
    #[serde(default)]
    pub dismissed: Option<Dismissal>,
    /// The channel `available_version` was computed for. Absent in caches written before
    /// channels existed, which only ever knew the stable releases.
    #[serde(default)]
    pub channel: Option<String>,
    /// When the last live check *failed* (offline, GitHub down), unix secs. Until
    /// [`FAILED_BACKOFF_SECS`] passed, nothing re-checks, so an offline `atlas dev` does not
    /// wait on the network at every start. Cleared by the next successful check.
    #[serde(default)]
    pub failed_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Dismissal {
    pub version: String,
    pub until: u64,
}

/// What a phone is told about updates (`info.update`): the cache, read for the channel this
/// machine follows. `offered` is only ever an upgrade over `installed`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub installed: String,
    pub offered: Option<String>,
    pub channel: Channel,
    pub action: Action,
    /// Unix seconds of the check that produced this.
    pub checked_at: u64,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `~/.atlas/.update_check`.
pub fn path() -> PathBuf {
    crate::paths::atlas_home().join(FILE)
}

/// What a background check may report for `offered`: the version when it is an
/// [`Decision::Upgrade`] over `installed`, nothing for current or older builds.
pub fn background_available(installed: &semver::Version, offered: &str) -> Option<String> {
    let offered_version = channel::parse_tag(offered)?;
    (channel::decide(installed, &offered_version) == Decision::Upgrade).then(|| offered.to_string())
}

impl UpdateCache {
    /// The cache at `path`. A file that does not parse as a whole (a field of an unexpected type,
    /// a hand edit) still keeps what can be salvaged — the dismissal above all, so "No" to the
    /// prompt is not forgotten — and counts as never checked. Missing or unreadable: default.
    pub fn load_at(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        if let Ok(cache) = serde_json::from_str(&text) {
            return cache;
        }
        let dismissed = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| serde_json::from_value::<Dismissal>(v.get("dismissed")?.clone()).ok());
        Self {
            dismissed,
            ..Self::default()
        }
    }

    /// Cache after a check on `on`: fresh timestamp and result, keeping a still-relevant dismissal.
    pub fn checked_for(&self, on: Channel, available: Option<String>) -> Self {
        Self {
            last_checked: now_secs(),
            available_version: available,
            dismissed: self.dismissed.clone(),
            channel: Some(on.as_str().to_string()),
            failed_at: None,
        }
    }

    /// Cache after a check on `on` that could not reach GitHub: what was known for `on` stays
    /// (the notice keeps working from it), the failure is stamped so nothing retries for a while.
    /// An offer computed for another channel is not carried over.
    pub fn failed_for(&self, on: Channel, now: u64) -> Self {
        let same = self.valid_for(on);
        Self {
            last_checked: if same { self.last_checked } else { 0 },
            available_version: if same {
                self.available_version.clone()
            } else {
                None
            },
            dismissed: self.dismissed.clone(),
            channel: Some(on.as_str().to_string()),
            failed_at: Some(now),
        }
    }

    /// Computed for `on` (a cache without a channel is a stable one).
    pub fn valid_for(&self, on: Channel) -> bool {
        self.channel.as_deref().unwrap_or(Channel::Stable.as_str()) == on.as_str()
    }

    /// A check is due: the cache is older than the check interval, never written, or computed
    /// for another channel, and the last attempt did not fail within the backoff window.
    pub fn is_stale_for(&self, on: Channel, now: u64) -> bool {
        if self
            .failed_at
            .is_some_and(|f| now.saturating_sub(f) < FAILED_BACKOFF_SECS)
        {
            return false;
        }
        !self.valid_for(on) || now.saturating_sub(self.last_checked) >= CHECK_INTERVAL_SECS
    }

    /// The cached version when it is an upgrade over `installed` on `on`, else `None`.
    pub fn available_for(&self, installed: &semver::Version, on: Channel) -> Option<String> {
        if !self.valid_for(on) {
            return None;
        }
        background_available(installed, self.available_version.as_deref()?)
    }

    /// `version` was dismissed and the 24h silence has not elapsed.
    pub fn is_dismissed(&self, version: &str, now: u64) -> bool {
        self.dismissed
            .as_ref()
            .is_some_and(|d| d.version == version && now < d.until)
    }

    pub fn dismiss(&mut self, version: &str, now: u64) {
        self.dismissed = Some(Dismissal {
            version: version.to_string(),
            until: now + DISMISS_SECS,
        });
    }

    /// What a check on `followed` found, for a client running `installed`. `None` when no
    /// successful check of that channel is on record (never checked, an unreadable file, or the
    /// user switched channel since: that offer is another channel's) or `installed` is not SemVer.
    pub fn summary(&self, installed: &str, followed: Channel) -> Option<Summary> {
        let version = channel::parse_tag(installed)?;
        if self.last_checked == 0 || !self.valid_for(followed) {
            return None;
        }
        let offered = self.available_for(&version, followed);
        Some(Summary {
            installed: version.to_string(),
            action: if offered.is_some() {
                Action::Update
            } else {
                Action::None
            },
            offered,
            channel: followed,
            checked_at: self.last_checked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(last: u64, offered: Option<&str>, channel: Option<&str>) -> UpdateCache {
        UpdateCache {
            last_checked: last,
            available_version: offered.map(Into::into),
            channel: channel.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn a_cached_upgrade_is_reported_with_its_channel_and_time() {
        let s = cache(1_700, Some("1.6.0"), Some("beta"))
            .summary("1.5.0", Channel::Beta)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({
                "installed": "1.5.0",
                "offered": "1.6.0",
                "channel": "beta",
                "action": "update",
                "checked_at": 1_700,
            })
        );
    }

    #[test]
    fn a_check_that_found_nothing_is_current_not_absent() {
        let s = cache(1_700, None, Some("alpha"))
            .summary("1.5.0", Channel::Alpha)
            .unwrap();
        assert_eq!((s.offered, s.action), (None, Action::None));
    }

    /// The phone must never be told "update to X" for a build that is not newer than the one
    /// running (the host may already be on it: the cache is written by the CLI, not by hostd).
    #[test]
    fn an_offer_that_is_not_newer_than_the_running_build_is_not_an_update() {
        for offered in ["1.5.0", "1.4.0", "1.5.0-alpha.2"] {
            let s = cache(9, Some(offered), Some("alpha"))
                .summary("1.5.0", Channel::Alpha)
                .unwrap();
            assert_eq!((s.offered, s.action), (None, Action::None), "{offered}");
        }
    }

    #[test]
    fn nothing_is_reported_without_a_successful_check_of_the_followed_channel() {
        // Never checked, or only a failed check on record (last_checked stays 0).
        assert!(
            UpdateCache::default()
                .summary("1.5.0", Channel::Alpha)
                .is_none()
        );
        assert!(
            UpdateCache::default()
                .failed_for(Channel::Alpha, 5)
                .summary("1.5.0", Channel::Alpha)
                .is_none()
        );
        // Another channel's offer is not this one's.
        assert!(
            cache(9, Some("1.6.0-beta.1"), Some("beta"))
                .summary("1.5.0", Channel::Stable)
                .is_none()
        );
        // A cache from before channels was a Stable one.
        assert!(
            cache(9, Some("1.6.0"), None)
                .summary("1.5.0", Channel::Stable)
                .is_some()
        );
        assert!(
            cache(9, Some("1.6.0"), Some("alpha"))
                .summary("not-a-version", Channel::Alpha)
                .is_none()
        );
    }

    fn ver(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn a_truncated_cache_still_counts_as_never_checked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c");
        std::fs::write(&path, r#"{"last_checked":17,"available_ver"#).unwrap();
        assert_eq!(UpdateCache::load_at(&path), UpdateCache::default());
        assert!(UpdateCache::load_at(&path).is_stale_for(Channel::Alpha, 20));
    }

    /// The reported loss: a cache with a good dismissal but one field of the wrong type used to
    /// reset to `default()`, so "No" to the prompt was forgotten and `atlas dev` asked again.
    #[test]
    fn a_cache_that_does_not_parse_keeps_its_dismissal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c");
        std::fs::write(
            &path,
            r#"{"last_checked":"yesterday","available_version":"9.9.9",
                "dismissed":{"version":"9.9.9","until":4102444800}}"#,
        )
        .unwrap();
        let c = UpdateCache::load_at(&path);
        assert!(c.is_dismissed("9.9.9", 1_000), "{c:?}");
        assert_eq!(c.available_version, None, "the rest is not trusted");
        assert!(
            c.is_stale_for(Channel::Alpha, 1_000),
            "and the next check refetches"
        );
    }

    #[test]
    fn garbage_or_a_missing_cache_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            UpdateCache::load_at(&dir.path().join("none")),
            UpdateCache::default()
        );
        let p = dir.path().join("g");
        std::fs::write(&p, "not json at all").unwrap();
        assert_eq!(UpdateCache::load_at(&p), UpdateCache::default());
    }

    // A failed check is stamped with a backoff.

    #[test]
    fn a_failed_check_backs_off_and_keeps_what_was_known() {
        let known = UpdateCache {
            last_checked: 1_000,
            available_version: Some("9.9.9".into()),
            dismissed: None,
            channel: Some("alpha".into()),
            failed_at: None,
        };
        let now = 1_000 + CHECK_INTERVAL_SECS + 10; // stale: a check is due
        assert!(known.is_stale_for(Channel::Alpha, now));
        let failed = known.failed_for(Channel::Alpha, now);
        assert_eq!(
            failed.available_version.as_deref(),
            Some("9.9.9"),
            "notice keeps working"
        );
        assert_eq!(failed.last_checked, 1_000, "not pretended fresh");
        assert!(
            !failed.is_stale_for(Channel::Alpha, now + 1),
            "no retry inside the backoff"
        );
        assert!(!failed.is_stale_for(Channel::Alpha, now + FAILED_BACKOFF_SECS - 1));
        assert!(
            failed.is_stale_for(Channel::Alpha, now + FAILED_BACKOFF_SECS),
            "then it retries"
        );
    }

    #[test]
    fn a_successful_check_clears_the_failure() {
        let failed = UpdateCache::default().failed_for(Channel::Alpha, 50);
        assert_eq!(failed.failed_at, Some(50));
        assert_eq!(failed.checked_for(Channel::Alpha, None).failed_at, None);
    }

    #[test]
    fn a_failure_never_carries_another_channels_offer() {
        let beta = UpdateCache::default().checked_for(Channel::Beta, Some("2.0.0-beta.1".into()));
        let failed = beta.failed_for(Channel::Alpha, 99);
        assert_eq!(failed.available_version, None);
        assert_eq!(failed.channel.as_deref(), Some("alpha"));
        assert_eq!(failed.available_for(&ver("1.0.0"), Channel::Alpha), None);
    }

    #[test]
    fn a_failure_keeps_the_dismissal() {
        let mut c = UpdateCache::default();
        c.dismiss("2.0.0", 10);
        assert!(c.failed_for(Channel::Alpha, 20).is_dismissed("2.0.0", 21));
    }
}
