//! The wheel of a GPU terminal: one command per page event (`term_wheel`), Ghostty's accumulator and routing in
//! `atlas_term::wheel`, applied here (one write to the program, or a scroll of the scrollback plus the sub-row
//! offset the renderer draws). A short silence after the last event snaps the offset to the nearest row.

use std::sync::Arc;
use std::time::{Duration, Instant};

use atlas_term::wheel::{Delta, Route};

use super::shared::Shared;
use super::with_term;

/// Silence after the last wheel event (fingers up and momentum over) before the offset snaps to a row.
pub(super) const SETTLE: Duration = Duration::from_millis(110);

/// What the page sends for one `wheel` event. `rows` is the delta in rows of the page's own cell height (the page
/// knows its CSS cell size, the renderer its physical one): positive = up. `precise` = a trackpad (pixel deltas).
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct WheelArgs {
    pub rows: f64,
    pub precise: bool,
    /// The cell under the pointer (0-based), for reports to a program that owns the mouse.
    pub col: u16,
    pub row: u16,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub readonly: bool,
}

impl Shared {
    /// Apply one wheel event. Returns whether a settle must be scheduled (an offset is on screen).
    pub(super) fn wheel_event(&self, a: &WheelArgs) -> bool {
        let cell_h = f64::from(self.state.lock().cell[1]).max(1.0);
        let mode = self.vt.mode();
        let at = self.vt.scrollbar();
        let out = self.wheel.lock().event(
            // Rows of the page's cell → pixels of the renderer's: both are "one cell tall".
            Delta {
                px: if a.precise { a.rows * cell_h } else { a.rows },
                precise: a.precise,
            },
            cell_h,
            mode,
            at,
            a.shift,
            a.readonly,
            (a.col, a.row),
            (a.alt, a.ctrl),
        );
        match out.route {
            Route::Write(bytes) => self.write(&bytes),
            Route::Scroll(n) => self.vt.scroll(n),
            Route::None => {}
        }
        self.update(|s| {
            s.scroll_frac = out.frac as f32;
            s.dirty = true;
        });
        out.frac != 0.0
    }

    /// The gesture is over: snap the offset to the nearest row.
    pub(super) fn wheel_settle(&self) {
        let cell_h = f64::from(self.state.lock().cell[1]).max(1.0);
        let rows = self.wheel.lock().settle(cell_h);
        if rows != 0 {
            self.vt.scroll(rows);
        }
        self.update(|s| {
            s.scroll_frac = 0.0;
            s.dirty = true;
        });
    }

    /// A key, new output at the bottom or a resize: whatever offset is on screen is dropped.
    pub(super) fn wheel_reset(&self) {
        self.wheel.lock().reset();
        let had = std::mem::take(&mut self.state.lock().scroll_frac) != 0.0;
        if had {
            self.redraw();
        }
    }
}

/// One wheel event of the page (trackpad or wheel). Returns nothing: the effect is a write to the program, a
/// scroll of the scrollback, or both nothing and a smaller offset.
#[tauri::command]
pub fn term_wheel(tab: String, e: WheelArgs) -> Result<(), String> {
    let sh = with_term(&tab)?;
    let stamp = Instant::now();
    sh.last_wheel.lock().replace(stamp);
    if sh.wheel_event(&e) {
        schedule_settle(sh, stamp);
    }
    Ok(())
}

/// After `SETTLE` of silence, snap. Every event re-arms by replacing the stamp: only the task whose stamp is
/// still the latest acts, so a continuing gesture never snaps mid-way.
fn schedule_settle(sh: Arc<Shared>, stamp: Instant) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        if *sh.last_wheel.lock() == Some(stamp) {
            sh.wheel_settle();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_scrollback(sh: &Shared) {
        let mut b = Vec::new();
        for i in 0..60 {
            b.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        sh.vt.feed(&b);
    }

    fn args(rows: f64) -> WheelArgs {
        WheelArgs {
            rows,
            precise: true,
            col: 0,
            row: 0,
            shift: false,
            alt: false,
            ctrl: false,
            readonly: false,
        }
    }

    fn rig() -> Arc<Shared> {
        let sh = Shared::for_test(20, 4);
        sh.state.lock().cell = [16.0, 32.0];
        feed_scrollback(&sh);
        sh
    }

    // Incident: a trackpad's small deltas never reached the scrollback (each was rounded to zero), and the
    // renderer had no way to show anything between two rows.
    #[test]
    fn small_trackpad_deltas_scroll_the_scrollback_and_leave_a_sub_row_offset() {
        let sh = rig();
        assert!(
            !sh.wheel_event(&args(0.3)),
            "the live bottom has no row to show below the last: no offset, no settle"
        );
        assert_eq!(sh.vt.scrollbar().0, 0);
        sh.vt.scroll(10);
        sh.wheel.lock().reset();
        let mut moved = 0;
        for _ in 0..7 {
            sh.wheel_event(&args(0.3));
            moved = sh.vt.scrollbar().0 - 10;
        }
        assert_eq!(moved, 2, "7 × 0.3 = 2.1 rows");
        let f = sh.state.lock().scroll_frac;
        assert!((f - 0.1).abs() < 1e-3, "{f}");
    }

    #[test]
    fn settling_snaps_to_the_nearest_row_and_clears_the_offset() {
        let sh = rig();
        sh.vt.scroll(10);
        sh.wheel_event(&args(0.7));
        assert!(sh.state.lock().scroll_frac > 0.0);
        sh.wheel_settle();
        assert_eq!(sh.state.lock().scroll_frac, 0.0);
        assert_eq!(sh.vt.scrollbar().0, 10 + 1, "0.7 of a row rounds to one");
    }

    #[test]
    fn a_program_that_owns_the_mouse_gets_one_write_and_never_an_offset() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sh = Arc::new(Shared::with_writer(20, 4, tx));
        sh.state.lock().cell = [16.0, 32.0];
        feed_scrollback(&sh);
        sh.vt.feed(b"\x1b[?1000h\x1b[?1006h");
        let mut a = args(3.0);
        a.col = 4;
        a.row = 2;
        assert!(!sh.wheel_event(&a));
        let sent: Vec<Vec<u8>> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            sent,
            [b"\x1b[<64;5;3M".repeat(3)],
            "one write, three reports"
        );
        assert_eq!(sh.state.lock().scroll_frac, 0.0);
        assert_eq!(sh.vt.scrollbar().0, 0, "the viewport did not move");
    }

    #[test]
    fn a_reset_drops_the_offset_and_the_remainder() {
        let sh = rig();
        sh.vt.scroll(10);
        sh.wheel_event(&args(0.4));
        sh.wheel_reset();
        assert_eq!(sh.state.lock().scroll_frac, 0.0);
        sh.wheel_event(&args(0.4));
        assert!(
            (sh.state.lock().scroll_frac - 0.4).abs() < 1e-3,
            "no leftover from before the reset"
        );
    }

    // Contract: `apps/desktop/src/lib/__fixtures__/term-wheel-args.json` is what `wheelArgs` (page side) really
    // produces; `wheel.test.ts` pins the producer to it and this deserializes it with the real consumer.
    #[test]
    fn wheel_args_fixture_is_what_the_page_sends() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/term-wheel-args.json");
        let a: WheelArgs = serde_json::from_str(&std::fs::read_to_string(path).unwrap())
            .expect("the page's payload must deserialize");
        assert_eq!((a.rows, a.precise, a.col, a.row), (1.5, true, 4, 2));
        assert!(a.shift && !a.alt && !a.ctrl && !a.readonly);
    }
}
