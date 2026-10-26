//! Scrollback bounded in bytes. alacritty caps history in lines, but a line costs `columns` cells, so
//! 100 000 lines at 500 columns is over a GB per tab. The line cap stays (user setting); the byte
//! budget tightens it to what fits, per tab.

use std::sync::atomic::Ordering;

use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Cell;

use super::{Proxy, Vt};

/// Bytes of scrollback cells kept per terminal.
pub(super) const BYTE_BUDGET: usize = 64 << 20;

/// History lines allowed: the user's `lines` cap, further limited by `budget` at `cols` columns per line.
pub(super) fn history_lines(lines: usize, cols: usize, budget: usize) -> usize {
    lines.min(budget / (cols.max(1) * size_of::<Cell>()))
}

/// Cell bytes of the lines currently in history.
pub(super) fn history_bytes(g: &Grid<Cell>) -> usize {
    g.history_size() * g.columns() * size_of::<Cell>()
}

impl Vt {
    /// Apply the user's line cap bounded by the byte budget at the current width. Called again when
    /// the width changes: the same bytes hold fewer lines at 500 columns than at 80.
    pub(super) fn apply_history(&self, t: &mut Term<Proxy>) {
        let lines = self.scrollback.load(Ordering::Relaxed);
        let cap = history_lines(lines, t.columns(), BYTE_BUDGET);
        t.grid_mut().update_history(cap);
        Self::reclaim(t);
    }

    /// Give the memory of dropped history back: alacritty keeps up to 1 000 spare rows after a shrink
    /// (ED3, RIS, a smaller cap) and only truncates beyond that.
    pub(super) fn reclaim(t: &mut Term<Proxy>) {
        t.grid_mut().truncate();
    }

    /// Cell bytes held by the scrollback (the memory probe the tests and the soak read).
    pub fn history_bytes(&self) -> usize {
        history_bytes(self.term.lock().grid())
    }

    /// Lines held by the scrollback.
    pub fn history_lines(&self) -> usize {
        self.term.lock().grid().history_size()
    }
}
