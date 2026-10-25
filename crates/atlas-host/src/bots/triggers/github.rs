//! GitHub event polling for `Github` triggers.
//!
//! Polls `gh api repos/<repo>/events` every 2 minutes.
//! Deduplicates seen event IDs via `<bot_dir>/<trigger_id>_seen.jsonl`.
//! If `gh` is not installed or returns an error, logs to stderr and skips.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::bots::store::{Trigger, TriggerKind};
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
    let bots = hub.bots.store.list_bots();
    for bot in bots {
        let bot_dir = hub.bots.store.bot_dir(&bot.id);
        for trigger in &bot.triggers {
            if !trigger.enabled {
                continue;
            }
            if let TriggerKind::Github { repo, events } = &trigger.kind {
                poll_trigger(hub, &bot.id, &bot_dir, trigger, repo, events).await;
            }
        }
    }
}

async fn poll_trigger(
    hub: &Arc<Hub>,
    bot_id: &str,
    bot_dir: &std::path::Path,
    trigger: &Trigger,
    repo: &str,
    wanted: &[String],
) {
    let seen_path = bot_dir.join(format!("{}_seen.jsonl", trigger.id));
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
        let title = ev["payload"]["pull_request"]["title"]
            .as_str()
            .or_else(|| ev["payload"]["issue"]["title"].as_str())
            .or_else(|| ev["payload"]["release"]["name"].as_str())
            .or_else(|| ev["payload"]["check_run"]["name"].as_str())
            .unwrap_or("(no title)");

        let matched = wanted
            .iter()
            .any(|w| event_matches(w, ev_type, action, &ev));
        if matched {
            let event_summary = format!("{ev_type} in {repo}: {title}");
            super::fire(hub, bot_id, &trigger.id, &trigger.prompt, &event_summary);
        }

        // Mark seen regardless of match, to avoid re-processing on next poll.
        seen.insert(id.clone());
        append_seen(&seen_path, &id);
    }
}

fn event_matches(wanted: &str, ev_type: &str, action: &str, ev: &serde_json::Value) -> bool {
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
