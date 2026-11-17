//! Cron scheduler for crewmate routines.
//!
//! Ticks every 30 s; enqueues `source="routine"` tasks for enabled routines whose
//! next fire since `last_run` is ≤ now. Persists `last_run` after enqueue.

use std::sync::Weak;

use croner::Cron;
use croner::parser::{CronParser, Seconds};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::server::Hub;

/// The one parser every schedule goes through: five fields (no seconds, no year), and the `0/15` step
/// spelling that older builds accepted (croner 4 wants `0-59/15` unless asked).
pub(crate) fn parse_cron(s: &str) -> Result<Cron, croner::errors::CronError> {
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .sloppy_ranges(true)
        .build()
        .parse(&question_marks_as_stars(s))
}

/// croner 2 read a `?` as `*` in any field, and so did the automations people saved with one (`? * * * *`,
/// `0 ?/2 * * *`); croner 4 allows `?` only in the day fields. The same schedule, spelled the way it parses:
/// a minute, hour or month field that is `?` or `?/n` becomes `*` or `*/n`. (Every other `?` spelling in those
/// fields, like `?-5` or `?,5`, never parsed in either, and stays an error.)
fn question_marks_as_stars(cron: &str) -> String {
    let mut fields: Vec<String> = cron.split_whitespace().map(str::to_string).collect();
    if fields.len() != 5 {
        return cron.to_string();
    }
    for i in [0, 1, 3] {
        if fields[i] == "?" || fields[i].starts_with("?/") {
            fields[i].replace_range(..1, "*");
        }
    }
    fields.join(" ")
}

/// Validate a cron expression using `croner`.
pub(crate) fn validate_cron(s: &str) -> Result<(), String> {
    parse_cron(s)
        .map(|_| ())
        .map_err(|e| format!("invalid cron '{s}': {e}"))
}

/// Validate an IANA time zone name of a trigger.
pub(crate) fn validate_tz(tz: &str) -> Result<(), String> {
    atlas_core::time::zone(tz)
        .map(|_| ())
        .ok_or_else(|| format!("unknown timezone: {tz}"))
}

/// The unix second of the next fire of `cron` strictly after `since_s`, on the wall clock of `tz` (an IANA
/// name; UTC when it is absent or unknown: hostd validates it on write, a file edited by hand must not crash).
/// A `since_s` no calendar holds counts as now.
pub(crate) fn next_fire_after(cron: &Cron, since_s: u64, tz: Option<&str>) -> Option<u64> {
    let zone = tz.and_then(atlas_core::time::zone).unwrap_or(TimeZone::UTC);
    let since = Timestamp::from_second(since_s as i64).unwrap_or_else(|_| Timestamp::now());
    let next = cron
        .find_next_occurrence(&since.to_zoned(zone), false)
        .ok()?;
    u64::try_from(next.timestamp().as_second()).ok()
}

/// Whether `cron` is due at `now_s`: its next fire after `last_s` (or the epoch) is not in the future.
pub(crate) fn is_due(cron: &Cron, last_s: Option<u64>, tz: Option<&str>, now_s: u64) -> bool {
    next_fire_after(cron, last_s.unwrap_or(0), tz).is_some_and(|next| next <= now_s)
}

/// Spawn the routine scheduler. Runs forever; call once from `CrewHub::start`.
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
    // Paused: a due routine is skipped, not queued for later.
    if hub.crew.is_paused() {
        return;
    }
    let crewmates = hub.crew.store.list_crewmates();
    for mut crewmate in crewmates {
        let mut fired: Vec<(usize, crate::crew::store::Task)> = Vec::new();
        for (i, routine) in crewmate.routines.iter().enumerate() {
            if !routine.enabled {
                continue;
            }
            let Ok(cron) = parse_cron(&routine.cron) else {
                continue;
            };
            // The next fire after last_run (or the epoch if never run) is due.
            let should_fire = is_due(&cron, routine.last_run, None, now_s);

            if should_fire {
                let task = crate::crew::store::Task::new("routine", &routine.prompt);
                match hub.crew.store.append_task(&crewmate.id, &task) {
                    Ok(()) => fired.push((i, task)),
                    Err(e) => eprintln!(
                        "atlas crew: crewmate {} task {}: routine {} due but the task cannot be stored: {e}",
                        crewmate.id, task.id, routine.id
                    ),
                }
            }
        }
        let mut changed = false;
        for (i, task) in fired {
            crewmate.routines[i].last_run = Some(now_s);
            crate::crew::runner::enqueue(hub, &hub.crew.store, &crewmate, task);
            hub.broadcast(super::events::task(&crewmate.id, None));
            changed = true;
        }
        if changed && let Err(e) = hub.crew.store.save_crewmate(&crewmate) {
            eprintln!(
                "atlas crew: crewmate {}: cannot save the routines' last_run: {e}",
                crewmate.id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_fires_when_next_occurrence_le_now() {
        let cron = parse_cron("* * * * *").unwrap();
        assert!(
            is_due(&cron, None, None, 1_000_000),
            "cron should fire when now is far in the future"
        );
    }

    #[test]
    fn cron_does_not_fire_when_just_ran() {
        let cron = parse_cron("* * * * *").unwrap();
        // last_run = now_s - 10s; next fire is ~50s in the future
        let now_s: u64 = 1_000_060;
        assert!(
            !is_due(&cron, Some(now_s - 10), None, now_s),
            "cron should not fire again within the same minute"
        );
    }

    #[test]
    fn validate_cron_accepts_and_rejects() {
        assert!(validate_cron("* * * * *").is_ok());
        assert!(validate_cron("not a cron").is_err());
    }
}
