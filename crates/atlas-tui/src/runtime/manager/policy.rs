//! Pure scheduling policy: restart decisions + dependency-condition evaluation.
//! No I/O and no clocks (callers pass `Instant`s), so it is unit-testable.

use std::time::{Duration, Instant};

use crate::config::{Condition, RestartPolicy};

/// Default `max_restarts` when the yaml leaves it out.
pub const DEFAULT_MAX_RESTARTS: u32 = 5;
/// A service that stays Running this long gets its restart counter reset.
pub const RESET_AFTER: Duration = Duration::from_secs(60);
const BACKOFF_BASE: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Exponential backoff for the `attempt`-th restart (0-based): 1s, 2s, 4s … 30s.
pub fn backoff(attempt: u32) -> Duration {
    BACKOFF_BASE
        .checked_mul(1u32 << attempt.min(16))
        .unwrap_or(BACKOFF_CAP)
        .min(BACKOFF_CAP)
}

/// What to do after an atlas-spawned child exited on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    /// Not restartable by policy (no policy, or a clean exit under `on_failure`).
    No,
    /// Schedule restart number `attempt` (1-based) after `delay`.
    Restart { attempt: u32, delay: Duration },
    /// Policy wants a restart but the budget is spent.
    GiveUp { attempts: u32 },
}

/// Per-service restart bookkeeping.
#[derive(Debug, Clone, Default)]
pub struct RestartTracker {
    /// Consecutive restarts since the last reset.
    pub attempts: u32,
    /// When the service last became Running (for the 60s reset).
    running_since: Option<Instant>,
    /// A restart is scheduled for this instant.
    pub pending_at: Option<Instant>,
}

impl RestartTracker {
    /// Record that the service is Running at `now`; starts the stability clock once.
    pub fn mark_running(&mut self, now: Instant) {
        self.running_since.get_or_insert(now);
    }

    /// Record a state other than Running (clears the stability clock).
    pub fn mark_not_running(&mut self) {
        self.running_since = None;
    }

    /// Reset the counter when the service has been Running for `RESET_AFTER`.
    /// Returns true if a reset happened.
    pub fn reset_if_stable(&mut self, now: Instant) -> bool {
        match self.running_since {
            Some(t) if self.attempts > 0 && now.duration_since(t) >= RESET_AFTER => {
                self.attempts = 0;
                true
            }
            _ => false,
        }
    }

    /// Decide what happens after the child exited (`exit_ok` = status 0).
    /// Bumps `attempts` and arms `pending_at` when a restart is scheduled.
    pub fn on_exit(
        &mut self,
        policy: RestartPolicy,
        max_restarts: Option<u32>,
        exit_ok: bool,
        now: Instant,
    ) -> RestartDecision {
        self.running_since = None;
        let wants = match policy {
            RestartPolicy::No => false,
            RestartPolicy::OnFailure => !exit_ok,
            RestartPolicy::Always => true,
        };
        if !wants {
            return RestartDecision::No;
        }
        let max = max_restarts.unwrap_or(DEFAULT_MAX_RESTARTS);
        if max != 0 && self.attempts >= max {
            return RestartDecision::GiveUp {
                attempts: self.attempts,
            };
        }
        let delay = backoff(self.attempts);
        self.attempts += 1;
        self.pending_at = Some(now + delay);
        RestartDecision::Restart {
            attempt: self.attempts,
            delay,
        }
    }

    /// True (and clears the schedule) once the pending restart is due.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.pending_at {
            Some(t) if now >= t => {
                self.pending_at = None;
                true
            }
            _ => false,
        }
    }

    /// Cancel a scheduled restart (user stop / quit).
    pub fn cancel(&mut self) {
        self.pending_at = None;
    }
}

/// Observable facts about one dependency, as seen by the scheduler.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DepFacts {
    /// Child has been spawned at least once this run.
    pub spawned: bool,
    /// Currently Running (probe or ready_log).
    pub healthy: bool,
    /// A `ready_log` line matched.
    pub log_ready: bool,
    /// Child exited with status 0 (and is not being restarted).
    pub completed_ok: bool,
    /// Terminal failure: Failed state with no restart pending, or exited non-zero
    /// with no restart pending, or gave up.
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Ready,
    Waiting,
    /// The condition can never be satisfied.
    Failed,
}

/// Evaluate one `(dependency, condition)` edge.
pub fn evaluate(cond: Condition, f: &DepFacts) -> Verdict {
    let satisfied = match cond {
        Condition::Started => f.spawned,
        Condition::Healthy => f.healthy,
        Condition::LogReady => f.log_ready,
        Condition::CompletedSuccessfully => f.completed_ok,
    };
    if satisfied {
        Verdict::Ready
    } else if f.failed {
        Verdict::Failed
    } else {
        Verdict::Waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn backoff_doubles_and_caps_at_30s() {
        let secs: Vec<u64> = (0..8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(backoff(u32::MAX), BACKOFF_CAP);
    }

    #[test]
    fn no_policy_never_restarts() {
        let mut t = RestartTracker::default();
        assert_eq!(
            t.on_exit(RestartPolicy::No, None, false, t0()),
            RestartDecision::No
        );
        assert_eq!(t.attempts, 0);
    }

    #[test]
    fn on_failure_skips_clean_exit_but_restarts_crash() {
        let mut t = RestartTracker::default();
        assert_eq!(
            t.on_exit(RestartPolicy::OnFailure, None, true, t0()),
            RestartDecision::No
        );
        assert!(matches!(
            t.on_exit(RestartPolicy::OnFailure, None, false, t0()),
            RestartDecision::Restart { attempt: 1, .. }
        ));
    }

    #[test]
    fn always_restarts_clean_exit() {
        let mut t = RestartTracker::default();
        assert!(matches!(
            t.on_exit(RestartPolicy::Always, None, true, t0()),
            RestartDecision::Restart { attempt: 1, .. }
        ));
    }

    #[test]
    fn gives_up_after_max_restarts_with_growing_backoff() {
        let mut t = RestartTracker::default();
        let now = t0();
        let d1 = t.on_exit(RestartPolicy::OnFailure, Some(2), false, now);
        let d2 = t.on_exit(RestartPolicy::OnFailure, Some(2), false, now);
        let d3 = t.on_exit(RestartPolicy::OnFailure, Some(2), false, now);
        assert_eq!(
            d1,
            RestartDecision::Restart {
                attempt: 1,
                delay: Duration::from_secs(1)
            }
        );
        assert_eq!(
            d2,
            RestartDecision::Restart {
                attempt: 2,
                delay: Duration::from_secs(2)
            }
        );
        assert_eq!(d3, RestartDecision::GiveUp { attempts: 2 });
    }

    #[test]
    fn zero_max_is_unlimited() {
        let mut t = RestartTracker::default();
        for _ in 0..50 {
            assert!(matches!(
                t.on_exit(RestartPolicy::Always, Some(0), false, t0()),
                RestartDecision::Restart { .. }
            ));
        }
    }

    #[test]
    fn default_budget_is_five() {
        let mut t = RestartTracker::default();
        for _ in 0..DEFAULT_MAX_RESTARTS {
            assert!(matches!(
                t.on_exit(RestartPolicy::Always, None, false, t0()),
                RestartDecision::Restart { .. }
            ));
        }
        assert_eq!(
            t.on_exit(RestartPolicy::Always, None, false, t0()),
            RestartDecision::GiveUp { attempts: 5 }
        );
    }

    #[test]
    fn counter_resets_only_after_a_stable_minute() {
        let mut t = RestartTracker::default();
        let start = t0();
        t.on_exit(RestartPolicy::Always, None, false, start);
        t.mark_running(start);
        assert!(!t.reset_if_stable(start + Duration::from_secs(59)));
        assert_eq!(t.attempts, 1);
        assert!(t.reset_if_stable(start + RESET_AFTER));
        assert_eq!(t.attempts, 0);
    }

    #[test]
    fn leaving_running_restarts_the_stability_clock() {
        let mut t = RestartTracker::default();
        let start = t0();
        t.on_exit(RestartPolicy::Always, None, false, start);
        t.mark_running(start);
        t.mark_not_running();
        t.mark_running(start + Duration::from_secs(50));
        assert!(!t.reset_if_stable(start + Duration::from_secs(100)));
        assert!(t.reset_if_stable(start + Duration::from_secs(110)));
    }

    #[test]
    fn pending_restart_fires_once_when_due_and_can_be_cancelled() {
        let mut t = RestartTracker::default();
        let now = t0();
        t.on_exit(RestartPolicy::Always, None, false, now);
        assert!(!t.take_due(now));
        assert!(t.take_due(now + Duration::from_secs(1)));
        assert!(!t.take_due(now + Duration::from_secs(2)));
        t.on_exit(RestartPolicy::Always, None, false, now);
        t.cancel();
        assert!(!t.take_due(now + Duration::from_secs(60)));
    }

    fn facts() -> DepFacts {
        DepFacts::default()
    }

    #[test]
    fn started_needs_a_spawn() {
        assert_eq!(evaluate(Condition::Started, &facts()), Verdict::Waiting);
        let f = DepFacts {
            spawned: true,
            ..facts()
        };
        assert_eq!(evaluate(Condition::Started, &f), Verdict::Ready);
    }

    #[test]
    fn healthy_needs_running() {
        let f = DepFacts {
            spawned: true,
            ..facts()
        };
        assert_eq!(evaluate(Condition::Healthy, &f), Verdict::Waiting);
        let f = DepFacts {
            spawned: true,
            healthy: true,
            ..facts()
        };
        assert_eq!(evaluate(Condition::Healthy, &f), Verdict::Ready);
    }

    #[test]
    fn log_ready_needs_a_matching_line() {
        let f = DepFacts {
            spawned: true,
            healthy: true,
            ..facts()
        };
        assert_eq!(evaluate(Condition::LogReady, &f), Verdict::Waiting);
        let f = DepFacts {
            log_ready: true,
            ..f
        };
        assert_eq!(evaluate(Condition::LogReady, &f), Verdict::Ready);
    }

    #[test]
    fn completed_successfully_needs_exit_zero() {
        let f = DepFacts {
            spawned: true,
            healthy: true,
            ..facts()
        };
        assert_eq!(
            evaluate(Condition::CompletedSuccessfully, &f),
            Verdict::Waiting
        );
        let f = DepFacts {
            completed_ok: true,
            ..f
        };
        assert_eq!(
            evaluate(Condition::CompletedSuccessfully, &f),
            Verdict::Ready
        );
    }

    #[test]
    fn dependency_failure_fails_every_unsatisfied_condition() {
        let f = DepFacts {
            spawned: true,
            failed: true,
            ..facts()
        };
        for c in [
            Condition::Healthy,
            Condition::LogReady,
            Condition::CompletedSuccessfully,
        ] {
            assert_eq!(evaluate(c, &f), Verdict::Failed, "{c:?}");
        }
        // Already spawned → `started` is still satisfied.
        assert_eq!(evaluate(Condition::Started, &f), Verdict::Ready);
    }

    #[test]
    fn satisfied_condition_wins_over_failed_flag() {
        // log_ready seen, then the service crashed: dependents already past the
        // gate are unaffected; evaluation of the satisfied edge stays Ready.
        let f = DepFacts {
            log_ready: true,
            failed: true,
            ..facts()
        };
        assert_eq!(evaluate(Condition::LogReady, &f), Verdict::Ready);
    }
}
