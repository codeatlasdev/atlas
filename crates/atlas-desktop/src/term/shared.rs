//! State shared between a terminal's reader task, its render thread and the Tauri commands.

#[cfg(test)]
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use atlas_term::Stats;
use atlas_term::Vt;
use atlas_term::colors::Palette;
use parking_lot::{Condvar, Mutex};

use super::layering::{self, Frame, Target};
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
}

/// hostd drops a request line over 1 MiB; base64 grows a chunk by 4/3.
const WRITE_CHUNK: usize = 512 * 1024;

impl Shared {
    /// The terminal background as AppKit floats: what its view's layer shows before a first drawable
    /// and at any gap (never the desktop).
    pub(super) fn bg(&self) -> [f64; 3] {
        let c = self.state.lock().pal.base[atlas_term::colors::NamedColor::Background as usize];
        layering::bg_floats(c.r, c.g, c.b)
    }

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
        if self.vt.scrollbar().0 != 0 {
            self.vt.scroll_to_bottom();
            self.redraw();
        }
        self.write(bytes);
    }
}

/// Where the native view belongs and whether it may be shown yet.
///
/// A new `CAMetalLayer` is empty until the render thread presents its first frame. The view stays off screen
/// until `drawn()` (the slot paints the terminal colour meanwhile, see `GpuTerminal.svelte`), so the hole the
/// page opens over it never shows an empty layer. `term_bounds` calls in between only record where it belongs.
///
/// `shown` is a *state* of the tab (it is on screen), never a reaction to an overlay: a menu, dialog or peek is
/// drawn over the view by the page, and the view is not touched.
#[derive(Default)]
pub(super) struct Placement {
    rect: Option<Frame>,
    /// Last usable size: a parked view keeps it, so showing it again is a move, not a resize.
    last: Option<(f64, f64)>,
    shown: bool,
    drawn: bool,
}

impl Placement {
    /// JS asked for this rect; `show: false` parks it (a tab that is not on screen). Returns where the view goes.
    pub(super) fn request(&mut self, rect: Frame, show: bool) -> Target {
        if layering::usable(rect) {
            self.rect = Some(rect);
            self.last = Some((rect.2, rect.3));
        }
        self.shown = show;
        self.target()
    }

    /// The page wants the view on screen (as opposed to a hidden tab), whether or not its first frame
    /// is drawn yet.
    pub(super) fn is_shown(&self) -> bool {
        self.shown
    }

    /// Is the view on screen over its slot (shown, first frame drawn, usable rect)? The page opens its hole then.
    pub(super) fn is_presented(&self) -> bool {
        matches!(self.target(), Target::Over(_))
    }

    /// A new view replaced the old one: it is empty until its first frame, so it is parked again.
    pub(super) fn parked(&mut self) {
        self.drawn = false;
    }

    /// The first frame is on the layer. Returns where the view goes now.
    pub(super) fn drawn(&mut self) -> Target {
        self.drawn = true;
        self.target()
    }

    /// Where the view goes for the current state.
    pub(super) fn target(&self) -> Target {
        match self.rect {
            Some(r) if self.shown && self.drawn => layering::resolve(r, true, self.last),
            _ => {
                let (w, h) = self.last.unwrap_or((1.0, 1.0));
                Target::Park(w, h)
            }
        }
    }
}

#[cfg(test)]
impl Shared {
    /// A terminal with no pty, GPU or app: what the render-loop tests drive.
    pub(super) fn for_test(cols: u16, rows: u16) -> Arc<Shared> {
        let (writer, _rx) = tokio::sync::mpsc::unbounded_channel();
        Arc::new(Shared {
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
            }),
            osc52: Mutex::new("ask".into()),
            wake: Condvar::new(),
            placement: Mutex::new(Placement::default()),
            view: AtomicUsize::new(0),
            writer,
        })
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

    const PARK: Target = Target::Park(300.0, 200.0);

    #[test]
    fn a_rebuilt_view_is_parked_again_until_its_first_frame_but_stays_wanted() {
        let mut p = Placement::default();
        p.request(R, true);
        assert_eq!(p.drawn(), Target::Over(R));
        p.parked();
        assert!(p.is_shown(), "the page still wants it on screen");
        assert!(!p.is_presented(), "no hole while the new layer is blank");
        assert_eq!(p.request(R, true), PARK, "blank layer stays off screen");
        assert_eq!(p.drawn(), Target::Over(R));
        assert!(p.is_presented());
    }

    /// Incident (desktop flicker): the native view was placed over the pane the moment it was created, before its
    /// first frame, so the empty layer flashed over the terminal.
    #[test]
    fn a_new_view_stays_parked_until_its_first_frame_is_drawn() {
        let mut p = Placement::default();
        assert_eq!(p.request(R, true), PARK);
        assert_eq!(p.drawn(), Target::Over(R));
    }

    #[test]
    fn rect_changes_before_the_first_frame_are_remembered_and_the_last_one_wins() {
        let mut p = Placement::default();
        let moved = (10.0, 20.0, 400.0, 220.0);
        assert_eq!(p.request(R, true), PARK);
        assert_eq!(p.request(moved, true), Target::Park(400.0, 220.0));
        assert_eq!(p.drawn(), Target::Over(moved));
    }

    #[test]
    fn parked_by_js_stays_parked_even_after_the_first_frame() {
        let mut p = Placement::default();
        assert_eq!(p.request(R, false), PARK);
        assert_eq!(p.drawn(), PARK);
        assert_eq!(p.request(R, true), Target::Over(R));
    }

    #[test]
    fn after_the_first_frame_requests_apply_at_once_and_parking_still_works() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        assert_eq!(p.request(R, false), PARK);
        assert_eq!(p.request(R, true), Target::Over(R));
    }

    // Blank-terminal audit: a layout in flight reports 0x0 (hidden pane, collapsing split). It must neither be
    // remembered as the pane's rect nor move the view, and the next real rect must bring the view back.
    #[test]
    fn a_zero_rect_in_flight_never_replaces_the_last_good_rect_and_the_view_returns() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        assert_eq!(p.request((0.0, 0.0, 0.0, 0.0), true), PARK, "0x0 parks at the last size");
        assert_eq!(p.request(R, true), Target::Over(R), "and the real rect shows it again");
        assert!(p.is_presented());
    }

    // Blank-terminal audit: `shown` never sticks. A hide followed by a show with the same rect (a tab switch
    // back, a reload of the page that re-syncs) must present the view again without a new first frame.
    #[test]
    fn hide_then_show_with_the_same_rect_presents_again() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        p.request(R, false);
        assert!(!p.is_presented());
        p.request(R, true);
        assert!(p.is_presented());
    }

    // An overlay is not a state of the tab: nothing here can express "covered", so a dialog cannot park a view.
    #[test]
    fn the_only_way_to_park_is_the_tab_not_being_shown() {
        let mut p = Placement::default();
        p.request(R, true);
        p.drawn();
        for _ in 0..3 {
            assert_eq!(p.request(R, true), Target::Over(R));
        }
        assert!(p.is_shown());
    }
}
