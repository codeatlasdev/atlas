//! hostd's side of the per-agent memory cap (#296): what the cap is (`agent_memory_max`, or computed),
//! telling the spawn helper (which owns the cgroups), and what to report when there is none.
//!
//! The helper applies the cap; hostd only computes it and asks. The cap depends on two settings
//! (`agent_memory_max`, `max_agents`) and the machine, so it is pushed when either setting changes and
//! again before an agent starts: a helper that restarted forgot it and would run on its own default.

use serde_json::{Value, json};

use super::spawn::cgroup::{self, Status};
use super::store::CrewSettings;
use crate::server::Hub;

/// Smallest `agent_memory_max` accepted: below this not even a shell starts under the cap.
pub const MIN_EXPLICIT: u64 = 64 << 20;

/// `Err` for a value no agent could run under; `Ok(None)` for `0` (back to the computed default).
pub fn parse_setting(bytes: u64) -> Result<Option<u64>, String> {
    match bytes {
        0 => Ok(None),
        n if n < MIN_EXPLICIT => Err(format!(
            "agent_memory_max must be at least {} (or 0 for the computed default): an agent cannot run in {}",
            cgroup::human(MIN_EXPLICIT),
            cgroup::human(n)
        )),
        n => Ok(Some(n)),
    }
}

/// The cap agents get under `settings` on a machine with `mem_total` bytes.
pub fn cap_for(settings: &CrewSettings, mem_total: u64) -> u64 {
    cgroup::compute_cap(settings.agent_memory_max, mem_total, settings.max_agents)
}

/// This machine's memory, in bytes (`/proc/meminfo`); 0 where there is none (the cap is then the floor).
pub fn mem_total() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| cgroup::mem_total_of(&t))
        .unwrap_or(0)
}

/// Tell the helper the current cap and return what it reports. Without a helper (no isolation, a Mac,
/// a server installed before the cap) the answer is `available: false` and nothing is asked. Blocking:
/// one unix-socket call, so async callers use [`apply_async`].
pub(crate) fn apply(hub: &Hub) -> Status {
    let settings = hub.crew.store.load_crew_settings();
    push(hub.crew.store.isolation(), &settings)
}

/// [`apply`] off the async runtime.
pub(crate) async fn apply_async(hub: &Hub) -> Status {
    let setup = hub.crew.store.isolation().cloned();
    let settings = hub.crew.store.load_crew_settings();
    tokio::task::spawn_blocking(move || push(setup.as_ref(), &settings))
        .await
        .unwrap_or_else(|e| Status::off(format!("memory cap check failed: {e}")))
}

fn push(setup: Option<&super::spawn::layout::Setup>, settings: &CrewSettings) -> Status {
    #[cfg(unix)]
    {
        let Some(setup) = setup else {
            return Status::off("crewmates do not run as their own users here (no spawn helper)");
        };
        let client = super::spawn::client::Client::at(&setup.socket);
        match client.set_memory_cap(cap_for(settings, mem_total())) {
            Ok(s) => s,
            // An older helper answers an error for a request it does not know.
            Err(e) => Status::off(format!("the spawn helper did not accept the cap: {e}")),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (setup, settings);
        Status::off("per-agent memory caps need Linux")
    }
}

/// The `memory_cap` object of `crew_settings_get`.
pub(crate) fn report(hub: &Hub) -> Value {
    json!(apply(hub))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    #[test]
    fn the_setting_takes_zero_as_the_default_and_refuses_what_cannot_run() {
        assert_eq!(parse_setting(0), Ok(None));
        assert_eq!(parse_setting(2 * GIB), Ok(Some(2 * GIB)));
        assert_eq!(parse_setting(MIN_EXPLICIT), Ok(Some(MIN_EXPLICIT)));
        let e = parse_setting(MIN_EXPLICIT - 1).unwrap_err();
        assert!(e.contains("at least 64 MiB"), "{e}");
    }

    #[test]
    fn the_cap_follows_both_settings() {
        let mut s = CrewSettings {
            max_agents: 4,
            ..CrewSettings::default()
        };
        assert_eq!(cap_for(&s, 16 * GIB), 3 * GIB);
        // More agents share the same memory: each gets less.
        s.max_agents = 8;
        assert_eq!(cap_for(&s, 16 * GIB), (3 * GIB) / 2);
        // An explicit cap beats the computation.
        s.agent_memory_max = Some(5 * GIB);
        assert_eq!(cap_for(&s, 16 * GIB), 5 * GIB);
    }

    #[test]
    fn the_setting_round_trips_and_an_old_crew_json_reads_as_unset() {
        let s = CrewSettings {
            agent_memory_max: Some(3 * GIB),
            ..CrewSettings::default()
        };
        let back: CrewSettings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.agent_memory_max, Some(3 * GIB));
        // crew.json written before the setting existed.
        let old: CrewSettings =
            serde_json::from_str(r#"{"short_replies":true,"paused":false,"max_agents":4}"#)
                .unwrap();
        assert_eq!(old.agent_memory_max, None);
        // Unset is not written at all, so an older hostd reading the file is not confused.
        let json = serde_json::to_value(CrewSettings::default()).unwrap();
        assert!(json.get("agent_memory_max").is_none(), "{json}");
    }
}
