//! The newest live frame, shared between the capture side (ScreenCaptureKit on macOS, a `grim` loop on Linux) and the
//! debug bridge's `frame` pull behind `bun run desktop:mirror`.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

#[derive(Default)]
pub struct Latest {
    pub seq: u64,
    pub jpeg: Arc<Vec<u8>>,
    pub size: (usize, usize),
}

#[derive(Default)]
pub struct Shared {
    latest: Mutex<Latest>,
    fresh: Condvar,
}

impl Shared {
    /// Make `jpeg` the newest frame and wake every waiting pull.
    pub fn publish(&self, jpeg: Vec<u8>, size: (usize, usize)) {
        let mut l = self.latest.lock();
        l.seq += 1;
        l.jpeg = Arc::new(jpeg);
        l.size = size;
        self.fresh.notify_all();
    }

    /// The newest frame's bytes, to tell a repeat from a change.
    #[cfg(target_os = "linux")]
    pub fn newest_jpeg(&self) -> Arc<Vec<u8>> {
        self.latest.lock().jpeg.clone()
    }
}

/// The newest frame after `since`, waiting up to `wait` for one (a still window sends no new frames: the current
/// one comes back after the wait so the viewer keeps a picture).
pub fn pull(shared: &Shared, since: u64, wait: Duration) -> super::Frame {
    let mut l = shared.latest.lock();
    if l.seq <= since {
        let _ = shared.fresh.wait_for(&mut l, wait);
    }
    (l.seq, l.jpeg.clone(), l.size)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use atlas_core::scaling::HANG;

    use super::*;

    #[test]
    fn a_pull_after_the_newest_frame_waits_for_the_next_and_a_stale_one_returns_at_once() {
        let s = Arc::new(Shared::default());
        s.publish(vec![1], (10, 20));
        // A newer frame exists: the wait limit is never started. The limit is the hang ceiling, so a pull that
        // waited anyway would come back only after all of it, and that is what the assert refuses.
        let t = Instant::now();
        let (seq, jpeg, size) = pull(&s, 0, HANG);
        assert!(t.elapsed() < HANG, "a newer frame exists: no wait");
        assert_eq!((seq, &**jpeg, size), (1, &[1u8][..], (10, 20)));

        let s2 = s.clone();
        let publisher = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            s2.publish(vec![2], (10, 20));
        });
        let (seq, jpeg, _) = pull(&s, 1, HANG);
        publisher.join().unwrap();
        assert_eq!(
            (seq, &**jpeg),
            (2, &[2u8][..]),
            "woken by the publish, not by the timeout"
        );

        // Nothing new: the wait times out and the viewer still gets the current picture.
        let t = Instant::now();
        let (seq, jpeg, _) = pull(&s, 2, Duration::from_millis(50));
        assert!(t.elapsed() >= Duration::from_millis(50));
        assert_eq!((seq, &**jpeg), (2, &[2u8][..]));
    }
}
