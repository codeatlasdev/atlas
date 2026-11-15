//! GitHub event polling for `Github` triggers.
//!
//! Polls the repo's events every 2 minutes through the crew's [`GithubApi`] (`gh api`). Deduplicates
//! seen event ids via `<crewmate_dir>/<trigger_id>_seen.jsonl`. What an event means (and whether
//! its author may start a task) is decided in `github_events`; this file only walks the feed.
//! If `gh` is missing or errors, it logs to stderr and tries again next time.

use std::collections::HashSet;
use std::sync::{Arc, Weak};
use std::time::Duration;

use super::github_events::{Cfg, decide, handle_for};
use crate::crew::github::GithubApi;
use crate::crew::store::{Crewmate, Trigger, TriggerKind};
use crate::server::Hub;

const POLL_EVERY: Duration = Duration::from_secs(120);
/// The seen file is cut back to its newest ids once it passes this (the events API lists recent events only).
pub(super) const SEEN_MAX_BYTES: u64 = 256 * 1024;

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
    let TriggerKind::Github {
        repo,
        events,
        handle,
        assignee,
        authors,
        ..
    } = &trigger.kind
    else {
        return;
    };
    let seen_path = hub
        .crew
        .store
        .crewmate_dir(&crewmate.id)
        .join(format!("{}_seen.jsonl", trigger.id));
    let mut seen = load_seen(&seen_path);

    let feed = match api.events(repo).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("triggers/github: {repo}: {e}");
            return;
        }
    };
    // Who the crew writes as: its own comments never start a task. Unknown = nothing is polled
    // (an event could be our own comment), and it is asked again next time.
    let own_login = match api.login().await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("triggers/github: cannot tell the crew's own GitHub login: {e}");
            return;
        }
    };
    let cfg = Cfg {
        events: events.clone(),
        authors: authors.clone(),
        handle: handle_for(handle.as_deref(), &crewmate.name),
        assignee: assignee.clone(),
        own_login,
    };

    // Oldest first, so tasks start in the order things happened.
    for ev in feed.iter().rev() {
        let Some(id) = ev["id"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        if seen.contains(id) {
            continue;
        }
        match decide(api, repo, ev, &cfg).await {
            Ok(Some(fired)) => super::fire(
                hub,
                &crewmate.id,
                &trigger.id,
                &trigger.prompt,
                super::Event::Github {
                    text: &fired.event,
                    origin: fired.origin,
                },
            ),
            Ok(None) => {}
            // Could not tell who wrote it (or what it says): not decided, so not seen. Asked again.
            Err(e) => {
                eprintln!(
                    "triggers/github: {} {id} in {repo} for crewmate {}: {e}; will retry",
                    ev["type"].as_str().unwrap_or("?"),
                    crewmate.id
                );
                continue;
            }
        }
        // Mark seen regardless of match, to avoid re-processing on next poll.
        seen.insert(id.to_string());
        append_seen(&seen_path, id);
    }
}

fn load_seen(path: &std::path::Path) -> HashSet<String> {
    crate::crew::store::jsonl::read_lines(path)
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn append_seen(path: &std::path::Path, id: &str) {
    use crate::crew::store::jsonl;
    if jsonl::append_line(path, id, false).is_ok() {
        // Newest ids kept: the events API only lists recent events, so older ids can never come back.
        let _ = jsonl::trim_to_tail(path, SEEN_MAX_BYTES, SEEN_MAX_BYTES / 2);
    }
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
        append_seen(&p, "100");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&p)
            .unwrap()
            .write_all(b"\xff\xfe\n20")
            .unwrap(); // a bad line, then a torn id
        append_seen(&p, "300");
        append_seen(&p, "400");

        let seen = load_seen(&p);
        for id in ["100", "300", "400"] {
            assert!(seen.contains(id), "{id} lost: {seen:?}");
        }
    }

    // #295: the file is capped, newest ids kept (the events API only lists recent ones).
    #[test]
    fn the_seen_file_is_capped_and_keeps_the_newest_ids() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("trig_seen.jsonl");
        let total = 60_000u64;
        for n in 0..total {
            append_seen(&p, &format!("{:012}", 10_000_000_000 + n));
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
