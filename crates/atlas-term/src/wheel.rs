//! Wheel and trackpad scrolling, as Ghostty does it (`Surface.scrollCallback`): one pixel accumulator for both
//! kinds of device, then one route per event. Pure: the pixel delta and the modes in, what to do out.
//!
//! * Precise devices (trackpads, Magic Mouse) report pixels; a notch wheel reports lines, which count as
//!   `DISCRETE_ROWS` rows of pixels each (Ghostty `mouse-scroll-multiplier.discrete`, default 3).
//! * The remainder under one row carries to the next event, so a slow drag adds up and nothing is lost;
//!   a change of direction drops it (reversing never needs to "pay back" the old remainder).
//! * Route: the alternate screen with DECSET 1007 and no mouse mode gets arrow keys; a program that asked for
//!   mouse reports gets wheel reports (⇧ gives the wheel to the terminal); anything else scrolls the scrollback.
//! * A route is ONE value per event: all the reports of an event go out in one write, in order.

use alacritty_terminal::term::TermMode;

use crate::mouse::{self, Action, Event};

/// Rows one notch of a non-precise wheel moves (Ghostty's default multiplier).
pub const DISCRETE_ROWS: f64 = 3.0;
/// Most rows one event moves: a fling can report hundreds, and no program wants them in one burst.
pub const MAX_ROWS: u32 = 50;

/// Pixels (positive = up, away from the user) of one wheel event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub px: f64,
    /// A trackpad-like device: `px` is real pixels. Otherwise `px` is in lines (a notch is ±1).
    pub precise: bool,
}

/// The sub-row remainder between events.
#[derive(Default, Debug)]
pub struct Accum {
    rest: f64,
}

impl Accum {
    /// Whole rows (positive = up) for this event; the fraction stays for the next. A change of direction drops
    /// the remainder: whole-row scrolling never needs to pay back the old direction.
    pub fn take(&mut self, d: Delta, cell_h: f64) -> i32 {
        if d.px * self.rest < 0.0 {
            self.rest = 0.0;
        }
        self.add(d, cell_h)
    }

    /// Like `take` but the remainder carries through a change of direction: while a sub-row offset is on screen
    /// the content must follow the fingers, not jump back to the row grid.
    fn add(&mut self, d: Delta, cell_h: f64) -> i32 {
        let cell_h = cell_h.max(1.0);
        let px = if d.precise {
            d.px
        } else {
            d.px * cell_h * DISCRETE_ROWS
        };
        self.rest += px;
        let rows = (self.rest / cell_h).trunc();
        self.rest -= rows * cell_h;
        rows as i32
    }
}

/// What one wheel event does.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// Nothing moved (the remainder is under one row).
    None,
    /// Write these bytes to the program: arrow keys, or wheel reports.
    Write(Vec<u8>),
    /// Scroll the viewport by this many lines (positive = up into history).
    Scroll(i32),
}

/// Where `rows` (positive = up) go. `col`/`row` is the cell under the pointer (0-based) for the reports.
/// `shift` is ⇧ held: the terminal keeps the wheel. `readonly`: a viewer that never sends to the program.
pub fn route(
    rows: i32,
    mode: TermMode,
    shift: bool,
    readonly: bool,
    col: u16,
    row: u16,
    mods: (bool, bool),
) -> Route {
    if rows == 0 {
        return Route::None;
    }
    let n = rows.unsigned_abs().min(MAX_ROWS) as usize;
    if !readonly {
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !mode.intersects(TermMode::MOUSE_MODE)
            && !shift
        {
            let arrow: &[u8] = match (rows > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, false) => b"\x1b[A",
                (true, true) => b"\x1bOA",
                (false, false) => b"\x1b[B",
                (false, true) => b"\x1bOB",
            };
            return Route::Write(arrow.repeat(n));
        }
        if mouse::captures(mode, shift) {
            let ev = Event {
                action: if rows > 0 {
                    Action::WheelUp
                } else {
                    Action::WheelDown
                },
                button: None,
                shift,
                alt: mods.0,
                ctrl: mods.1,
                col,
                row,
                px: None,
            };
            return match mouse::report(&ev, mode, false) {
                Some(one) => Route::Write(one.repeat(n)),
                None => Route::None,
            };
        }
    }
    Route::Scroll(rows)
}

/// The wheel of one terminal: the accumulator, and the sub-row offset the renderer draws while the terminal's
/// own scrollback scrolls (Kitty `pixel_scroll`, Ghostty overscan #14400). Never for a program that has the
/// wheel (alternate screen, mouse reports): they get `Route::Write` and the offset is 0.
#[derive(Default, Debug)]
pub struct Wheel {
    acc: Accum,
}

/// One event's outcome.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub route: Route,
    /// Rows (-1..1) the content is shifted by below its row grid: positive = down (older lines coming in from the
    /// top). 0 when the terminal does not scroll its own scrollback with this event, or at either end of it.
    pub frac: f64,
}

impl Wheel {
    /// One event. `at` is `(offset, max)`: the viewport's distance from the bottom and the scrollback length, in
    /// rows. `cell` is the cell under the pointer (for reports) and `mods` is (⌥, ⌃).
    #[allow(clippy::too_many_arguments)]
    pub fn event(
        &mut self,
        d: Delta,
        cell_h: f64,
        mode: TermMode,
        (offset, max): (u32, u32),
        shift: bool,
        readonly: bool,
        cell: (u16, u16),
        mods: (bool, bool),
    ) -> Outcome {
        let owned = !readonly
            && (mouse::captures(mode, shift)
                || (mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !shift));
        if owned {
            // The program has the wheel: whole rows (a flip drops the remainder, as Ghostty), no offset ever.
            let rows = self.acc.take(d, cell_h);
            return Outcome {
                route: route(rows, mode, shift, readonly, cell.0, cell.1, mods),
                frac: 0.0,
            };
        }
        let rows = self.acc.add(d, cell_h);
        let route = route(rows, mode, shift, readonly, cell.0, cell.1, mods);
        // Where the viewport is once `rows` are applied: an offset only exists strictly inside the scrollback.
        let after = (i64::from(offset) + i64::from(rows)).clamp(0, i64::from(max));
        if after == 0 || after == i64::from(max) {
            self.acc.rest = 0.0;
            return Outcome { route, frac: 0.0 };
        }
        Outcome {
            route,
            frac: self.acc.rest / cell_h.max(1.0),
        }
    }

    /// The fingers and the momentum are done: snap the remainder to the nearest row. Returns the rows to scroll
    /// (0 or ±1); the offset is 0 afterwards.
    pub fn settle(&mut self, cell_h: f64) -> i32 {
        let r = (self.acc.rest / cell_h.max(1.0)).round() as i32;
        self.acc.rest = 0.0;
        r
    }

    /// Drop the remainder (a key, new output at the bottom).
    pub fn reset(&mut self) {
        self.acc.rest = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 20.0;
    const PRIMARY: TermMode = TermMode::ALTERNATE_SCROLL;

    fn pixels(px: f64) -> Delta {
        Delta { px, precise: true }
    }
    fn notches(n: f64) -> Delta {
        Delta {
            px: n,
            precise: false,
        }
    }

    // Incident: the trackpad's many small deltas were rounded one by one (never scrolled, or over-scrolled).
    #[test]
    fn small_trackpad_deltas_add_up_to_whole_rows_and_keep_the_rest() {
        let mut a = Accum::default();
        let got: Vec<i32> = (0..10).map(|_| a.take(pixels(6.0), H)).collect();
        assert_eq!(got, [0, 0, 0, 1, 0, 0, 1, 0, 0, 1]);
        // 60 px in total = exactly 3 rows: nothing lost, nothing invented.
        assert_eq!(got.iter().sum::<i32>(), 3);
    }

    #[test]
    fn a_big_delta_is_many_rows_and_the_fraction_stays() {
        let mut a = Accum::default();
        assert_eq!(a.take(pixels(95.0), H), 4);
        assert_eq!(
            a.take(pixels(5.0), H),
            1,
            "the 15 px left over complete a row"
        );
    }

    #[test]
    fn a_direction_flip_drops_the_remainder() {
        let mut a = Accum::default();
        assert_eq!(a.take(pixels(15.0), H), 0);
        // Reversing: the 15 px of "up" would otherwise swallow the first 15 px of "down".
        assert_eq!(a.take(pixels(-12.0), H), 0);
        assert_eq!(a.take(pixels(-12.0), H), -1);
    }

    // Ghostty: a notch is 3 rows of pixels, through the same accumulator as a trackpad.
    #[test]
    fn a_wheel_notch_is_three_rows() {
        let mut a = Accum::default();
        assert_eq!(a.take(notches(1.0), H), 3);
        assert_eq!(a.take(notches(-2.0), H), -6);
    }

    #[test]
    fn a_degenerate_cell_height_never_divides_by_zero() {
        let mut a = Accum::default();
        assert_eq!(a.take(pixels(3.0), 0.0), 3);
    }

    #[test]
    fn the_primary_screen_scrolls_its_scrollback() {
        assert_eq!(
            route(4, PRIMARY, false, false, 3, 3, (false, false)),
            Route::Scroll(4)
        );
        assert_eq!(
            route(-2, PRIMARY, false, true, 3, 3, (false, false)),
            Route::Scroll(-2)
        );
        assert_eq!(
            route(0, PRIMARY, false, false, 3, 3, (false, false)),
            Route::None
        );
    }

    #[test]
    fn the_alternate_screen_gets_arrows_unless_the_program_owns_the_mouse() {
        let alt = PRIMARY | TermMode::ALT_SCREEN;
        assert_eq!(
            route(3, alt, false, false, 0, 0, (false, false)),
            Route::Write(b"\x1b[A\x1b[A\x1b[A".to_vec())
        );
        assert_eq!(
            route(
                -1,
                alt | TermMode::APP_CURSOR,
                false,
                false,
                0,
                0,
                (false, false)
            ),
            Route::Write(b"\x1bOB".to_vec())
        );
        // 1007 off: the program asked for nothing, and the alternate screen has no scrollback to move.
        assert_eq!(
            route(3, TermMode::ALT_SCREEN, false, false, 0, 0, (false, false)),
            Route::Scroll(3)
        );
        // A runaway fling is capped.
        let Route::Write(w) = route(500, alt, false, false, 0, 0, (false, false)) else {
            panic!("arrows")
        };
        assert_eq!(w.len(), 3 * MAX_ROWS as usize);
    }

    // Incident: a program with mouse mode on (omp's overlays, vim, tmux) got up to 20 separate IPC reports, each
    // after an awaited mode query. One event is one write now, and the reports are in the program's encoding.
    #[test]
    fn a_mouse_program_gets_all_the_reports_of_one_event_in_one_write() {
        let mode = PRIMARY | TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        assert_eq!(
            route(3, mode, false, false, 4, 2, (false, false)),
            Route::Write(b"\x1b[<64;5;3M\x1b[<64;5;3M\x1b[<64;5;3M".to_vec())
        );
        assert_eq!(
            route(-1, mode, false, false, 0, 0, (true, false)),
            Route::Write(b"\x1b[<73;1;1M".to_vec()),
            "⌥ is bit 8 of the button code"
        );
    }

    // The wheel never reaches the program's own scrollback behind its back: on the alternate screen WITH a mouse
    // mode the arrows are off (the program asked for the mouse), and ⇧ hands the wheel back to the terminal.
    #[test]
    fn shift_gives_the_wheel_back_and_a_mouse_program_never_gets_arrows() {
        let mode = PRIMARY | TermMode::ALT_SCREEN | TermMode::MOUSE_REPORT_CLICK;
        let Route::Write(w) = route(1, mode, false, false, 0, 0, (false, false)) else {
            panic!("report")
        };
        assert!(w.starts_with(b"\x1b[M"), "a report, not an arrow: {w:?}");
        assert_eq!(
            route(2, mode, true, false, 0, 0, (false, false)),
            Route::Scroll(2)
        );
    }

    #[test]
    fn a_read_only_terminal_never_writes_to_the_program() {
        let mode = PRIMARY | TermMode::ALT_SCREEN | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            route(2, mode, false, true, 0, 0, (false, false)),
            Route::Scroll(2)
        );
    }

    // A report past what the legacy encoding can address is dropped: scrolling the viewport instead would move
    // a program that owns the mouse.
    #[test]
    fn an_unaddressable_report_is_dropped_not_turned_into_a_scroll() {
        let mode = PRIMARY | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            route(1, mode, false, false, 400, 0, (false, false)),
            Route::None
        );
    }

    // ---- smooth (sub-row) scrolling of the terminal's own scrollback ----

    fn ev(w: &mut Wheel, px: f64, mode: TermMode, at: (u32, u32)) -> Outcome {
        w.event(
            pixels(px),
            H,
            mode,
            at,
            false,
            false,
            (0, 0),
            (false, false),
        )
    }

    // Kitty `pixel_scroll` / Ghostty overscan: between whole rows the content sits at the pixel position the
    // fingers are at; the remainder under one row is the offset, with the sign of the motion.
    #[test]
    fn inside_the_scrollback_the_remainder_is_drawn_as_a_sub_row_offset() {
        let mut w = Wheel::default();
        let o = ev(&mut w, 5.0, PRIMARY, (10, 100));
        assert_eq!(o.route, Route::None);
        assert!((o.frac - 0.25).abs() < 1e-9, "{}", o.frac);
        let o = ev(&mut w, 20.0, PRIMARY, (10, 100));
        assert_eq!(o.route, Route::Scroll(1));
        assert!(
            (o.frac - 0.25).abs() < 1e-9,
            "5 + 20 px = one row and a quarter: {}",
            o.frac
        );
    }

    // No row jumps: after every event, rows moved + the offset equal the pixels travelled.
    #[test]
    fn rows_and_offset_always_add_up_to_the_pixels_travelled() {
        let mut w = Wheel::default();
        let (mut rows, mut travelled) = (0, 0.0);
        let mut offset = 50u32;
        for px in [3.0, 7.0, 11.0, 2.0, 19.0, 23.0, 1.0, 40.0, -6.0, -30.0, 9.0] {
            let o = ev(&mut w, px, PRIMARY, (offset, 1000));
            travelled += px;
            if let Route::Scroll(n) = o.route {
                rows += n;
                offset = (offset as i32 + n) as u32;
            }
            assert!(
                ((f64::from(rows) + o.frac) * H - travelled).abs() < 1e-6,
                "after {travelled} px: {rows} rows + {} offset",
                o.frac
            );
            assert!(o.frac.abs() < 1.0);
        }
    }

    // A fraction at either end of the scrollback would show a gap above the first line / below the last.
    #[test]
    fn at_either_end_of_the_scrollback_there_is_no_offset() {
        let at = |px: f64, pos: (u32, u32)| ev(&mut Wheel::default(), px, PRIMARY, pos).frac;
        assert_eq!(
            at(5.0, (0, 100)),
            0.0,
            "live bottom: a fraction would show a gap under the last line"
        );
        assert_eq!(at(-5.0, (0, 100)), 0.0);
        assert_eq!(at(5.0, (100, 100)), 0.0, "top of the scrollback");
        assert_eq!(at(25.0, (99, 100)), 0.0, "the next row reaches the top");
    }

    // Smooth scrolling is the terminal's own scrollback only: never a program that has the wheel.
    #[test]
    fn a_program_that_has_the_wheel_never_sees_an_offset() {
        for mode in [
            PRIMARY | TermMode::ALT_SCREEN,
            PRIMARY | TermMode::MOUSE_REPORT_CLICK,
        ] {
            let mut w = Wheel::default();
            let o = ev(&mut w, 45.0, mode, (10, 100));
            assert!(matches!(o.route, Route::Write(_)), "{mode:?}");
            assert_eq!(o.frac, 0.0);
        }
    }

    #[test]
    fn settle_snaps_to_the_nearest_row_and_clears_the_offset() {
        let mut w = Wheel::default();
        ev(&mut w, 12.0, PRIMARY, (10, 100));
        assert_eq!(w.settle(H), 1, "0.6 of a row rounds up");
        assert_eq!(w.settle(H), 0, "nothing left");
        ev(&mut w, 8.0, PRIMARY, (10, 100));
        assert_eq!(w.settle(H), 0, "0.4 of a row rounds down");
        ev(&mut w, -14.0, PRIMARY, (10, 100));
        assert_eq!(w.settle(H), -1);
    }

    // A direction change while an offset is on screen follows the fingers: no jump back to the row grid.
    #[test]
    fn reversing_direction_continues_from_the_current_position() {
        let mut w = Wheel::default();
        ev(&mut w, 12.0, PRIMARY, (10, 100));
        let o = ev(&mut w, -4.0, PRIMARY, (10, 100));
        assert_eq!(o.route, Route::None);
        assert!(
            (o.frac - 0.4).abs() < 1e-9,
            "12 - 4 = 8 px = 0.4 row: {}",
            o.frac
        );
    }
}
