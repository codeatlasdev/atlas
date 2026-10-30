//! How long a synchronized update (DECSET 2026) may stay open before the screen is drawn anyway.
//!
//! vte hard-codes 150 ms, which an agent's frame can exceed under load (a hook, a GC pause, a slow tool): the
//! half-drawn frame was painted, the flicker the program asked 2026 to avoid. Ghostty's `sync_reset_ms` is
//! 1000 ms. The parser is generic over its `Timeout`, so Atlas ignores the duration vte asks for.

use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Timeout;

/// Ghostty `termio/Thread.zig` `sync_reset_ms`.
pub(super) const HOLD: Duration = Duration::from_millis(1000);

#[derive(Default)]
pub(crate) struct SyncHold {
    until: Option<Instant>,
}

impl SyncHold {
    /// When the pending update must be force-ended.
    pub(crate) fn sync_timeout(&self) -> Option<Instant> {
        self.until
    }
}

impl Timeout for SyncHold {
    fn set_timeout(&mut self, _vte_asked: Duration) {
        self.until = Some(Instant::now() + HOLD);
    }
    fn clear_timeout(&mut self) {
        self.until = None;
    }
    fn pending_timeout(&self) -> bool {
        self.until.is_some()
    }
}
