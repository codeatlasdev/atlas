//! Cron scheduler for bot routines.
//!
//! Ticks every 30 s; enqueues `source="routine"` tasks for enabled routines whose
//! next fire since `last_run` is ≤ now. Persists `last_run` after enqueue.

use std::sync::Weak;

use crate::server::Hub;

/// Validate a cron expression using `croner`.
pub(crate) fn validate_cron(s: &str) -> Result<(), String> {
    croner::Cron::new(s)
        .parse()
        .map(|_| ())
        .map_err(|e| format!("invalid cron '{s}': {e}"))
}

/// Spawn the routine scheduler. Runs forever; call once from `BotHub::start`.
pub(crate) fn start(hub_weak: Weak<Hub>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(hub) = hub_weak.upgrade() else { break };
            let now_s = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            tick(&hub, now_s);
        }
    });
}

pub(crate) fn tick(hub: &std::sync::Arc<Hub>, now_s: u64) {
    let bots = hub.bots.store.list_bots();
    for mut bot in bots {
        let mut fired: Vec<(usize, crate::bots::store::Task)> = Vec::new();
        for (i, routine) in bot.routines.iter().enumerate() {
            if !routine.enabled {
                continue;
            }
            let Ok(cron) = croner::Cron::new(&routine.cron).parse() else {
                continue;
            };
            // Find the next fire after last_run (or epoch if never run).
            let since = routine.last_run.unwrap_or(0);
            let since_dt =
                chrono::DateTime::from_timestamp(since as i64, 0).unwrap_or_else(chrono::Utc::now);
            let should_fire = cron
                .find_next_occurrence(&since_dt, false)
                .ok()
                .map(|next| next.timestamp() as u64 <= now_s)
                .unwrap_or(false);

            if should_fire {
                let task = crate::bots::store::Task::new("routine", &routine.prompt);
                if hub.bots.store.append_task(&bot.id, &task).is_ok() {
                    fired.push((i, task));
                }
            }
        }
        let mut changed = false;
        for (i, task) in fired {
            bot.routines[i].last_run = Some(now_s);
            crate::bots::runner::enqueue(hub, &hub.bots.store, &bot, task);
            hub.broadcast(serde_json::json!({"ev": "bot_task", "bot_id": bot.id}).to_string());
            changed = true;
        }
        if changed {
            let _ = hub.bots.store.save_bot(&bot);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cron_fires_when_next_occurrence_le_now() {
        let cron = croner::Cron::new("* * * * *").parse().unwrap();
        let since_dt = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let now_s: u64 = 1_000_000;
        let should = cron
            .find_next_occurrence(&since_dt, false)
            .ok()
            .map(|next| next.timestamp() as u64 <= now_s)
            .unwrap_or(false);
        assert!(should, "cron should fire when now is far in the future");
    }

    #[test]
    fn cron_does_not_fire_when_just_ran() {
        let cron = croner::Cron::new("* * * * *").parse().unwrap();
        // last_run = now_s - 10s; next fire is ~50s in the future
        let now_s: u64 = 1_000_060;
        let since_dt = chrono::DateTime::from_timestamp(now_s as i64 - 10, 0).unwrap();
        let should = cron
            .find_next_occurrence(&since_dt, false)
            .ok()
            .map(|next| next.timestamp() as u64 <= now_s)
            .unwrap_or(false);
        assert!(!should, "cron should not fire again within the same minute");
    }

    #[test]
    fn validate_cron_accepts_and_rejects() {
        assert!(super::validate_cron("* * * * *").is_ok());
        assert!(super::validate_cron("not a cron").is_err());
    }
}
