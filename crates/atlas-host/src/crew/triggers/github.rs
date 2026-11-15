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
use std::io::{BufRead, Write};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::github_events::{Cfg, decide, handle_for};
use crate::crew::github::GithubApi;
use crate::crew::store::{Crewmate, Trigger, TriggerKind};
use crate::server::Hub;

const POLL_EVERY: Duration = Duration::from_secs(120);

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
    let key = format!("{}:{}", crewmate.id, trigger.id);
    if held(&key) {
        return;
    }
    let seen_path = hub
        .crew
        .store
        .crewmate_dir(&crewmate.id)
        .join(format!("{}_seen.jsonl", trigger.id));
    let is_new = !seen_path.exists();
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

    // A new trigger starts at the head of the feed: what is already there is history.
    if is_new {
        let ids: Vec<&str> = feed
            .iter()
            .filter_map(|ev| ev["id"].as_str().filter(|s| !s.is_empty()))
            .collect();
        if let Err(e) = append_seen(&seen_path, &ids) {
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

    // Oldest first, so tasks start in the order things happened.
    for ev in feed.iter().rev() {
        let Some(id) = ev["id"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        if seen.contains(id) {
            continue;
        }
        let fired = match decide(api, repo, ev, &cfg).await {
            Ok(f) => f,
            // Could not tell who wrote it (or what it says): not decided, so not seen. Asked again.
            Err(e) => {
                eprintln!(
                    "triggers/github: {} {id} in {repo} for crewmate {}: {e}; will retry",
                    ev["type"].as_str().unwrap_or("?"),
                    crewmate.id
                );
                continue;
            }
        };
        // Seen first, whether it matched or not: a task is only made for an event that cannot come
        // back. If the record cannot be written nothing fires, and the trigger rests for a while.
        if let Err(e) = append_seen(&seen_path, &[id]) {
            eprintln!(
                "triggers/github: {repo}: cannot record event {id} for crewmate {}: {e}; not fired, not polled for {} min",
                crewmate.id,
                BACKOFF.as_secs() / 60
            );
            hold(key);
            return;
        }
        seen.insert(id.to_string());
        if let Some(fired) = fired {
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
}

fn load_seen(path: &std::path::Path) -> HashSet<String> {
    let Ok(f) = std::fs::File::open(path) else {
        return HashSet::new();
    };
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Append `ids` to the seen file and flush them to disk. The error is the caller's to act on:
/// a record that is not durable means the event is not yet safe to fire.
fn append_seen(path: &std::path::Path, ids: &[&str]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for id in ids {
        writeln!(f, "{id}")?;
    }
    f.sync_all()
}
