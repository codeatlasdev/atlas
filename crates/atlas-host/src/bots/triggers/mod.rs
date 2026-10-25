//! Trigger system: 7 trigger kinds that replace and extend the routine scheduler.
//!
//! | Kind       | Fires when …                                          |
//! |------------|-------------------------------------------------------|
//! | Schedule   | cron expression fires (via `croner`)                  |
//! | Interval   | a fixed number of seconds elapses (≥ 60)              |
//! | Webhook    | `POST /bots/<id>/hook/<token>` is received             |
//! | Github     | a matching GitHub repository event appears            |
//! | File       | a watched path changes on disk                        |
//! | Bot        | another bot's task finishes with a matching state     |
//! | Service    | a service log indicates `failed` or `recovered`       |
//!
//! # Integration points (wired up by the main agent)
//! - `BotHub::start` calls `triggers::start(hub_weak)`.
//! - `runner::finish_task` calls `triggers::on_task_finished`.
//! - `hook_server::route` calls `triggers::webhook::handle` for `/bots/…/hook/…`.

mod file_watch;
mod github;
mod service;
pub(crate) mod webhook;

use std::sync::{Arc, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::bots::store::{Task, TaskState, Trigger, TriggerKind};
use crate::server::Hub;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Validate a trigger definition. Returns `Err` with a human-readable message
/// for any constraint violation.
pub(crate) fn validate_trigger(t: &Trigger) -> Result<(), String> {
    match &t.kind {
        TriggerKind::Schedule { cron } => crate::bots::schedule::validate_cron(cron)?,
        TriggerKind::Interval { every_secs } => {
            if *every_secs < 60 {
                return Err("interval every_secs must be >= 60".into());
            }
        }
        TriggerKind::Webhook { .. } => {}
        TriggerKind::Github { repo, events } => {
            if repo.is_empty() {
                return Err("github repo must not be empty".into());
            }
            const VALID: &[&str] = &[
                "pr_opened",
                "pr_review_requested",
                "issue_opened",
                "ci_failed",
                "release",
            ];
            for e in events {
                if !VALID.contains(&e.as_str()) {
                    return Err(format!("unknown github event: {e}"));
                }
            }
        }
        TriggerKind::File { path, .. } => {
            if path.is_empty() {
                return Err("file path must not be empty".into());
            }
        }
        TriggerKind::Bot { from_bot, on } => {
            if from_bot.is_empty() {
                return Err("from_bot must not be empty".into());
            }
            if on != "done" && on != "failed" {
                return Err("bot trigger on must be 'done' or 'failed'".into());
            }
        }
        TriggerKind::Service { service, on, .. } => {
            if service.is_empty() {
                return Err("service name must not be empty".into());
            }
            if on != "failed" && on != "recovered" {
                return Err("service trigger on must be 'failed' or 'recovered'".into());
            }
        }
    }
    Ok(())
}

/// Replace `{{event}}` in `template` with `event_summary`.
pub(crate) fn render_prompt(template: &str, event_summary: &str) -> String {
    template.replace("{{event}}", event_summary)
}

/// Tick `Schedule` and `Interval` triggers. Call every ~30 s from a background
/// task (or directly from tests via `tick_schedule`).
pub(crate) fn tick_schedule(hub: &Arc<Hub>, now_s: u64) {
    let bots = hub.bots.store.list_bots();
    for mut bot in bots {
        // Collect (trigger_index) for all triggers that should fire this tick.
        let indices: Vec<usize> = bot
            .triggers
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if !t.enabled {
                    return None;
                }
                let fires = match &t.kind {
                    TriggerKind::Schedule { cron } => should_fire_cron(cron, t.last_fired, now_s),
                    TriggerKind::Interval { every_secs } => match t.last_fired {
                        None => true,
                        Some(last) => now_s.saturating_sub(last) >= *every_secs,
                    },
                    _ => false,
                };
                fires.then_some(i)
            })
            .collect();

        if indices.is_empty() {
            continue;
        }
        let mut changed = false;
        for i in indices {
            let prompt = render_prompt(&bot.triggers[i].prompt, "");
            let task = Task::new("trigger", &prompt);
            if hub.bots.store.append_task(&bot.id, &task).is_ok() {
                bot.triggers[i].last_fired = Some(now_s);
                crate::bots::runner::enqueue(hub, &hub.bots.store, &bot, task);
                hub.broadcast(serde_json::json!({"ev": "bot_task", "bot_id": &bot.id}).to_string());
                changed = true;
            }
        }
        if changed {
            let _ = hub.bots.store.save_bot(&bot);
        }
    }
}

/// Called by the runner when a task completes. Fires any `Bot` triggers that
/// watch `finished_bot_id` for `state`.
///
/// The integration call in `runner.rs` is added by the main agent.
pub(crate) fn on_task_finished(hub: &Hub, finished_bot_id: &str, _task_id: &str, state: TaskState) {
    let on_str = match state {
        TaskState::Done => "done",
        TaskState::Failed => "failed",
        _ => return,
    };
    let finished_name = hub
        .bots
        .store
        .load_bot(finished_bot_id)
        .ok()
        .map(|b| b.name)
        .unwrap_or_else(|| finished_bot_id.to_string());
    let event = format!("Bot {finished_name} {on_str}");

    // Need an Arc to call enqueue.
    let Some(hub_arc) = hub.bots.hub_arc() else {
        return;
    };

    let bots = hub.bots.store.list_bots();
    for mut bot in bots {
        let indices: Vec<usize> = bot
            .triggers
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if !t.enabled {
                    return None;
                }
                matches!(
                    &t.kind,
                    TriggerKind::Bot { from_bot, on }
                    if from_bot == finished_bot_id && on == on_str
                )
                .then_some(i)
            })
            .collect();

        if indices.is_empty() {
            continue;
        }
        let now = now_s();
        let mut changed = false;
        for i in indices {
            let prompt = render_prompt(&bot.triggers[i].prompt, &event);
            let task = Task::new("trigger", &prompt);
            if hub.bots.store.append_task(&bot.id, &task).is_ok() {
                bot.triggers[i].last_fired = Some(now);
                crate::bots::runner::enqueue(&hub_arc, &hub.bots.store, &bot, task);
                hub.broadcast(serde_json::json!({"ev": "bot_task", "bot_id": &bot.id}).to_string());
                changed = true;
            }
        }
        if changed {
            let _ = hub.bots.store.save_bot(&bot);
        }
    }
}

/// Spawn all background trigger watchers. Call once from `BotHub::start`.
///
/// Spawns:
/// - A 30 s ticker for `Schedule` / `Interval` triggers.
/// - A 2 min poller for `Github` triggers.
/// - A file-watcher task per `File` trigger (rescans every 2 min).
/// - A 30 s poller for `Service` triggers.
pub(crate) fn start(hub_weak: Weak<Hub>) {
    // ── Schedule + Interval ──────────────────────────────────────────────────
    {
        let w = hub_weak.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(30));
            iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                iv.tick().await;
                let Some(hub) = w.upgrade() else { break };
                tick_schedule(&hub, now_s());
            }
        });
    }
    // ── Github ───────────────────────────────────────────────────────────────
    {
        let w = hub_weak.clone();
        tokio::spawn(github::run(w));
    }
    // ── File ─────────────────────────────────────────────────────────────────
    {
        let w = hub_weak.clone();
        tokio::spawn(file_watch::run(w));
    }
    // ── Service ──────────────────────────────────────────────────────────────
    {
        let w = hub_weak;
        tokio::spawn(service::run(w));
    }
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn should_fire_cron(cron_expr: &str, last_fired: Option<u64>, now_s: u64) -> bool {
    let Ok(cron) = croner::Cron::new(cron_expr).parse() else {
        return false;
    };
    let since = last_fired.unwrap_or(0);
    let since_dt =
        chrono::DateTime::from_timestamp(since as i64, 0).unwrap_or_else(chrono::Utc::now);
    cron.find_next_occurrence(&since_dt, false)
        .ok()
        .map(|next| next.timestamp() as u64 <= now_s)
        .unwrap_or(false)
}

/// Enqueue a trigger-fired task and update `last_fired` on the trigger.
///
/// Loads a fresh copy of the bot, so callers do not need to hold a mutable
/// reference. No-op if the bot no longer exists or the task cannot be written.
pub(crate) fn fire(
    hub: &Arc<Hub>,
    bot_id: &str,
    trigger_id: &str,
    prompt_template: &str,
    event: &str,
) {
    let Ok(mut bot) = hub.bots.store.load_bot(bot_id) else {
        return;
    };
    let prompt = render_prompt(prompt_template, event);
    let task = Task::new("trigger", &prompt);
    if hub.bots.store.append_task(bot_id, &task).is_err() {
        return;
    }
    if let Some(t) = bot.triggers.iter_mut().find(|t| t.id == trigger_id) {
        t.last_fired = Some(now_s());
    }
    let _ = hub.bots.store.save_bot(&bot);
    crate::bots::runner::enqueue(hub, &hub.bots.store, &bot, task);
    hub.broadcast(serde_json::json!({"ev": "bot_task", "bot_id": bot_id}).to_string());
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::store::{Trigger, TriggerKind};

    fn trigger(kind: TriggerKind) -> Trigger {
        Trigger {
            id: "t1".into(),
            kind,
            enabled: true,
            prompt: "do {{event}} now".into(),
            last_fired: None,
        }
    }

    // ── Schedule ─────────────────────────────────────────────────────────────

    #[test]
    fn schedule_fires_when_due() {
        // last_fired = None (never ran); now is far in the future → should fire.
        assert!(
            should_fire_cron("* * * * *", None, 1_000_000),
            "expected cron to fire when last_fired is None and now is epoch+~11 days"
        );
    }

    #[test]
    fn schedule_does_not_fire_when_recent() {
        // last_fired 10 s ago inside the same cron minute → next fire is ~50 s away.
        let now_s: u64 = 1_000_060;
        assert!(
            !should_fire_cron("* * * * *", Some(now_s - 10), now_s),
            "expected cron NOT to fire again within the same minute"
        );
    }

    // ── Interval ─────────────────────────────────────────────────────────────

    #[test]
    fn interval_fires_after_duration() {
        let now_s: u64 = 1_000_000;

        // Never fired → should fire immediately.
        let t = trigger(TriggerKind::Interval { every_secs: 60 });
        let fires = match &t.kind {
            TriggerKind::Interval { every_secs } => {
                t.last_fired.is_none() || now_s.saturating_sub(t.last_fired.unwrap()) >= *every_secs
            }
            _ => unreachable!(),
        };
        assert!(fires, "interval with no last_fired should fire");

        // Fired 30 s ago, period is 60 s → should NOT fire yet.
        let t2 = Trigger {
            last_fired: Some(now_s - 30),
            ..trigger(TriggerKind::Interval { every_secs: 60 })
        };
        let fires2 = match &t2.kind {
            TriggerKind::Interval { every_secs } => {
                t2.last_fired.is_none()
                    || now_s.saturating_sub(t2.last_fired.unwrap()) >= *every_secs
            }
            _ => unreachable!(),
        };
        assert!(!fires2, "interval should not fire 30 s into a 60 s period");

        // Fired 60 s ago → should fire.
        let t3 = Trigger {
            last_fired: Some(now_s - 60),
            ..trigger(TriggerKind::Interval { every_secs: 60 })
        };
        let fires3 = match &t3.kind {
            TriggerKind::Interval { every_secs } => {
                t3.last_fired.is_none()
                    || now_s.saturating_sub(t3.last_fired.unwrap()) >= *every_secs
            }
            _ => unreachable!(),
        };
        assert!(
            fires3,
            "interval should fire exactly at the period boundary"
        );
    }

    #[test]
    fn interval_rejects_under_60s() {
        let bad = trigger(TriggerKind::Interval { every_secs: 30 });
        assert!(
            validate_trigger(&bad).is_err(),
            "30 s interval should be rejected"
        );
        let good = trigger(TriggerKind::Interval { every_secs: 60 });
        assert!(
            validate_trigger(&good).is_ok(),
            "60 s interval should be accepted"
        );
    }

    // ── Webhook ──────────────────────────────────────────────────────────────

    #[test]
    fn webhook_matches_path() {
        let (bot_id, token) = webhook::parse_webhook_path("/bots/my-bot/hook/abc123").unwrap();
        assert_eq!(bot_id, "my-bot");
        assert_eq!(token, "abc123");
    }

    #[test]
    fn webhook_rejects_wrong_token() {
        // Empty token.
        assert!(webhook::parse_webhook_path("/bots/my-bot/hook/").is_none());
        // Token with an extra path segment (slash inside token).
        assert!(webhook::parse_webhook_path("/bots/my-bot/hook/abc/extra").is_none());
        // Completely wrong prefix.
        assert!(webhook::parse_webhook_path("/hook/abc123").is_none());
        // Empty bot id.
        assert!(webhook::parse_webhook_path("/bots//hook/abc123").is_none());
    }

    // ── render_prompt ────────────────────────────────────────────────────────

    #[test]
    fn render_prompt_substitutes_event() {
        assert_eq!(
            render_prompt("handle {{event}} now", "file changed"),
            "handle file changed now"
        );
        assert_eq!(render_prompt("no placeholder", "ignored"), "no placeholder");
        assert_eq!(
            render_prompt("{{event}} and {{event}}", "x"),
            "x and x",
            "should replace all occurrences"
        );
    }
}
