//! Service-state trigger: polls `.logs/<service>.log` every 30 s,
//! fires on `failed` / `recovered` transitions.
//!
//! State is tracked in memory (per trigger ID); unknown → failed does NOT fire
//! to avoid spurious alerts on first startup when logs are stale.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::crew::store::TriggerKind;
use crate::server::Hub;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Unknown,
    Running,
    Failed,
}

pub(super) async fn run(hub_weak: Weak<Hub>) {
    // key = trigger_id → last known state
    let mut states: HashMap<String, State> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(hub) = hub_weak.upgrade() else { break };
        let crewmates = hub.crew.store.list_crewmates();
        for crewmate in &crewmates {
            for trigger in &crewmate.triggers {
                if !trigger.enabled {
                    continue;
                }
                if let TriggerKind::Service {
                    project_dir,
                    service,
                    on,
                } = &trigger.kind
                {
                    check(
                        &hub,
                        &crewmate.id,
                        trigger,
                        project_dir,
                        service,
                        on,
                        &mut states,
                    );
                }
            }
        }
    }
}

fn check(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    trigger: &crate::crew::store::Trigger,
    project_dir: &str,
    service: &str,
    on: &str,
    states: &mut HashMap<String, State>,
) {
    let log_path = Path::new(project_dir)
        .join(".logs")
        .join(format!("{service}.log"));
    let current = detect(&log_path);
    let prev = states.get(&trigger.id).copied().unwrap_or(State::Unknown);
    states.insert(trigger.id.clone(), current);

    let fires = match (prev, current, on) {
        // Running → Failed: fire "failed"
        (State::Running, State::Failed, "failed") => true,
        // Failed → Running: fire "recovered"
        (State::Failed, State::Running, "recovered") => true,
        // Don't fire Unknown → Failed to avoid startup noise.
        _ => false,
    };

    if fires {
        let label = match current {
            State::Failed => "failed",
            State::Running => "recovered",
            State::Unknown => "unknown",
        };
        let event = format!("Service {service} {label} in {project_dir}");
        super::fire(
            hub,
            crewmate_id,
            &trigger.id,
            &trigger.prompt,
            super::Event::Local(&event),
        );
    }
}

/// How much of the end of a log `detect` reads.
const TAIL_BYTES: u64 = 64 * 1024;

/// Classify the last meaningful log line.
fn detect(log_path: &Path) -> State {
    // Only the end of the log tells the state: a service log can be gigabytes.
    let Ok(content) = crate::crew::store::jsonl::read_tail(log_path, TAIL_BYTES) else {
        return State::Unknown;
    };
    let last = content
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let lower = last.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("failed") || lower.contains("crash") {
        State::Failed
    } else if lower.contains("started") || lower.contains("ready") || lower.contains("listening") {
        State::Running
    } else {
        State::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    // #295: `detect` read the whole log every 30 s. Only the end can matter: this log is 256 MiB (sparse)
    // and starts with a byte that is not UTF-8, which the old whole-file read could not even decode.
    #[test]
    fn detect_reads_only_the_end_of_a_huge_log() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("api.log");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"\xff").unwrap();
        let len = 256u64 << 20;
        f.set_len(len).unwrap();
        f.seek(SeekFrom::Start(len - 24)).unwrap();
        f.write_all(b"\n12:00:01 [stdout] ready\n").unwrap();
        drop(f);

        assert_eq!(detect(&p), State::Running);
    }

    #[test]
    fn detect_reads_the_last_meaningful_line_of_a_small_log() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("api.log");
        std::fs::write(&p, "listening on 3000\ncrash: boom\n\n").unwrap();
        assert_eq!(detect(&p), State::Failed);
        std::fs::write(&p, "crash: boom\nlistening on 3000\n").unwrap();
        assert_eq!(detect(&p), State::Running);
        assert_eq!(detect(&t.path().join("none.log")), State::Unknown);
    }
}
