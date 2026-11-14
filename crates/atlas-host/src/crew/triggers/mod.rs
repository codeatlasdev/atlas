//! Trigger system: 7 trigger kinds that replace and extend the routine scheduler.
//!
//! | Kind       | Fires when …                                          |
//! |------------|-------------------------------------------------------|
//! | Schedule   | cron expression fires (via `croner`)                  |
//! | Interval   | a fixed number of seconds elapses (≥ 60)              |
//! | Webhook    | `POST /crew/<id>/hook/<token>` is received             |
//! | Github     | a matching GitHub repository event appears            |
//! | File       | a watched path changes on disk                        |
//! | Crewmate        | another crewmate's task finishes with a matching state     |
//! | Service    | a service log indicates `failed` or `recovered`       |
//!
//! # Integration points (wired up by the main agent)
//! - `CrewHub::start` calls `triggers::start(hub_weak)`.
//! - `runner::finish_task` calls `triggers::on_task_finished`.
//! - `hook_server::route` calls `triggers::webhook::handle` for `/crew/…/hook/…`.

mod file_watch;
mod github;
mod service;
pub(crate) mod webhook;

use std::sync::{Arc, Weak};

use crate::crew::now_s;
use crate::crew::store::{Task, TaskState, Trigger, TriggerKind};
use crate::server::Hub;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Validate a trigger definition. Returns `Err` with a human-readable message
/// for any constraint violation.
pub(crate) fn validate_trigger(t: &Trigger) -> Result<(), String> {
    match &t.kind {
        TriggerKind::Schedule { cron } => crate::crew::schedule::validate_cron(cron)?,
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
        TriggerKind::Crewmate { from_crewmate, on } => {
            if from_crewmate.is_empty() {
                return Err("from_crewmate must not be empty".into());
            }
            if on != "done" && on != "failed" {
                return Err("crewmate trigger on must be 'done' or 'failed'".into());
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
    let crewmates = hub.crew.store.list_crewmates();
    for crewmate in crewmates {
        // Collect trigger indices that should fire this tick.
        let indices: Vec<usize> = crewmate
            .triggers
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if !t.enabled {
                    return None;
                }
                let fires = match &t.kind {
                    TriggerKind::Schedule { cron } => {
                        should_fire_cron(cron, t.last_fired, t.tz.as_deref(), now_s)
                    }
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
        for i in indices {
            let prompt = render_prompt(&crewmate.triggers[i].prompt, "");
            let task = Task::new("trigger", &prompt);
            let trigger_id = crewmate.triggers[i].id.clone();
            let crewmate_id = crewmate.id.clone();
            if hub.crew.store.append_task(&crewmate_id, &task).is_ok() {
                // Reload the crewmate before updating last_fired to avoid overwriting
                // a concurrent crewmate_triggers_set (tick_schedule stale-snapshot bug).
                if let Ok(mut fresh) = hub.crew.store.load_crewmate(&crewmate_id) {
                    if let Some(t) = fresh.triggers.iter_mut().find(|t| t.id == trigger_id) {
                        t.last_fired = Some(now_s);
                    }
                    let _ = hub.crew.store.save_crewmate(&fresh);
                    crate::crew::runner::enqueue(hub, &hub.crew.store, &fresh, task.clone());
                } else {
                    crate::crew::runner::enqueue(hub, &hub.crew.store, &crewmate, task.clone());
                }
                hub.broadcast(crate::crew::events::task(&crewmate_id, None));
            }
        }
    }
}

/// Called by the runner when a task completes. Fires any `Crewmate` triggers that
/// watch `finished_crewmate_id` for `state`.
///
/// The integration call in `runner.rs` is added by the main agent.
pub(crate) fn on_task_finished(
    hub: &Hub,
    finished_crewmate_id: &str,
    _task_id: &str,
    state: TaskState,
) {
    let on_str = match state {
        TaskState::Done => "done",
        TaskState::Failed => "failed",
        _ => return,
    };
    let finished_name = hub
        .crew
        .store
        .load_crewmate(finished_crewmate_id)
        .ok()
        .map(|b| b.name)
        .unwrap_or_else(|| finished_crewmate_id.to_string());
    let event = format!("Crewmate {finished_name} {on_str}");

    // Need an Arc to call enqueue.
    let Some(hub_arc) = hub.crew.hub_arc() else {
        return;
    };

    let crewmates = hub.crew.store.list_crewmates();
    for mut crewmate in crewmates {
        let indices: Vec<usize> = crewmate
            .triggers
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if !t.enabled {
                    return None;
                }
                matches!(
                    &t.kind,
                    TriggerKind::Crewmate { from_crewmate, on }
                    if from_crewmate == finished_crewmate_id && on == on_str
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
            let prompt = render_prompt(&crewmate.triggers[i].prompt, &event);
            let task = Task::new("trigger", &prompt);
            if hub.crew.store.append_task(&crewmate.id, &task).is_ok() {
                crewmate.triggers[i].last_fired = Some(now);
                crate::crew::runner::enqueue(&hub_arc, &hub.crew.store, &crewmate, task);
                hub.broadcast(crate::crew::events::task(&crewmate.id, None));
                changed = true;
            }
        }
        if changed {
            let _ = hub.crew.store.save_crewmate(&crewmate);
        }
    }
}

/// Spawn all background trigger watchers. Call once from `CrewHub::start`.
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

/// Evaluate a cron expression in an optional IANA timezone; returns true when the
/// next occurrence since `last_fired` (or epoch) has passed `now_s`.
fn should_fire_cron(
    cron_expr: &str,
    last_fired: Option<u64>,
    tz: Option<&str>,
    now_s: u64,
) -> bool {
    let Ok(cron) = croner::Cron::new(cron_expr).parse() else {
        return false;
    };
    let since = last_fired.unwrap_or(0);
    if let Some(tz_str) = tz
        && let Ok(zone) = tz_str.parse::<chrono_tz::Tz>()
    {
        let since_dt = chrono::DateTime::from_timestamp(since as i64, 0)
            .unwrap_or_else(chrono::Utc::now)
            .with_timezone(&zone);
        return cron
            .find_next_occurrence(&since_dt, false)
            .ok()
            .map(|next| next.timestamp() as u64 <= now_s)
            .unwrap_or(false);
    }
    let since_dt =
        chrono::DateTime::from_timestamp(since as i64, 0).unwrap_or_else(chrono::Utc::now);
    cron.find_next_occurrence(&since_dt, false)
        .ok()
        .map(|next| next.timestamp() as u64 <= now_s)
        .unwrap_or(false)
}

/// Compute the unix-seconds timestamp of the next fire for a `Schedule` or `Interval`
/// trigger, or `None` for event-driven kinds. Used by `crewmate_list` to populate `next_fire`.
pub(crate) fn compute_next_fire(
    kind: &TriggerKind,
    last_fired: Option<u64>,
    tz: Option<&str>,
    now_s: u64,
) -> Option<u64> {
    match kind {
        TriggerKind::Schedule { cron } => {
            let Ok(c) = croner::Cron::new(cron).parse() else {
                return None;
            };
            let since = last_fired.unwrap_or(now_s);
            if let Some(tz_str) = tz
                && let Ok(zone) = tz_str.parse::<chrono_tz::Tz>()
            {
                let since_dt = chrono::DateTime::from_timestamp(since as i64, 0)
                    .unwrap_or_else(chrono::Utc::now)
                    .with_timezone(&zone);
                return c
                    .find_next_occurrence(&since_dt, false)
                    .ok()
                    .map(|dt| dt.timestamp() as u64);
            }
            let since_dt =
                chrono::DateTime::from_timestamp(since as i64, 0).unwrap_or_else(chrono::Utc::now);
            c.find_next_occurrence(&since_dt, false)
                .ok()
                .map(|dt| dt.timestamp() as u64)
        }
        TriggerKind::Interval { every_secs } => {
            let last = last_fired.unwrap_or(now_s);
            Some(last + every_secs)
        }
        _ => None,
    }
}

/// Enqueue a trigger-fired task and update `last_fired` on the trigger.
///
/// Loads a fresh copy of the crewmate, so callers do not need to hold a mutable
/// reference. No-op if the crewmate no longer exists or the task cannot be written.
pub(crate) fn fire(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    trigger_id: &str,
    prompt_template: &str,
    event: &str,
) {
    let Ok(mut crewmate) = hub.crew.store.load_crewmate(crewmate_id) else {
        return;
    };
    let prompt = render_prompt(prompt_template, event);
    let task = Task::new("trigger", &prompt);
    if hub.crew.store.append_task(crewmate_id, &task).is_err() {
        return;
    }
    if let Some(t) = crewmate.triggers.iter_mut().find(|t| t.id == trigger_id) {
        t.last_fired = Some(now_s());
    }
    let _ = hub.crew.store.save_crewmate(&crewmate);
    crate::crew::runner::enqueue(hub, &hub.crew.store, &crewmate, task);
    hub.broadcast(crate::crew::events::task(crewmate_id, None));
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::store::{Trigger, TriggerKind};

    fn trigger(kind: TriggerKind) -> Trigger {
        Trigger {
            id: "t1".into(),
            kind,
            enabled: true,
            prompt: "do {{event}} now".into(),
            last_fired: None,
            tz: None,
            next_fire: None,
        }
    }

    // ── Schedule ─────────────────────────────────────────────────────────────

    #[test]
    fn schedule_fires_when_due() {
        // last_fired = None (never ran); now is far in the future → should fire.
        assert!(
            should_fire_cron("* * * * *", None, None, 1_000_000),
            "expected cron to fire when last_fired is None and now is epoch+~11 days"
        );
    }

    #[test]
    fn schedule_does_not_fire_when_recent() {
        // last_fired 10 s ago inside the same cron minute → next fire is ~50 s away.
        let now_s: u64 = 1_000_060;
        assert!(
            !should_fire_cron("* * * * *", Some(now_s - 10), None, now_s),
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
        let (crewmate_id, token) =
            webhook::parse_webhook_path("/crew/my-crewmate/hook/abc123").unwrap();
        assert_eq!(crewmate_id, "my-crewmate");
        assert_eq!(token, "abc123");
    }

    #[test]
    fn webhook_rejects_wrong_token() {
        // Empty token.
        assert!(webhook::parse_webhook_path("/crew/my-crewmate/hook/").is_none());
        // Token with an extra path segment (slash inside token).
        assert!(webhook::parse_webhook_path("/crew/my-crewmate/hook/abc/extra").is_none());
        // Completely wrong prefix.
        assert!(webhook::parse_webhook_path("/hook/abc123").is_none());
        // Empty crewmate id.
        assert!(webhook::parse_webhook_path("/crew//hook/abc123").is_none());
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

    // ── Timezone-aware schedule ────────────────────────────────────────────────

    #[test]
    fn new_schedule_created_at_14h_does_not_fire_on_next_tick() {
        // Contract bug fix: a new schedule trigger created at 14:00 with cron "0 9 * * *"
        // must NOT fire on the next 30 s tick — baseline last_fired = creation time.
        //
        // 2024-10-30 14:00:00 UTC (a Wednesday)
        let created_at: u64 = 1730296800;
        let tick = created_at + 30;
        assert!(
            !should_fire_cron("0 9 * * *", Some(created_at), None, tick),
            "new 09:00 schedule created at 14:00 must not fire 30s later"
        );
    }

    #[test]
    fn cron_09_sao_paulo_fires_at_12_utc_not_09_utc() {
        // Contract: cron is evaluated in the user's timezone.
        // "0 9 * * *" in America/Sao_Paulo (UTC-3) fires at 12:00 UTC.
        //
        // Use a fixed Tuesday: 2024-10-29.
        // Yesterday's 09:00 SP = yesterday 12:00 UTC = 1730203200 (2024-10-29 12:00 UTC).
        let yesterday_12_utc: u64 = 1730203200;
        let today_12_utc = yesterday_12_utc + 86400; // 2024-10-30 12:00 UTC = 09:00 SP
        let today_09_utc = today_12_utc - 3 * 3600; // 2024-10-30 09:00 UTC = 06:00 SP

        // Should fire at 12:00 UTC (= 09:00 SP).
        assert!(
            should_fire_cron(
                "0 9 * * *",
                Some(yesterday_12_utc),
                Some("America/Sao_Paulo"),
                today_12_utc + 30
            ),
            "cron '0 9 * * *' in America/Sao_Paulo should fire at 12:00 UTC"
        );
        // Must NOT fire at 09:00 UTC (= 06:00 SP — too early).
        assert!(
            !should_fire_cron(
                "0 9 * * *",
                Some(yesterday_12_utc),
                Some("America/Sao_Paulo"),
                today_09_utc + 30
            ),
            "cron '0 9 * * *' in America/Sao_Paulo must NOT fire at 09:00 UTC"
        );
    }
}
