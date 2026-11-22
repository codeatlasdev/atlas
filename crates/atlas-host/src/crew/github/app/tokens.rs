//! Installation tokens, minted per repository and cached until shortly before they expire.
//!
//! A token lives an hour. It is kept in memory only (never written anywhere), is scoped to one repository
//! (`repositories: [repo]`) and to the app's own minimal permissions, and is replaced [`REFRESH_MARGIN_SECS`] before
//! its expiry so a caller never receives one that dies in the middle of a push. The clock is a seam: the refresh
//! is tested with a fake one.

use std::collections::HashMap;

pub(crate) const REFRESH_MARGIN_SECS: i64 = 5 * 60;

pub(crate) trait Clock: Send + Sync {
    /// Unix seconds.
    fn now(&self) -> i64;
}

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    }
}

/// A token and when GitHub says it stops working.
#[derive(Clone)]
pub(crate) struct Token {
    pub value: String,
    /// Unix seconds.
    pub expires_at: i64,
    /// Made by this call (a cache hit is `false`): what the audit records.
    pub fresh: bool,
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("expires_at", &self.expires_at)
            .field("fresh", &self.fresh)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub(crate) struct TokenCache {
    // One lock across the mint: two crewmates asking for the same repo at once make one token, not two.
    by_repo: tokio::sync::Mutex<HashMap<String, (String, i64)>>,
}

impl TokenCache {
    /// The cached token of `repo` while it has more than [`REFRESH_MARGIN_SECS`] left, else the one `mint` makes
    /// (`(token, expires_at)`).
    pub(crate) async fn get_or_mint<E, F>(
        &self,
        repo: &str,
        now: i64,
        mint: impl FnOnce() -> F,
    ) -> Result<Token, E>
    where
        F: Future<Output = Result<(String, i64), E>>,
    {
        let key = repo.to_ascii_lowercase();
        let mut map = self.by_repo.lock().await;
        if let Some((value, expires_at)) = map.get(&key)
            && now < expires_at - REFRESH_MARGIN_SECS
        {
            return Ok(Token {
                value: value.clone(),
                expires_at: *expires_at,
                fresh: false,
            });
        }
        let (value, expires_at) = mint().await?;
        map.insert(key, (value.clone(), expires_at));
        Ok(Token {
            value,
            expires_at,
            fresh: true,
        })
    }

    /// Forget everything (the app was removed: its tokens are not ours to hand out any more).
    pub(crate) async fn clear(&self) {
        self.by_repo.lock().await.clear();
    }
}
