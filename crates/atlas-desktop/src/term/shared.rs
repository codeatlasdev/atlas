//! State shared between a terminal's reader task, its render thread and the Tauri commands.

use std::time::{Duration, Instant};

use atlas_term::Stats;
use atlas_term::Vt;
use atlas_term::colors::Palette;
use parking_lot::{Condvar, Mutex};

pub(super) struct State {
    /// Pending physical size for the render thread.
    pub(super) size: Option<(u32, u32)>,
    pub(super) dirty: bool,
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
}

impl Shared {
    /// Ask the render thread for a new frame.
    pub(super) fn redraw(&self) {
        self.update(|s| s.dirty = true);
    }

    pub(super) fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.lock());
        self.wake.notify_one();
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
        self.rect = Some(rect);
        self.shown = show;
        self.frame()
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
mod tests {
    use super::*;

    const R: (f64, f64, f64, f64) = (10.0, 20.0, 300.0, 200.0);

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
        assert_eq!(p.request(R, true), Some(R));
    }

    #[test]
    fn after_the_first_frame_requests_apply_at_once_and_parking_still_works() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        assert_eq!(p.request(R, false), None);
        assert_eq!(p.request(R, true), Some(R));
    }
}
