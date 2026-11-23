//! Pulling the history rows that the next scrolls recycle into the cache.
//!
//! A full scrollback is a ring: every line that scrolls in recycles the oldest history row, which `Row::reset`
//! rewrites cell by cell. That row was last touched 10,000 lines ago, so it is in RAM, not in cache, and the
//! reset (a read of each cell's old `extra` plus a 24-byte write) waits on memory for every cell. On a flood of
//! plain text that wait was a third of `Vt::feed` (docs/perf/terminal-linux.md, "Where the grid write goes").
//! The rows the next scrolls will reset are known before they are parsed: the oldest ones, in order, one per
//! line feed. `Vt::advance` feeds the parser one line at a time and asks the CPU to fetch the next rows first,
//! so the memory waits overlap with parsing.
//!
//! Two things were measured to matter, and each is a constant below: the fetch must be *paced* (one row's worth
//! per line parsed: a burst of the 3,000 cache lines of a 64 KiB chunk overflows the CPU's miss queue and most of
//! it is dropped, 0 % gain, against +35 % paced) and it must run *one row ahead* (the row the next line needs).
//! The estimate only decides how much to fetch; a wrong one wastes a prefetch, never a cell.

use alacritty_terminal::grid::Grid;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::cell::Cell;

/// A slice of the stream is cut after the first line feed past this many bytes: about one line of prose. Shorter
/// slices (a line at a time) cost more in `Processor::advance` calls than they save on lines this short.
const MIN_SLICE: usize = 96;

/// Rows ahead of the lines being parsed that are fetched.
const AHEAD: usize = 1;

/// Rows fetched per slice at most (a slice of line feeds only would ask for all of them).
const MAX_ROWS: usize = 1024;

/// A row smaller than this is fetched well enough by the CPU's own stream prefetcher as `Row::reset` walks it: on
/// `seq` (8 cells, 192 bytes) the software fetch measured no gain and cost 10 % of the time.
const MIN_ROW_BYTES: usize = 1024;

const CACHE_LINE: usize = 64;

/// What to fetch before a slice is parsed: the `rows` oldest history rows, `lines` cache lines from the start of each.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Plan {
    pub rows: usize,
    pub lines: usize,
}

/// The fetch for `bytes` fed to a grid that holds `history` rows of at most `cap`, `cols` wide; `None` when
/// nothing will be recycled (history still growing, or no line feed in the bytes) or the rows are too small to
/// gain from it (`MIN_ROW_BYTES`).
pub(super) fn plan(history: usize, cap: usize, cols: usize, bytes: &[u8]) -> Option<Plan> {
    if cap == 0 || history < cap {
        return None;
    }
    let line_feeds = bytes.iter().filter(|&&b| b == b'\n').count();
    if line_feeds == 0 {
        return None;
    }
    // Bytes per line stand for cells per line (escape sequences make it an overestimate, the grid width caps it).
    let cells = (bytes.len() / line_feeds).clamp(1, cols.max(1));
    let row_bytes = cells * size_of::<Cell>();
    if row_bytes < MIN_ROW_BYTES {
        return None;
    }
    Some(Plan {
        rows: (line_feeds + AHEAD).min(history).min(MAX_ROWS),
        // One more than the bytes need: a row's cells do not start on a cache line.
        lines: row_bytes.div_ceil(CACHE_LINE) + 1,
    })
}

/// `bytes` cut into the slices `Vt::advance` parses between fetches: each ends with a line feed, at or past
/// `MIN_SLICE` bytes, except the last.
pub(super) fn slices(mut bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            return None;
        }
        let from = MIN_SLICE.min(bytes.len());
        let end = memchr::memchr(b'\n', &bytes[from..]).map_or(bytes.len(), |i| from + i + 1);
        let (slice, rest) = bytes.split_at(end);
        bytes = rest;
        Some(slice)
    })
}

/// The `k`-th oldest history row (0 = the one the next scroll recycles).
fn nth_oldest(history: usize, k: usize) -> Line {
    Line(-(history as i32) + k as i32)
}

/// Ask the CPU to bring the cells `Row::reset` is about to rewrite into the cache.
pub(super) fn warm(grid: &Grid<Cell>, history: usize, plan: &Plan) {
    for k in 0..plan.rows {
        let cells = &grid[nth_oldest(history, k)][..];
        let start = cells.as_ptr().cast::<u8>();
        let lines = plan.lines.min(size_of_val(cells).div_ceil(CACHE_LINE));
        for l in 0..lines {
            prefetch(start.wrapping_add(l * CACHE_LINE));
        }
    }
}

/// A hint, never a fault: the CPU drops a prefetch it cannot use.
#[inline(always)]
fn prefetch(p: *const u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: a prefetch reads nothing the program can observe and cannot fault on any address.
    unsafe {
        core::arch::x86_64::_mm_prefetch::<{ core::arch::x86_64::_MM_HINT_T0 }>(p.cast());
    }
    #[cfg(target_arch = "aarch64")]
    // SAFETY: `prfm` is a hint with no architectural effect and cannot fault.
    unsafe {
        core::arch::asm!("prfm pldl1keep, [{0}]", in(reg) p, options(nostack, preserves_flags, readonly));
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = p;
}

#[cfg(test)]
mod tests {
    use super::super::Vt;
    use super::*;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::Column;

    const COLS: usize = 120;

    fn lines_of(width: usize, count: usize) -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..count {
            v.extend(std::iter::repeat_n(b'x', width - 2));
            v.extend(b"\r\n");
        }
        v
    }

    #[test]
    fn nothing_is_recycled_while_the_history_is_still_growing() {
        assert_eq!(plan(9_999, 10_000, COLS, &lines_of(98, 100)), None);
        assert_eq!(plan(0, 0, COLS, &lines_of(98, 100)), None, "history off");
    }

    #[test]
    fn bytes_without_a_line_feed_scroll_nothing() {
        assert_eq!(plan(10_000, 10_000, COLS, &[b'x'; 4096]), None);
        // A full-screen repaint addresses rows with CUP and never scrolls.
        assert_eq!(plan(10_000, 10_000, COLS, b"\x1b[1;1Hab\x1b[2;1Hcd"), None);
    }

    #[test]
    fn one_row_per_line_feed_one_row_ahead_and_the_cache_lines_of_a_line() {
        // Prose of 98 bytes: 98 cells = 2352 bytes = 37 cache lines (+1 for a row not starting on one).
        let p = plan(10_000, 10_000, COLS, &lines_of(98, 40)).unwrap();
        assert_eq!(p, Plan { rows: 40 + AHEAD, lines: 38 });
        // 300-byte lines wrap at 120 cells: a row is at most 120 cells = 2880 bytes = 45 cache lines (+1).
        let p = plan(10_000, 10_000, COLS, &lines_of(300, 10)).unwrap();
        assert_eq!(p.lines, 46);
    }

    #[test]
    fn short_lines_are_left_to_the_hardware_prefetcher() {
        // `seq`: 8 cells = 192 bytes a row.
        assert_eq!(plan(10_000, 10_000, COLS, &lines_of(8, 512)), None);
        // 42 cells = 1008 bytes: still under; 43 cells (1032) fetches.
        assert_eq!(plan(10_000, 10_000, COLS, &lines_of(42, 50)), None);
        assert!(plan(10_000, 10_000, COLS, &lines_of(43, 50)).is_some());
    }

    #[test]
    fn never_more_rows_than_the_history_has_or_the_burst_limit() {
        assert_eq!(plan(30, 30, COLS, &lines_of(98, 512)).unwrap().rows, 30);
        assert_eq!(plan(10_000, 10_000, COLS, &lines_of(98, 5_000)).unwrap().rows, MAX_ROWS);
    }

    #[test]
    fn slices_cover_the_bytes_once_and_end_on_a_line_feed_past_the_minimum() {
        let text = lines_of(98, 7);
        let s: Vec<_> = slices(&text).collect();
        assert_eq!(s.concat(), text);
        assert_eq!(s.len(), 7, "one slice per 98-byte line");
        assert!(s.iter().all(|p| p.ends_with(b"\n")));
        // Short lines are grouped until MIN_SLICE bytes have gone by.
        let short = lines_of(8, 40);
        let s: Vec<_> = slices(&short).collect();
        assert_eq!(s.concat(), short);
        assert!(s[..s.len() - 1].iter().all(|p| p.len() >= MIN_SLICE && p.ends_with(b"\n")));
        // No line feed: one slice. Nothing: none.
        assert_eq!(slices(&[b'x'; 1000]).count(), 1);
        assert_eq!(slices(b"").count(), 0);
        // A line feed inside the first MIN_SLICE bytes does not cut.
        assert_eq!(slices(b"ab\ncd").count(), 1);
    }

    // The premise of the whole module, pinned against the real alacritty: a scroll recycles the oldest history
    // row (the same allocation, not a copy) and the next scroll the one after it. If alacritty's ring changed,
    // `warm` would fetch rows nobody resets and this test would say so. It also pins that `Vt::feed` settles
    // the ring (drops alacritty's spare rows) before it relies on that.
    #[test]
    fn a_scroll_recycles_the_oldest_history_rows_in_order() {
        let vt = Vt::new(10, 4, |_| {});
        vt.set_options(20, "block", false);
        vt.feed(&(0..30).map(|i| format!("line{i}\r\n")).collect::<String>().into_bytes());
        let history = vt.term.lock().grid().history_size();
        assert_eq!(history, 20, "the scrollback is full");
        // The next chunk settles the ring (no spare rows), as it does in a flood.
        vt.feed(b"\r\n");
        let t = vt.term.lock();
        let cells = |t: &alacritty_terminal::term::Term<super::super::Proxy>, line: Line| t.grid()[line][..].as_ptr();
        let oldest: Vec<_> = (0..3).map(|k| cells(&t, nth_oldest(history, k))).collect();
        drop(t);
        vt.feed(b"\r\n\r\n\r\n");
        let t = vt.term.lock();
        assert_eq!(t.grid().history_size(), history);
        // The cursor sat on the last row (Line 3): three scrolls leave the recycled rows at Line 1, 2, 3.
        let recycled: Vec<_> = (1..=3).map(|l| cells(&t, Line(l))).collect();
        assert_eq!(recycled, oldest);
        assert_eq!(t.grid()[Line(3)][Column(0)].c, ' ', "and they were reset");
    }

    /// Everything the grid holds, history first: one string per row (text and the colours that made it).
    fn dump(vt: &Vt) -> Vec<String> {
        let t = vt.term.lock();
        let first = -(t.grid().history_size() as i32);
        (first..t.screen_lines() as i32)
            .map(|l| {
                (0..t.columns())
                    .map(|c| {
                        let cell = &t.grid()[Line(l)][Column(c)];
                        format!("{}{:?}", cell.c, cell.fg)
                    })
                    .collect::<String>()
            })
            .collect()
    }

    // Fetching is invisible: a flood fed in big chunks (sliced, with fetches) leaves exactly the grid that the same
    // bytes fed a byte at a time (never sliced, never fetched) leave: every row of history and screen.
    #[test]
    fn fetching_rows_does_not_change_what_the_grid_holds() {
        let mut flood = Vec::new();
        for i in 0..1_500 {
            // Prose lines of 80-110 bytes, a colour change in some, one long enough to wrap.
            let n = 80 + (i * 7) % 31;
            if i % 5 == 0 {
                flood.extend(format!("\x1b[3{}m", i % 7).bytes());
            }
            flood.extend((0..n).map(|k| b'a' + ((i + k) % 26) as u8));
            flood.extend(b"\r\n");
            if i % 400 == 0 {
                flood.extend(std::iter::repeat_n(b'W', 200));
                flood.extend(b"\r\n");
            }
        }
        let fast = Vt::new(100, 10, |_| {});
        fast.set_options(300, "block", false);
        for chunk in flood.chunks(4_000) {
            fast.feed(chunk);
        }
        let slow = Vt::new(100, 10, |_| {});
        slow.set_options(300, "block", false);
        for b in &flood {
            slow.feed(std::slice::from_ref(b));
        }
        assert_eq!(fast.term.lock().grid().history_size(), 300, "the history was full for most of the flood");
        assert_eq!(dump(&fast), dump(&slow));
    }
}
