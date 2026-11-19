//! The clock behind a timer whose DECISION a test has to drive.
//!
//! "Nothing happened for 3 s, so exit" tested with a 3 s limit measures the machine: a loaded runner lets the
//! limit pass before the test has connected (or stretches a sleep past the margin the test counted on), and the
//! test is red with nothing broken. A component that takes a [`Clock`] instead of calling `Instant::now()` and
//! `tokio::time::sleep` gets the real one in production ([`Clock::default`], a thin shim over tokio's timer) and a
//! [`ManualClock`] in a test, which moves only when the test says so: idleness is then a statement ("an hour
//! passed"), not a race. The timers still run on the tokio runtime; only what they measure is injected.

use std::time::{Duration, Instant};

#[cfg(any(test, feature = "test-fixtures"))]
use std::sync::Arc;

/// A source of `now` and of sleeps. Cheap to clone; the default is the system clock.
#[derive(Debug, Clone, Default)]
pub struct Clock(Inner);

#[derive(Debug, Clone, Default)]
enum Inner {
    #[default]
    System,
    #[cfg(any(test, feature = "test-fixtures"))]
    Manual(Arc<manual::Shared>),
}

impl Clock {
    /// The current instant.
    pub fn now(&self) -> Instant {
        match &self.0 {
            Inner::System => Instant::now(),
            #[cfg(any(test, feature = "test-fixtures"))]
            Inner::Manual(m) => m.now(),
        }
    }

    /// Resolves once `deadline` has passed on this clock.
    pub fn sleep_until(&self, deadline: Instant) -> impl Future<Output = ()> + Send + 'static {
        let clock = self.clone();
        async move {
            match clock.0 {
                Inner::System => tokio::time::sleep_until(deadline.into()).await,
                #[cfg(any(test, feature = "test-fixtures"))]
                Inner::Manual(m) => m.sleep_until(deadline).await,
            }
        }
    }

    /// Resolves `after` from now on this clock. The deadline is fixed when this is CALLED, not when the future is
    /// first polled, so a task spawned to sleep cannot start its sleep later than the code that spawned it.
    pub fn sleep(&self, after: Duration) -> impl Future<Output = ()> + Send + 'static {
        self.sleep_until(self.now() + after)
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
pub use manual::ManualClock;

#[cfg(any(test, feature = "test-fixtures"))]
mod manual {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;
    use tokio::sync::watch;

    use super::{Clock, Inner};

    pub struct Shared {
        base: Instant,
        offset: Mutex<Duration>,
        /// Bumped by every [`ManualClock::advance`]: what a pending sleep waits on.
        moved: watch::Sender<()>,
        /// Sleeps that started waiting / that finished waiting, as counters a test can wait on.
        armed: watch::Sender<u64>,
        fired: watch::Sender<u64>,
    }

    impl std::fmt::Debug for Shared {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ManualClock")
        }
    }

    impl Shared {
        pub fn now(&self) -> Instant {
            self.base + *self.offset.lock()
        }

        pub async fn sleep_until(&self, deadline: Instant) {
            self.armed.send_modify(|n| *n += 1);
            // Subscribe before looking at the time: an `advance` between the two is then seen as a change.
            let mut moved = self.moved.subscribe();
            while self.now() < deadline {
                let _ = moved.changed().await;
            }
            self.fired.send_modify(|n| *n += 1);
        }
    }

    /// A clock that moves only by [`advance`](Self::advance). Hand [`clock`](Self::clock) to the code under test.
    ///
    /// `armed` counts the sleeps that started and `fired` those that ended, so a test can wait for the real event
    /// ("the daemon looked again") instead of for a duration. `fired` is the moment the sleeper's continuation
    /// starts; on the current-thread runtime of `#[tokio::test]` it has run up to its next await by the time the
    /// test resumes.
    #[derive(Debug, Clone)]
    pub struct ManualClock(Arc<Shared>);

    impl Default for ManualClock {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ManualClock {
        pub fn new() -> Self {
            Self(Arc::new(Shared {
                base: Instant::now(),
                offset: Mutex::new(Duration::ZERO),
                moved: watch::channel(()).0,
                armed: watch::channel(0).0,
                fired: watch::channel(0).0,
            }))
        }

        /// The handle the code under test takes.
        pub fn clock(&self) -> Clock {
            Clock(Inner::Manual(self.0.clone()))
        }

        /// Move time forward: every sleep whose deadline is now past wakes.
        pub fn advance(&self, by: Duration) {
            *self.0.offset.lock() += by;
            self.0.moved.send_replace(());
        }

        pub fn armed(&self) -> u64 {
            *self.0.armed.borrow()
        }

        pub fn fired(&self) -> u64 {
            *self.0.fired.borrow()
        }

        /// Wait until more than `n` sleeps have started (a recurring timer that re-arms after handling a tick:
        /// its re-arm proves the tick was handled).
        pub async fn armed_past(&self, n: u64) {
            wait_past(&self.0.armed, n, "armed").await;
        }

        /// Wait until more than `n` sleeps have finished.
        pub async fn fired_past(&self, n: u64) {
            wait_past(&self.0.fired, n, "fired").await;
        }

        /// Advance `by` and wait for the recurring timer to handle the tick that brings and arm again.
        pub async fn tick(&self, by: Duration) {
            let before = self.armed();
            self.advance(by);
            self.armed_past(before).await;
        }
    }

    async fn wait_past(counter: &watch::Sender<u64>, n: u64, what: &str) {
        let mut rx = counter.subscribe();
        tokio::time::timeout(crate::scaling::HANG, rx.wait_for(|c| *c > n))
            .await
            .unwrap_or_else(|_| panic!("manual clock: no sleep {what} after the {n}th"))
            .expect("the clock outlives its waiters");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn a_manual_clock_stands_still_until_advanced() {
        let manual = ManualClock::new();
        let clock = manual.clock();
        let t0 = clock.now();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(clock.now(), t0, "real time must not move a manual clock");
        manual.advance(HOUR);
        assert_eq!(clock.now() - t0, HOUR);
    }

    #[tokio::test]
    async fn a_sleep_wakes_only_when_its_deadline_has_passed() {
        let manual = ManualClock::new();
        let sleep = tokio::spawn(manual.clock().sleep(HOUR));
        manual.armed_past(0).await;
        manual.advance(HOUR - Duration::from_secs(1));
        // Awaited as an event on the counter: a sleep that woke early would already have bumped it.
        tokio::task::yield_now().await;
        assert_eq!(manual.fired(), 0, "woke 1 s early");
        assert!(!sleep.is_finished());
        manual.advance(Duration::from_secs(1));
        sleep.await.unwrap();
        assert_eq!(manual.fired(), 1);
    }

    #[tokio::test]
    async fn the_deadline_of_sleep_is_fixed_when_it_is_called() {
        let manual = ManualClock::new();
        let clock = manual.clock();
        let sleep = clock.sleep(HOUR); // not polled yet
        manual.advance(HOUR);
        tokio::time::timeout(crate::scaling::HANG, sleep)
            .await
            .expect("a sleep called before the advance ended up an hour late");
    }

    #[tokio::test]
    async fn the_system_clock_sleeps_for_real() {
        let clock = Clock::default();
        let t0 = clock.now();
        clock.sleep(Duration::from_millis(30)).await;
        assert!(clock.now() - t0 >= Duration::from_millis(30));
    }
}
