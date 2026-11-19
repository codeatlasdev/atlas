//! Read-only facts about a service's schedule (sidebar notes, exit code,
//! restart counts, uptime, flags), answered from the live state or, for an
//! attached client, from the runner's mirror.

use std::time::Instant;

use serde_json::json;

use super::ServiceManager;
use crate::runtime::note::Note;
use crate::runtime::service::ServiceState;

impl ServiceManager {
    /// Sidebar annotation for `name` in English, one line: waiting reason, restart countdown, gave-up. What the
    /// status file, MCP and `--ci` carry; a person reads [`Self::service_note_coded`].
    pub fn service_note(&self, name: &str) -> Option<String> {
        self.service_note_coded(name).map(|n| n.line())
    }

    /// The sidebar annotation as a coded sentence (render it with [`Note::render_line`]).
    pub fn service_note_coded(&self, name: &str) -> Option<Note> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.note.clone();
        }
        let s = self.services.get(name)?;
        if let Some(at) = s.sched.tracker.pending_at {
            let secs = at.saturating_duration_since(Instant::now()).as_secs() + 1;
            let max = match s
                .sched
                .max_restarts
                .unwrap_or(super::policy::DEFAULT_MAX_RESTARTS)
            {
                0 => "∞".to_string(),
                m => m.to_string(),
            };
            return Some(Note::new(
                "sched.restarting_in",
                json!({"secs": secs, "attempt": s.sched.tracker.attempts, "limit": max}),
            ));
        }
        if let Some(w) = &s.sched.waiting {
            return Some(w.clone());
        }
        if let Some((port, secs)) = self.start_wait(name) {
            let wait = match (port, s.sched.start_warned) {
                (Some(p), true) => {
                    Note::new("sched.start_timeout_port", json!({"port": p, "secs": secs}))
                }
                (Some(p), false) => {
                    Note::new("sched.waiting_port", json!({"port": p, "secs": secs}))
                }
                (None, true) => Note::new("sched.start_timeout_log", json!({"secs": secs})),
                (None, false) => Note::new("sched.waiting_log", json!({"secs": secs})),
            };
            let held = s
                .sched
                .port_conflict
                .as_ref()
                .zip(port)
                .map(|(c, p)| c.note(p));
            return Some(match held {
                Some(h) => Note::pair(wait, h),
                None => wait,
            });
        }
        let base = match s.status.state {
            ServiceState::Failed => match (&s.sched.port_conflict, s.status.port) {
                (Some(c), Some(p)) => Some(c.note(p)),
                _ => s.sched.fail_reason.clone(),
            },
            ServiceState::Unhealthy => s.probes.unhealthy.as_ref().map(|u| {
                Note::new(
                    "health.unhealthy_for",
                    json!({"secs": u.since.elapsed().as_secs(), "misses": u.misses}),
                )
            }),
            ServiceState::Degraded => s.probes.degraded.clone(),
            ServiceState::Stopped if s.sched.completed_ok => {
                Some(Note::new("sched.done", json!({})))
            }
            ServiceState::Running if s.sched.tracker.attempts > 0 => Some(Note::new(
                "sched.restarted",
                json!({"attempts": s.sched.tracker.attempts}),
            )),
            _ => None,
        };
        // A suspected leak rides along on a live service: the reason it is `degraded` (or the
        // restart count) stays, the leak is added, never hidden behind it.
        let leak = matches!(
            s.status.state,
            ServiceState::Running | ServiceState::Degraded
        )
        .then(|| s.mem.leak.suspect().map(|l| l.note()))
        .flatten();
        match (base, leak) {
            (Some(b), Some(l)) => Some(Note::pair(b, l)),
            (b, l) => b.or(l),
        }
    }

    /// `(port, seconds)` while a spawned service is Starting and waiting on its
    /// probe: `Some(port)` for a port/health wait, `None` port for `ready_log`.
    /// `None` when nothing is being waited on (including no-probe services,
    /// which settle after `MIN_UPTIME` on their own).
    pub fn start_wait(&self, name: &str) -> Option<(Option<u16>, u64)> {
        let s = self.services.get(name)?;
        if s.status.state != ServiceState::Starting
            || s.child.is_none()
            || s.sched.tracker.pending_at.is_some()
        {
            return None;
        }
        let waits_on_log = s.sched.ready_log.is_some();
        if !waits_on_log && s.status.port.is_none() && s.status.health_url.is_none() {
            return None;
        }
        let secs = s.sched.starting_since?.elapsed().as_secs();
        let port = if waits_on_log { None } else { s.status.port };
        Some((port, secs))
    }

    /// Last exit code of the service's process (`None` while running, never
    /// exited, or killed by a signal).
    pub fn exit_code(&self, name: &str) -> Option<i32> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.exit_code;
        }
        let s = self.services.get(name)?;
        if s.child.is_some() { None } else { s.exit_code }
    }

    /// Restarts attempted in the current streak (0 after 60s of stability).
    pub fn restart_count(&self, name: &str) -> u32 {
        if self.mirror.is_some() {
            return self.mirror_facts(name).map_or(0, |f| f.restarts);
        }
        self.services
            .get(name)
            .map_or(0, |s| s.sched.tracker.attempts)
    }

    /// Whole seconds the service has been Running; `None` when it is not.
    pub fn uptime_secs(&self, name: &str) -> Option<u64> {
        let s = self.services.get(name)?;
        (s.status.state == ServiceState::Running)
            .then(|| s.sched.running_since.map(|t| t.elapsed().as_secs()))
            .flatten()
    }

    /// One-shot task (explicit `oneshot`, or awaited via `completed_successfully`).
    pub fn is_oneshot(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| s.sched.oneshot)
    }

    /// Label of the action that suspended `name`, while it is suspended.
    pub fn paused_by(&self, name: &str) -> Option<String> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.paused_by.clone();
        }
        let s = self.services.get(name)?;
        (s.status.state == ServiceState::Suspended)
            .then(|| s.sched.paused_by.clone())
            .flatten()
    }

    /// Whether `name` was left out of this run's selection and not started since.
    pub fn is_deselected(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| s.deselected)
    }

    /// Whether `name` is skipped at boot (`autostart: false`) and not yet started.
    pub fn is_manual_start(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| !s.autostart)
    }

    /// Fail-reason for CI output: English, the first line (what `reason` of `atlas dev --ci` always carried).
    pub fn fail_reason(&self, name: &str) -> Option<String> {
        self.services
            .get(name)?
            .sched
            .fail_reason
            .as_ref()
            .map(Note::line)
    }

    /// The foreign process holding `name`'s port, when its last start found one.
    pub fn port_conflict(&self, name: &str) -> Option<crate::runtime::port_conflict::PortConflict> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.port_conflict.clone();
        }
        self.services.get(name)?.sched.port_conflict.clone()
    }

    /// True if the service finished successfully (one-shot done).
    pub fn completed_ok(&self, name: &str) -> bool {
        if self.mirror.is_some() {
            return self.mirror_facts(name).is_some_and(|f| f.completed_ok);
        }
        self.services
            .get(name)
            .is_some_and(|s| s.sched.completed_ok)
    }

    /// True while anything is queued, waiting on dependencies, or due to restart.
    pub fn scheduling_pending(&self) -> bool {
        self.services
            .values()
            .any(|s| s.sched.pending_start || s.sched.tracker.pending_at.is_some())
    }

    /// Test hook: pretend `name` finished as a one-shot.
    #[cfg(test)]
    pub(crate) fn test_set_completed(&mut self, name: &str) {
        if let Some(s) = self.services.get_mut(name) {
            s.sched.completed_ok = true;
        }
    }

    /// Test hook: pretend the last child of `name` exited with `code`.
    #[cfg(test)]
    pub(crate) fn test_set_exit_code(&mut self, name: &str, code: i32) {
        if let Some(s) = self.services.get_mut(name) {
            s.exit_code = Some(code);
        }
    }
}
