//! Where a terminal's native view sits in the window, as pure decisions (the AppKit calls are in `mac`).
//!
//! The view stack of the main window, bottom to top:
//!
//! 1. the vibrancy view (`window_vibrancy`, inserted `Below` everything),
//! 2. **every terminal view** (this module),
//! 3. the main `WKWebView`, transparent (`drawsBackground = false`, window `transparent: true`),
//! 4. browser child webviews (`add_child`), which stay above the page.
//!
//! A terminal is therefore a *hole* in the page: the slot element in the DOM is transparent while the
//! view is presented there, and everything the page draws over it (menus, popovers, dialogs, the sidebar
//! peek, toasts) is simply above it. Nothing is hidden for an overlay. The view never takes the mouse
//! (`hitTest:` → nil, and the webview covers it anyway): pointer and key events reach the slot's DOM
//! handlers, which forward them to the `term_*` commands.
//!
//! A view is moved off screen only when its tab really is not on screen (a background tab, a worktree
//! or space that is not current): a state, never a reaction to an overlay.

/// Frame of a terminal view in the content view's top-left coordinates.
pub type Frame = (f64, f64, f64, f64);

/// Where a hidden terminal's view is parked. Only tabs that are really not on screen.
pub(super) const OFFSCREEN: f64 = -20_000.0;

/// What a `term_bounds` request resolves to for the AppKit side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    /// Put the view over this rect.
    Over(Frame),
    /// Move the view off screen at this size (a hidden tab keeps its size for its return).
    Park(f64, f64),
}

/// A rect that can really hold a terminal: finite and positive. A hidden pane or a layout in flight
/// reports 0×0 (or NaN); applying it would shrink the surface to nothing, and a view the page then
/// shows again blank until the next resize.
pub(super) fn usable(f: Frame) -> bool {
    let (x, y, w, h) = f;
    [x, y, w, h].iter().all(|v| v.is_finite()) && w >= 1.0 && h >= 1.0
}

/// Resolve a request: `show` and a usable rect put the view there; anything else parks it at the
/// last usable size (`last`), or a 1×1 sliver when it never had one.
pub(super) fn resolve(rect: Frame, show: bool, last: Option<(f64, f64)>) -> Target {
    if show && usable(rect) {
        Target::Over(rect)
    } else {
        let (w, h) = last.unwrap_or((1.0, 1.0));
        Target::Park(w, h)
    }
}

/// Does the view's content hole belong in the page? Only when the view is on screen over the slot,
/// has drawn its first frame and the slot is not in motion (a tab swipe moves the pane by a CSS
/// transform the view cannot follow, so the slot paints the terminal colour while it moves).
pub(super) fn hole_open(shown: bool, drawn: bool, moving: bool) -> bool {
    shown && drawn && !moving
}

/// Physical pixels of a frame of `w` × `h` points at backing `scale`: what the Metal drawable and the
/// renderer's surface are sized to, at least 1 so the layer never loses its surface.
pub(super) fn pixels(w: f64, h: f64, scale: f64) -> (u32, u32) {
    let px = |v: f64| ((v * scale).round().max(1.0)) as u32;
    (px(w), px(h))
}

/// A terminal on screen that the native page stage must cover: its frame in the content view's top-left
/// coordinates and its background colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    pub frame: Frame,
    pub bg: [f64; 3],
}

/// The fills for a stage over `pager` (x, y, w, h): every presented terminal that intersects it, its frame
/// clipped to the pager and expressed relative to the pager's origin (where the overlay layers live).
///
/// `WKWebView.takeSnapshot` captures web content only, so a snapshot has a transparent hole where a terminal is,
/// and the live terminal (under the page) would stay still beneath a sliding overlay. A fill of the terminal's
/// colour in the sliding layer covers it and slides with its page: the same flat block the page showed when the
/// view was parked.
pub fn stage_fills(pager: Frame, terms: &[Fill]) -> Vec<Fill> {
    let (px, py, pw, ph) = pager;
    terms
        .iter()
        .filter_map(|t| {
            let (x, y, w, h) = t.frame;
            let (x0, y0) = (x.max(px), y.max(py));
            let (x1, y1) = ((x + w).min(px + pw), (y + h).min(py + ph));
            (x1 > x0 && y1 > y0).then_some(Fill {
                frame: (x0 - px, y0 - py, x1 - x0, y1 - y0),
                bg: t.bg,
            })
        })
        .collect()
}

/// `#rrggbb`-style bytes as the 0..1 sRGB floats AppKit takes.
pub(super) fn bg_floats(r: u8, g: u8, b: u8) -> [f64; 3] {
    [r, g, b].map(|v| f64::from(v) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Frame = (10.0, 20.0, 300.0, 200.0);

    #[test]
    fn a_shown_usable_rect_is_placed() {
        assert_eq!(resolve(R, true, None), Target::Over(R));
    }

    #[test]
    fn a_hidden_tab_parks_at_its_last_size() {
        assert_eq!(resolve(R, false, Some((300.0, 200.0))), Target::Park(300.0, 200.0));
        assert_eq!(resolve(R, false, None), Target::Park(1.0, 1.0));
    }

    // Blank-terminal audit: a 0x0 or NaN rect from a layout in flight must never be applied as a frame.
    #[test]
    fn degenerate_rects_park_instead_of_shrinking_the_surface() {
        for bad in [
            (0.0, 0.0, 0.0, 0.0),
            (0.0, 0.0, 300.0, 0.0),
            (0.0, 0.0, 0.0, 200.0),
            (f64::NAN, 0.0, 300.0, 200.0),
            (0.0, 0.0, f64::INFINITY, 200.0),
            (0.0, 0.0, -5.0, 200.0),
        ] {
            assert_eq!(resolve(bad, true, Some((300.0, 200.0))), Target::Park(300.0, 200.0), "{bad:?}");
        }
    }

    #[test]
    fn the_hole_is_open_only_while_the_view_is_really_there_and_still() {
        assert!(hole_open(true, true, false));
        assert!(!hole_open(false, true, false), "hidden tab: the slot paints");
        assert!(!hole_open(true, false, false), "before the first frame: the slot paints");
        assert!(!hole_open(true, true, true), "a pane in motion: the slot paints");
    }

    const BG: [f64; 3] = [0.1, 0.2, 0.3];

    // The stage slides snapshots that have a hole where a terminal is: the fill covers the live view under it.
    #[test]
    fn a_terminal_inside_the_pager_is_filled_relative_to_the_pager_origin() {
        let fills = stage_fills((100.0, 40.0, 800.0, 600.0), &[Fill { frame: (120.0, 80.0, 300.0, 200.0), bg: BG }]);
        assert_eq!(fills, vec![Fill { frame: (20.0, 40.0, 300.0, 200.0), bg: BG }]);
    }

    #[test]
    fn a_terminal_poking_out_of_the_pager_is_clipped_and_one_outside_is_dropped() {
        let pager = (100.0, 40.0, 800.0, 600.0);
        let out = stage_fills(
            pager,
            &[
                Fill { frame: (50.0, 40.0, 100.0, 100.0), bg: BG },
                Fill { frame: (0.0, 0.0, 90.0, 900.0), bg: BG },
                Fill { frame: (900.0, 40.0, 50.0, 50.0), bg: BG },
            ],
        );
        assert_eq!(out, vec![Fill { frame: (0.0, 0.0, 50.0, 100.0), bg: BG }]);
    }

    #[test]
    fn bg_floats_span_zero_to_one() {
        assert_eq!(bg_floats(0, 0, 0), [0.0, 0.0, 0.0]);
        assert_eq!(bg_floats(255, 255, 255), [1.0, 1.0, 1.0]);
        let [r, ..] = bg_floats(51, 0, 0);
        assert!((r - 0.2).abs() < 1e-9);
    }

    // The Metal drawable follows the frame in the same block: pixels are the frame's points times the scale,
    // never zero (a zero drawable makes the layer lose its surface).
    #[test]
    fn surface_pixels_follow_the_frame_and_never_hit_zero() {
        assert_eq!(pixels(300.0, 200.0, 2.0), (600, 400));
        assert_eq!(pixels(300.4, 200.6, 2.0), (600, 401));
        assert_eq!(pixels(0.2, 0.0, 2.0), (1, 1));
    }
}
