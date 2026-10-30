//! State shared between a terminal's reader task, its render thread and the Tauri commands.

#[cfg(test)]
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use atlas_term::Stats;
use atlas_term::Vt;
use atlas_term::colors::Palette;
use parking_lot::{Condvar, Mutex};

use super::watchdog::Why;

pub(super) struct State {
    /// Pending physical size for the render thread.
    pub(super) size: Option<(u32, u32)>,
    pub(super) dirty: bool,
    /// The render thread must replace its surface and renderer (wake, display change, "Reload
    /// terminal"); the first reason asked wins until it has done so.
    pub(super) rebuild: Option<Why>,
    pub(super) reset: bool,
    pub(super) closed: bool,
    pub(super) stats: Stats,
    /// Cell size in physical pixels, from the renderer.
    pub(super) cell: [f32; 2],
    /// Keyboard focus is in this terminal's input sink.
    pub(super) focused: bool,
    /// Visual bell: tint the frame until then.
    pub(super) bell_until: Option<Instant>,
    /// Theme colors; `term_recolor` swaps them and sets `pal_new` for the render thread.
    pub(super) pal: Palette,
    pub(super) pal_new: bool,
    /// Pending font size (pt) for the render thread (`term_set_prefs`).
    pub(super) font_pt: Option<f32>,
    /// Pending ligature setting for the renderer.
    pub(super) ligatures: Option<bool>,
    /// Font size (pt) the terminal is at, or will be once `font_pt` is applied.
    pub(super) font_cur: f32,
    /// Pending font request (`term_set_prefs`) for the render thread.
    pub(super) font_new: Option<atlas_term::FontSpec>,
    /// The font request the terminal is at, or will be once `font_new` is applied.
    pub(super) font_spec: atlas_term::FontSpec,
    /// Rows (-1..1) the content is drawn below its row grid: the sub-row offset of a smooth scroll
    /// (`wheel::term_wheel`). The render thread asks the snapshot for overscan rows while it is not 0.
    pub(super) scroll_frac: f32,
}

pub(super) const BLINK: Duration = Duration::from_millis(530);
pub(super) const BELL: Duration = Duration::from_millis(120);

pub(super) struct Shared {
    pub(super) vt: Vt,
    pub(super) state: Mutex<State>,
    /// OSC 52 policy (`allow` | `ask` | `deny`), live-updatable.
    pub(super) osc52: Mutex<String>,
    pub(super) wake: Condvar,
    /// Where the native view belongs, and whether its first frame is drawn yet.
    pub(super) placement: Mutex<Placement>,
    /// Retained `NSView*` the renderer draws into; touched only on the main thread. Here, not in
    /// `Term`, so the render thread can swap it when it rebuilds the surface without ever
    /// taking the `TERMS` registry lock (see `render_loop`).
    pub(super) view: AtomicUsize,
    /// Bytes for the pty, sent in order by one drain task (`super::drain`).
    pub(super) writer: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Wheel accumulator and sub-row offset (`atlas_term::wheel`).
    pub(super) wheel: Mutex<atlas_term::wheel::Wheel>,
    /// When the last wheel event arrived: the settle task acts only if it is still the latest.
    pub(super) last_wheel: Mutex<Option<Instant>>,
}

/// hostd drops a request line over 1 MiB; base64 grows a chunk by 4/3.
const WRITE_CHUNK: usize = 512 * 1024;

impl Shared {
    pub(super) fn view(&self) -> usize {
        self.view.load(Ordering::Acquire)
    }

    pub(super) fn set_view(&self, v: usize) -> usize {
        self.view.swap(v, Ordering::AcqRel)
    }

    /// Ask the render thread for a new frame.
    pub(super) fn redraw(&self) {
        self.update(|s| s.dirty = true);
    }

    /// Ask the render thread to replace its surface and renderer, keeping the screen and scrollback.
    pub(super) fn rebuild(&self, why: Why) {
        self.update(|s| {
            s.rebuild.get_or_insert(why);
        });
    }

    pub(super) fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.lock());
        self.wake.notify_one();
    }

    /// Queue bytes for the pty.
    pub(super) fn write(&self, bytes: &[u8]) {
        for c in bytes.chunks(WRITE_CHUNK) {
            let _ = self.writer.send(c.to_vec());
        }
    }

    /// User input (keys, paste): back to the live screen first (Ghostty
    /// `scroll-to-bottom = keystroke`); output alone never moves the viewport (#97).
    pub(super) fn input(&self, bytes: &[u8]) {
        self.wheel_reset();
        if self.vt.scrollbar().0 != 0 {
            self.vt.scroll_to_bottom();
            self.redraw();
        }
        self.write(bytes);
    }
}

/// Where the native view belongs and whether it may be shown yet.
///
/// A new `CAMetalLayer` is empty until the render thread presents its first frame, so a view placed over the
/// pane at once shows the webview through it for a frame (a flash). The view therefore stays parked off screen
/// until `drawn()`; `term_bounds` calls in between only record where it belongs.
#[derive(Default)]
pub(super) struct Placement {
    rect: Option<(f64, f64, f64, f64)>,
    shown: bool,
    drawn: bool,
}

impl Placement {
    /// JS asked for this rect; `show: false` parks it (dialog open, space moving). Returns the frame to apply
    /// to the view: the rect, or `None` to park it off screen.
    pub(super) fn request(
        &mut self,
        rect: (f64, f64, f64, f64),
        show: bool,
    ) -> Option<(f64, f64, f64, f64)> {
        // Back from parked: the layer holds the picture from before (a parked view does not draw), so it stays off
        // screen until a fresh frame is on it (`drawn`). Showing it at once flashed the stale picture.
        if show && !self.shown {
            self.drawn = false;
        }
        self.rect = Some(rect);
        self.shown = show;
        self.frame()
    }

    /// Shown but waiting for a fresh frame: the render loop must draw one now and then call `drawn`.
    pub(super) fn awaiting(&self) -> bool {
        self.shown && !self.drawn
    }

    /// The page wants the view on screen (as opposed to parked under a dialog or in a hidden tab),
    /// whether or not its first frame is drawn yet.
    pub(super) fn is_shown(&self) -> bool {
        self.shown
    }

    /// A new view replaced the old one: it is empty until its first frame, so it is parked again.
    pub(super) fn parked(&mut self) {
        self.drawn = false;
    }

    /// The first frame is on the layer. Returns the frame to apply now (`None`: still parked).
    pub(super) fn drawn(&mut self) -> Option<(f64, f64, f64, f64)> {
        self.drawn = true;
        self.frame()
    }

    fn frame(&self) -> Option<(f64, f64, f64, f64)> {
        self.rect.filter(|_| self.shown && self.drawn)
    }
}

#[cfg(test)]
impl Shared {
    /// A terminal with no pty, GPU or app: what the render-loop tests drive.
    pub(super) fn for_test(cols: u16, rows: u16) -> Arc<Shared> {
        let (writer, _rx) = tokio::sync::mpsc::unbounded_channel();
        Arc::new(Self::with_writer(cols, rows, writer))
    }

    /// Like `for_test`, with the channel the pty writes go to, so a test can read what the program was sent.
    pub(super) fn with_writer(
        cols: u16,
        rows: u16,
        writer: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Shared {
        Shared {
            vt: Vt::new(cols, rows, |_| {}),
            state: Mutex::new(State {
                size: None,
                dirty: false,
                rebuild: None,
                reset: false,
                closed: false,
                stats: Stats::default(),
                cell: [0.0; 2],
                focused: false,
                bell_until: None,
                pal: Palette::new(
                    atlas_term::colors::parse_hex("#c8c8c8").unwrap(),
                    atlas_term::colors::parse_hex("#0a0a0a").unwrap(),
                    None,
                ),
                pal_new: false,
                font_pt: None,
                ligatures: None,
                font_cur: 12.5,
                font_new: None,
                font_spec: Default::default(),
                scroll_frac: 0.0,
            }),
            osc52: Mutex::new("ask".into()),
            wake: Condvar::new(),
            placement: Mutex::new(Placement::default()),
            view: AtomicUsize::new(0),
            writer,
            wheel: Mutex::default(),
            last_wheel: Mutex::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: (f64, f64, f64, f64) = (10.0, 20.0, 300.0, 200.0);

    // A wake and a display change arriving together are one rebuild, attributed to the first.
    #[test]
    fn rebuild_requests_coalesce_and_keep_the_first_reason() {
        let sh = Shared::for_test(10, 2);
        sh.rebuild(Why::Wake);
        sh.rebuild(Why::Screens);
        assert_eq!(sh.state.lock().rebuild, Some(Why::Wake));
        assert_eq!(sh.state.lock().rebuild.take(), Some(Why::Wake));
        sh.rebuild(Why::Manual);
        assert_eq!(sh.state.lock().rebuild, Some(Why::Manual));
    }

    #[test]
    fn a_rebuilt_view_is_parked_again_until_its_first_frame_but_stays_wanted() {
        let mut p = Placement::default();
        p.request(R, true);
        assert_eq!(p.drawn(), Some(R));
        p.parked();
        assert!(p.is_shown(), "the page still wants it on screen");
        assert_eq!(p.request(R, true), None, "blank layer stays off screen");
        assert_eq!(p.drawn(), Some(R));
    }

    /// Incident (desktop flicker): the native view was placed over the pane the moment it was created, before its
    /// first frame, so the empty layer flashed over the terminal.
    #[test]
    fn a_new_view_stays_parked_until_its_first_frame_is_drawn() {
        let mut p = Placement::default();
        assert_eq!(p.request(R, true), None);
        assert_eq!(p.drawn(), Some(R));
    }

    #[test]
    fn rect_changes_before_the_first_frame_are_remembered_and_the_last_one_wins() {
        let mut p = Placement::default();
        let moved = (10.0, 20.0, 400.0, 220.0);
        assert_eq!(p.request(R, true), None);
        assert_eq!(p.request(moved, true), None);
        assert_eq!(p.drawn(), Some(moved));
    }

    #[test]
    fn parked_by_js_stays_parked_even_after_the_first_frame() {
        let mut p = Placement::default();
        assert_eq!(p.request(R, false), None);
        assert_eq!(p.drawn(), None);
        assert_eq!(
            p.request(R, true),
            None,
            "shown again: waits for a fresh frame"
        );
        assert_eq!(p.drawn(), Some(R));
    }

    #[test]
    fn while_shown_requests_apply_at_once_and_parking_still_works() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        let moved = (10.0, 20.0, 400.0, 220.0);
        assert_eq!(
            p.request(moved, true),
            Some(moved),
            "a move while shown is immediate"
        );
        assert_eq!(p.request(R, false), None);
    }

    // Incident (2026-10-26, ghost frames switching tabs): a parked terminal does not draw, so its layer still holds
    // the picture from before it was parked. Shown again at once, it flashed that stale picture until the next
    // frame. Back from parked, a view waits (off screen, the slot's own colour shows) until a fresh frame is drawn.
    #[test]
    fn a_view_shown_again_waits_for_a_fresh_frame() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        p.request(R, false);
        assert_eq!(p.request(R, true), None);
        assert!(p.awaiting(), "the render loop must draw one frame now");
        assert_eq!(p.drawn(), Some(R));
        assert!(!p.awaiting());
    }
}
