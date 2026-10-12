//! Sidebar health summary. Completed one-shots and user-stopped / disabled /
//! suspended services are not "down", so they stay out of the denominator.

use crate::runtime::service::ServiceState;

/// Counts behind `5/5 running · 1 done · 1 stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Health {
    pub running: usize,
    pub starting: usize,
    pub failed: usize,
    /// Completed one-shots.
    pub done: usize,
    /// Stopped by the user, disabled, or suspended by an action.
    pub stopped: usize,
}

impl Health {
    /// `states`: (state, completed_ok) per service.
    pub fn from_states(states: impl IntoIterator<Item = (ServiceState, bool)>) -> Self {
        let mut h = Self::default();
        for (state, completed_ok) in states {
            match state {
                ServiceState::Running => h.running += 1,
                ServiceState::Starting => h.starting += 1,
                ServiceState::Failed => h.failed += 1,
                ServiceState::Stopped if completed_ok => h.done += 1,
                ServiceState::Stopped | ServiceState::Disabled | ServiceState::Suspended => {
                    h.stopped += 1
                }
            }
        }
        h
    }

    /// Services that are expected to be up (running, coming up, or broken).
    pub fn expected(&self) -> usize {
        self.running + self.starting + self.failed
    }

    pub fn total(&self) -> usize {
        self.expected() + self.done + self.stopped
    }

    /// Everything expected is running (and at least one thing is).
    pub fn all_ok(&self) -> bool {
        self.expected() > 0 && self.running == self.expected()
    }

    /// Pulse the status dot only while something is coming up or broken.
    pub fn pulses(&self) -> bool {
        self.failed > 0 || self.starting > 0
    }

    pub fn percent(&self) -> usize {
        (self.running * 100)
            .checked_div(self.expected())
            .unwrap_or(0)
    }

    /// `5/5 running · 1 done · 1 stopped`.
    pub fn label(&self) -> String {
        let mut s = format!("{}/{} running", self.running, self.expected());
        if self.done > 0 {
            s.push_str(&format!(" · {} done", self.done));
        }
        if self.stopped > 0 {
            s.push_str(&format!(" · {} stopped", self.stopped));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ServiceState::*;

    fn h(v: &[(ServiceState, bool)]) -> Health {
        Health::from_states(v.iter().cloned())
    }

    #[test]
    fn oneshot_done_and_stopped_leave_the_denominator() {
        let x = h(&[
            (Running, false),
            (Running, false),
            (Running, false),
            (Running, false),
            (Running, false),
            (Stopped, true),
            (Stopped, false),
        ]);
        assert_eq!(x.label(), "5/5 running · 1 done · 1 stopped");
        assert!(x.all_ok());
        assert!(!x.pulses());
        assert_eq!(x.percent(), 100);
        assert_eq!(x.total(), 7);
    }

    #[test]
    fn disabled_and_suspended_count_as_stopped() {
        let x = h(&[(Running, false), (Disabled, false), (Suspended, false)]);
        assert_eq!(x.label(), "1/1 running · 2 stopped");
        assert!(x.all_ok());
    }

    #[test]
    fn failed_and_starting_are_expected_and_pulse() {
        let x = h(&[(Running, false), (Failed, false), (Starting, false)]);
        assert_eq!(x.label(), "1/3 running");
        assert!(!x.all_ok());
        assert!(x.pulses());
        assert_eq!(x.percent(), 33);
    }

    #[test]
    fn nothing_expected_is_not_ok_and_has_no_percent() {
        let x = h(&[(Stopped, true)]);
        assert!(!x.all_ok());
        assert_eq!(x.percent(), 0);
        assert_eq!(x.label(), "0/0 running · 1 done");
    }
}
