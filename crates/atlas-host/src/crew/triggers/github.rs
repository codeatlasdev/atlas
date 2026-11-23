//! GitHub event polling for `Github` triggers.
//!
//! Polls the repo's events every 2 minutes through the crew's [`GithubApi`] (`gh api`). Deduplicates
//! seen event ids via `<crewmate_dir>/<trigger_id>_seen.jsonl`. What an event means (and whether
//! its author may start a task) is decided in `github_events`; this file only walks the feed.
//! If `gh` is missing or errors, it logs to stderr and tries again next time.
//!
//! Dedupe is durable before it fires: an event is written to the seen file first, and only then
//! becomes a task, so a crash or an unwritable file can lose a fire but never repeat one. A trigger
//! whose seen file cannot be written is left alone for [`BACKOFF`] instead of polled every 2 minutes.
//! A trigger with no seen file yet is new: the feed as it stands is marked seen and fires nothing, so
//! enabling one never answers the ~30 old events the feed still lists.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::github_events::{Cfg, decide};
use crate::crew::github::GithubApi;
use crate::crew::store::{Crewmate, Trigger, TriggerKind};
use crate::server::Hub;

const POLL_EVERY: Duration = Duration::from_secs(120);
/// The seen file is cut back to its newest ids once it passes this (the events API lists recent events only).
pub(super) const SEEN_MAX_BYTES: u64 = 256 * 1024;

/// How long a trigger whose seen file cannot be written is left alone.
const BACKOFF: Duration = Duration::from_secs(15 * 60);

/// Triggers (`<crewmate>:<trigger>`) not polled until the instant, because their seen file is unwritable.
static HELD: parking_lot::Mutex<Option<HashMap<String, Instant>>> = parking_lot::Mutex::new(None);

fn held(key: &str) -> bool {
    HELD.lock()
        .as_ref()
        .and_then(|m| m.get(key))
        .is_some_and(|until| Instant::now() < *until)
}

fn hold(key: String) {
    HELD.lock()
        .get_or_insert_with(HashMap::new)
        .insert(key, Instant::now() + BACKOFF);
}

/// Test-only: how often the poller runs instead of every 2 minutes.
#[cfg(test)]
pub(crate) static TEST_GITHUB_POLL: parking_lot::Mutex<Option<Duration>> =
    parking_lot::Mutex::new(None);

fn poll_every() -> Duration {
    #[cfg(test)]
    if let Some(d) = *TEST_GITHUB_POLL.lock() {
        return d;
    }
    POLL_EVERY
}

pub(super) async fn run(hub_weak: Weak<Hub>) {
    loop {
        tokio::time::sleep(poll_every()).await;
        let Some(hub) = hub_weak.upgrade() else { break };
        poll_all(&hub).await;
    }
}

async fn poll_all(hub: &Arc<Hub>) {
    // Paused: not even polled, so nothing is fired and nothing is marked seen behind the user's back.
    if hub.crew.is_paused() {
        return;
    }
    let api = hub.crew.github();
    for crewmate in hub.crew.store.list_crewmates() {
        for trigger in crewmate.triggers.iter().filter(|t| t.enabled) {
            if matches!(trigger.kind, TriggerKind::Github { .. }) {
                poll_trigger(hub, api.as_ref(), &crewmate, trigger).await;
            }
        }
    }
}

async fn poll_trigger(hub: &Arc<Hub>, api: &dyn GithubApi, crewmate: &Crewmate, trigger: &Trigger) {
    let TriggerKind::Github { repo, .. } = &trigger.kind else {
        return;
    };
    let key = format!("{}:{}", crewmate.id, trigger.id);
    if held(&key) {
        return;
    }
    let dir = hub.crew.store.crewmate_dir(&crewmate.id);
    let seen_path = dir.join(format!("{}_seen.jsonl", trigger.id));
    // Marks a trigger whose repo the webhook covers: the poll is off for it, and when it comes back it must not
    // answer what the webhook already did (see `hook_cutoff`).
    let hooked = dir.join(format!("{}_seen.hooked", trigger.id));
    if hub.crew.store.load_crew_settings().public_url.is_some()
        && hub.crew.github_app.webhook_covers(repo).await
    {
        if !hooked.exists()
            && let Err(e) = std::fs::write(&hooked, b"")
        {
            // i18n-ignore: log
            eprintln!(
                "triggers/github: {repo}: cannot mark trigger {} as covered by the webhook: {e}",
                trigger.id
            );
        }
        return;
    }
    let resuming = hooked.exists();
    let is_new = !seen_path.exists() && !resuming;
    let mut seen = load_seen(&seen_path);

    let feed = match api.events(repo).await {
        Ok(f) => f,
        Err(e) => {
            // i18n-ignore: log
            eprintln!("triggers/github: {repo}: {e}");
            return;
        }
    };
    // Who the crew writes as: its own comments never start a task. Unknown = nothing is polled
    // (an event could be our own comment), and it is asked again next time.
    let own_login = match api.login().await {
        Ok(l) => l,
        Err(e) => {
            // i18n-ignore: log
            eprintln!("triggers/github: cannot tell the crew's own GitHub login: {e}");
            return;
        }
    };
    let Some((_, cfg)) = Cfg::for_trigger(crewmate, trigger, own_login) else {
        return;
    };

    // A new trigger starts at the head of the feed: what is already there is history.
    if is_new {
        let ids: Vec<&str> = feed
            .iter()
            .filter_map(|ev| ev["id"].as_str().filter(|s| !s.is_empty()))
            .collect();
        if let Err(e) = append_seen(&seen_path, &ids) {
            // i18n-ignore: log
            eprintln!(
                "triggers/github: {repo}: cannot baseline trigger {} of crewmate {}: {e}; not polled for {} min",
                trigger.id,
                crewmate.id,
                BACKOFF.as_secs() / 60
            );
            hold(key);
        }
        return;
    }

    // The poll takes over from a webhook that stopped covering the repo: what happened up to the webhook's last
    // good delivery it already handled.
    let cutoff = if resuming {
        hub.crew.github_app.hook_state().last_ok
    } else {
        0
    };
    let mut undecided = false;
    // Oldest first, so tasks start in the order things happened.
    for ev in feed.iter().rev() {
        let Some(id) = ev["id"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        if seen.contains(id) {
            continue;
        }
        let fired = if created_at(ev).is_some_and(|t| t <= cutoff) {
            None
        } else {
            match decide(api, repo, ev, &cfg).await {
                Ok(f) => f,
                // Could not tell who wrote it (or what it says): not decided, so not seen. Asked again.
                Err(e) => {
                    // i18n-ignore: log
                    eprintln!(
                        "triggers/github: {} {id} in {repo} for crewmate {}: {e}; will retry",
                        ev["type"].as_str().unwrap_or("?"),
                        crewmate.id
                    );
                    undecided = true;
                    continue;
                }
            }
        };
        // Seen first, whether it matched or not: a task is only made for an event that cannot come
        // back. If the record cannot be written nothing fires, and the trigger rests for a while.
        if let Err(e) = append_seen(&seen_path, &[id]) {
            // i18n-ignore: log
            eprintln!(
                "triggers/github: {repo}: cannot record event {id} for crewmate {}: {e}; not fired, not polled for {} min",
                crewmate.id,
                BACKOFF.as_secs() / 60
            );
            hold(key);
            return;
        }
        seen.insert(id.to_string());
        if let Some(mut fired) = fired {
            if let Some(origin) = &mut fired.origin {
                origin.requester = actor_of(ev);
            }
            super::fire(
                hub,
                &crewmate.id,
                &trigger.id,
                &trigger.prompt,
                super::Event::Github {
                    text: &fired.event,
                    origin: fired.origin,
                },
            );
        }
    }
    // Back to normal polling once the whole feed was dealt with.
    if resuming && !undecided {
        let _ = std::fs::remove_file(&hooked);
    }
}

/// When a feed event happened, in unix seconds.
fn created_at(ev: &serde_json::Value) -> Option<i64> {
    Some(
        ev["created_at"]
            .as_str()?
            .parse::<jiff::Timestamp>()
            .ok()?
            .as_second(),
    )
}

/// The account that set an event off: the feed's `actor`.
pub(super) fn actor_of(ev: &serde_json::Value) -> Option<crate::crew::store::GithubUser> {
    Some(crate::crew::store::GithubUser {
        login: ev["actor"]["login"]
            .as_str()
            .filter(|l| !l.is_empty())?
            .to_string(),
        id: ev["actor"]["id"].as_u64()?,
    })
}

fn load_seen(path: &std::path::Path) -> HashSet<String> {
    crate::crew::store::jsonl::read_lines(path)
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Append `ids` to the seen file and flush them to disk. The error is the caller's to act on:
/// a record that is not durable means the event is not yet safe to fire. A torn final line is ended
/// first (#293), and the file is cut back to its newest ids once it passes [`SEEN_MAX_BYTES`]: the
/// events API only lists recent events, so an older id can never come back.
fn append_seen(path: &std::path::Path, ids: &[&str]) -> std::io::Result<()> {
    // Created even for no ids: an empty feed's baseline is the file existing (see `is_new`).
    crate::crew::store::jsonl::append_lines(path, ids, true)?;
    let _ = crate::crew::store::jsonl::trim_to_tail(path, SEEN_MAX_BYTES, SEEN_MAX_BYTES / 2);
    Ok(())
}

#[cfg(test)]
mod seen_tests {
    use super::*;
    use std::io::Write;

    // #293: a bad byte stopped `load_seen` there, so every later id was "new" again and its event fired
    // a second time; a torn last id (no newline) glued the next id onto it.
    #[test]
    fn the_seen_file_survives_invalid_utf8_and_a_torn_tail() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("trig_seen.jsonl");
        append_seen(&p, &["100"]).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&p)
            .unwrap()
            .write_all(b"\xff\xfe\n20")
            .unwrap(); // a bad line, then a torn id
        append_seen(&p, &["300"]).unwrap();
        append_seen(&p, &["400"]).unwrap();

        let seen = load_seen(&p);
        for id in ["100", "300", "400"] {
            assert!(seen.contains(id), "{id} lost: {seen:?}");
        }
    }

    // The baseline of a trigger whose feed is empty is the file existing: an empty batch must create it.
    #[test]
    fn an_empty_batch_still_creates_the_seen_file() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("trig_seen.jsonl");
        append_seen(&p, &[]).unwrap();
        assert!(p.exists());
        assert!(load_seen(&p).is_empty());
    }

    // #295: the file is capped, newest ids kept (the events API only lists recent ones).
    #[test]
    fn the_seen_file_is_capped_and_keeps_the_newest_ids() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("trig_seen.jsonl");
        let total = 60_000u64;
        // In batches, as the poll records them: each call fsyncs (record before firing, #294), and 60 000
        // single-id calls took past nextest's 180 s on Windows, where an fsync costs milliseconds. 60
        // batches still cross the cap many times over.
        let ids: Vec<String> = (0..total)
            .map(|n| format!("{:012}", 10_000_000_000 + n))
            .collect();
        for batch in ids.chunks(1000) {
            let batch: Vec<&str> = batch.iter().map(String::as_str).collect();
            append_seen(&p, &batch).unwrap();
        }
        let len = std::fs::metadata(&p).unwrap().len();
        assert!(
            len <= super::SEEN_MAX_BYTES + 32,
            "seen file grew to {len} bytes"
        );
        let seen = load_seen(&p);
        assert!(seen.contains(&format!("{:012}", 10_000_000_000 + total - 1)));
        assert!(seen.contains(&format!("{:012}", 10_000_000_000 + total - 1000)));
        assert!(!seen.contains(&format!("{:012}", 10_000_000_000u64)));
    }
}
