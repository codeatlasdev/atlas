//! GitHub event polling for `Github` triggers.
//!
//! Polls `gh api repos/<repo>/events` every 2 minutes.
//! Deduplicates seen event IDs via `<crewmate_dir>/<trigger_id>_seen.jsonl`.
//! If `gh` is not installed or returns an error, logs to stderr and skips.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::crew::store::{Trigger, TriggerKind};
use crate::server::Hub;

pub(super) async fn run(hub_weak: Weak<Hub>) {
    let mut interval = tokio::time::interval(Duration::from_secs(120));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(hub) = hub_weak.upgrade() else { break };
        poll_all(&hub).await;
    }
}

async fn poll_all(hub: &Arc<Hub>) {
    // Paused: not even polled, so nothing is fired and nothing is marked seen behind the user's back.
    if hub.crew.is_paused() {
        return;
    }
    let crewmates = hub.crew.store.list_crewmates();
    for crewmate in crewmates {
        let crewmate_dir = hub.crew.store.crewmate_dir(&crewmate.id);
        for trigger in &crewmate.triggers {
            if !trigger.enabled {
                continue;
            }
            if let TriggerKind::Github {
                repo,
                events,
                authors,
                ..
            } = &trigger.kind
            {
                poll_trigger(
                    hub,
                    &crewmate.id,
                    &crewmate_dir,
                    trigger,
                    repo,
                    events,
                    authors,
                )
                .await;
            }
        }
    }
}

async fn poll_trigger(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    crewmate_dir: &std::path::Path,
    trigger: &Trigger,
    repo: &str,
    wanted: &[String],
    authors: &[String],
) {
    let seen_path = crewmate_dir.join(format!("{}_seen.jsonl", trigger.id));
    let mut seen = load_seen(&seen_path);

    // `--jq '.[]'` emits one JSON object per line.
    let output = tokio::process::Command::new("gh")
        .args(["api", &format!("repos/{repo}/events"), "--jq", ".[]"])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            eprintln!(
                "triggers/github: gh api failed for {repo}: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            return;
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("triggers/github: gh CLI not found; skipping github triggers");
            } else {
                eprintln!("triggers/github: gh error: {e}");
            }
            return;
        }
    };

    let text = String::from_utf8_lossy(&output);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let id = match ev["id"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        if seen.contains(&id) {
            continue;
        }

        let ev_type = ev["type"].as_str().unwrap_or("");
        let action = ev["payload"]["action"].as_str().unwrap_or("");

        let wanted_event = wanted
            .iter()
            .any(|w| event_matches(w, ev_type, action, &ev));
        if wanted_event {
            // Only events from accepted authors fire; the text reaches the agent wrapped as untrusted.
            match super::github_event_prompt(&ev, repo, wanted, authors) {
                Some(event) => super::fire(
                    hub,
                    crewmate_id,
                    &trigger.id,
                    &trigger.prompt,
                    super::Event::Wrapped(&event),
                ),
                None => eprintln!(
                    "triggers/github: {ev_type} {id} in {repo} skipped for crewmate {crewmate_id}: its author is not in the trigger's authors"
                ),
            }
        }

        // Mark seen regardless of match, to avoid re-processing on next poll.
        seen.insert(id.clone());
        append_seen(&seen_path, &id);
    }
}

pub(super) fn event_matches(
    wanted: &str,
    ev_type: &str,
    action: &str,
    ev: &serde_json::Value,
) -> bool {
    match wanted {
        "pr_opened" => ev_type == "PullRequestEvent" && action == "opened",
        "pr_review_requested" => ev_type == "PullRequestReviewRequestedEvent",
        "issue_opened" => ev_type == "IssuesEvent" && action == "opened",
        "ci_failed" => {
            ev_type == "CheckRunEvent"
                && ev["payload"]["check_run"]["conclusion"].as_str() == Some("failure")
        }
        "release" => ev_type == "ReleaseEvent",
        _ => false,
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

fn append_seen(path: &std::path::Path, id: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{id}");
    }
}
