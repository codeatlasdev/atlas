//! Region math for the capture overlay, pure and platform-independent.
//!
//! Coordinates are **global logical points** with the origin at the top-left of the primary display and y growing
//! down (what `tauri::Monitor` and CSS use), unless a name says physical. A display's backing scale (1 or 2 on a
//! Mac) turns points into pixels only at the very end, in [`to_pixels`].

/// A rectangle: x, y (top-left), width, height.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }
}

/// A global-point rectangle as the pixel rectangle ScreenCaptureKit's `sourceRect` and `CGImage` cropping need:
/// relative to the display's own origin, scaled by its backing factor, with even sides (video encoders reject odd
/// sizes; stills ignore the evenness but share the rule so a screenshot and a recording of one region agree).
/// `None` when the region does not intersect the display.
pub fn to_pixels(region: Rect, display: Rect, scale: f64) -> Option<(u32, u32, u32, u32)> {
    let r = region.intersect(&display)?;
    let x = ((r.x - display.x) * scale).round().max(0.0);
    let y = ((r.y - display.y) * scale).round().max(0.0);
    let w = even((r.w * scale).round());
    let h = even((r.h * scale).round());
    (w >= 2.0 && h >= 2.0).then_some((x as u32, y as u32, w as u32, h as u32))
}

fn even(v: f64) -> f64 {
    (v / 2.0).floor() * 2.0
}

/// The block of source pixels the loupe shows: `grid` pixels square, centred on the pointer's pixel, clamped so the
/// block never leaves the display (the loupe then shows the edge, not nothing). Pixel coordinates, display-relative.
pub fn loupe_source(
    pointer: (f64, f64),
    display: Rect,
    scale: f64,
    grid: u32,
) -> (u32, u32, u32, u32) {
    let px = ((pointer.0 - display.x) * scale).floor().max(0.0) as i64;
    let py = ((pointer.1 - display.y) * scale).floor().max(0.0) as i64;
    let (dw, dh) = (
        (display.w * scale).round() as i64,
        (display.h * scale).round() as i64,
    );
    let g = i64::from(grid).min(dw).min(dh);
    let x = (px - g / 2).clamp(0, dw - g);
    let y = (py - g / 2).clamp(0, dh - g);
    (x as u32, y as u32, g as u32, g as u32)
}

/// How far a macOS window's drop shadow reaches past its frame, points (about 40 below, 20 to the sides, 10 above:
/// one even margin on every side keeps the window centred and the shadow whole).
pub const SHADOW_MARGIN_PT: f64 = 40.0;
/// The margin of a window on a padded background, points.
pub const PADDING_PT: f64 = 64.0;

/// What a window capture looks like around the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLook {
    /// The frame only (no shadow, no margin).
    Plain,
    /// The frame inside a transparent margin that holds the window's own drop shadow.
    Shadow,
    /// The frame on a padded background.
    Padded,
}

/// The look the two settings ask for: padding wins when both are on.
pub fn window_look(shadow: bool, padding: bool) -> WindowLook {
    match (padding, shadow) {
        (true, _) => WindowLook::Padded,
        (false, true) => WindowLook::Shadow,
        (false, false) => WindowLook::Plain,
    }
}

/// The output size of a window capture and the margin around the window, all in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Pixels between the canvas edge and the window frame, on every side.
    pub inset: u32,
}

/// The canvas of a window of `size` points at `scale` pixels per point with `look`. Sides are even and never below
/// 2 (ScreenCaptureKit rejects an empty size, an encoder an odd one).
pub fn window_canvas(size: (f64, f64), scale: f64, look: WindowLook) -> Canvas {
    let margin_pt = match look {
        WindowLook::Plain => 0.0,
        WindowLook::Shadow => SHADOW_MARGIN_PT,
        WindowLook::Padded => PADDING_PT,
    };
    let inset = (margin_pt * scale).round();
    let side = |pt: f64| even((pt * scale).round() + 2.0 * inset).max(2.0) as u32;
    Canvas {
        width: side(size.0),
        height: side(size.1),
        inset: inset as u32,
    }
}

/// The display of `displays` that contains the global point `(x, y)`, else the first one (`None` when there are none).
pub fn display_at(displays: &[Rect], x: f64, y: f64) -> Option<Rect> {
    displays
        .iter()
        .find(|d| d.contains(x, y))
        .or(displays.first())
        .copied()
}

/// Pixels per point of a still: the display's own (`native`, retina) or one (`1x`).
pub fn effective_scale(display_scale: f64, one_x: bool) -> f64 {
    if one_x { 1.0 } else { display_scale }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);

    #[test]
    fn the_display_under_the_pointer_wins_and_the_first_is_the_fallback() {
        let second = Rect::new(1512.0, -200.0, 2560.0, 1440.0);
        let all = [D, second];
        assert_eq!(display_at(&all, 10.0, 10.0), Some(D));
        assert_eq!(display_at(&all, 1600.0, -100.0), Some(second));
        // the right/bottom edges belong to the next display (half-open), so the seam is never in both
        assert_eq!(display_at(&all, 1512.0, 10.0), Some(second));
        // a pointer in no display (between mismatched edges) takes the first, like the card always did
        assert_eq!(display_at(&all, 9999.0, 9999.0), Some(D));
        assert_eq!(display_at(&[], 1.0, 1.0), None);
    }

    /// A Retina region of 300×150 pt is a 600×300 px image; the crop starts at the display's own origin, not the
    /// global one, so a second display's regions are not shifted by the first display's width.
    #[test]
    fn a_region_becomes_display_relative_pixels() {
        assert_eq!(
            to_pixels(Rect::new(100.0, 200.0, 300.0, 150.0), D, 2.0),
            Some((200, 400, 600, 300))
        );
        let second = Rect::new(1512.0, -200.0, 2560.0, 1440.0);
        assert_eq!(
            to_pixels(Rect::new(1612.0, -100.0, 400.0, 300.0), second, 1.0),
            Some((100, 100, 400, 300))
        );
    }

    #[test]
    fn pixel_sides_are_even_so_a_recording_of_the_region_encodes() {
        let (_, _, w, h) = to_pixels(Rect::new(0.0, 0.0, 101.0, 55.0), D, 1.0).unwrap();
        assert_eq!((w, h), (100, 54));
        let (_, _, w, h) = to_pixels(Rect::new(0.0, 0.0, 100.5, 50.5), D, 2.0).unwrap();
        assert_eq!((w % 2, h % 2), (0, 0));
    }

    #[test]
    fn a_region_off_the_display_has_no_pixels() {
        assert_eq!(
            to_pixels(Rect::new(3000.0, 0.0, 100.0, 100.0), D, 2.0),
            None
        );
        // only a sliver inside: kept, intersected
        let r = to_pixels(Rect::new(1500.0, 970.0, 100.0, 100.0), D, 1.0).unwrap();
        assert_eq!((r.2, r.3), (12, 12));
        // a sliver under 2 px is nothing
        assert_eq!(
            to_pixels(Rect::new(1511.0, 0.0, 100.0, 100.0), D, 1.0),
            None
        );
    }

    #[test]
    fn the_loupe_samples_a_block_around_the_pointer_and_clamps_at_the_edges() {
        assert_eq!(loupe_source((100.0, 100.0), D, 2.0, 15), (193, 193, 15, 15));
        assert_eq!(loupe_source((0.0, 0.0), D, 2.0, 15), (0, 0, 15, 15));
        let (x, y, g, _) = loupe_source((1511.0, 981.0), D, 2.0, 15);
        assert_eq!(g, 15);
        assert_eq!(
            (x + g, y + g),
            (3024, 1964),
            "flush with the bottom-right corner"
        );
    }

    // ───────── window capture: shadow / padding canvas and the retina choice ─────────

    #[test]
    fn a_window_without_extras_is_just_its_frame_in_pixels() {
        let c = window_canvas((800.0, 600.0), 2.0, WindowLook::Plain);
        assert_eq!(
            c,
            Canvas {
                width: 1600,
                height: 1200,
                inset: 0
            }
        );
    }

    /// The shadow of a macOS window reaches about 40 pt below, 20 pt to the sides and 10 pt above; one even margin
    /// on every side keeps the window centred and the shadow whole.
    #[test]
    fn a_shadow_adds_a_margin_on_every_side_in_pixels() {
        let c = window_canvas((800.0, 600.0), 2.0, WindowLook::Shadow);
        assert_eq!(c.inset, SHADOW_MARGIN_PT as u32 * 2);
        assert_eq!(c.width, 1600 + 2 * c.inset);
        assert_eq!(c.height, 1200 + 2 * c.inset);
        // at 1x the margin is in 1x pixels
        let c1 = window_canvas((800.0, 600.0), 1.0, WindowLook::Shadow);
        assert_eq!(c1.inset, SHADOW_MARGIN_PT as u32);
    }

    #[test]
    fn padding_is_wider_than_the_shadow_margin_and_even() {
        let p = window_canvas((800.0, 600.0), 2.0, WindowLook::Padded);
        let s = window_canvas((800.0, 600.0), 2.0, WindowLook::Shadow);
        assert!(p.inset > s.inset);
        assert_eq!((p.width % 2, p.height % 2), (0, 0));
    }

    /// Padding wins over the shadow when both are on (the padded background carries its own soft shadow).
    #[test]
    fn padding_wins_over_the_shadow() {
        assert_eq!(window_look(true, true), WindowLook::Padded);
        assert_eq!(window_look(true, false), WindowLook::Shadow);
        assert_eq!(window_look(false, true), WindowLook::Padded);
        assert_eq!(window_look(false, false), WindowLook::Plain);
    }

    /// "1x" means one pixel per point whatever the display: a 2x display's 800×600 pt window is 800×600 px.
    #[test]
    fn one_x_ignores_the_display_scale_and_native_keeps_it() {
        assert_eq!(effective_scale(2.0, false), 2.0);
        assert_eq!(effective_scale(2.0, true), 1.0);
        assert_eq!(effective_scale(1.0, true), 1.0);
        assert_eq!(effective_scale(3.0, false), 3.0);
    }

    #[test]
    fn a_tiny_or_zero_window_still_yields_a_valid_canvas() {
        let c = window_canvas((0.0, 0.0), 2.0, WindowLook::Plain);
        assert_eq!(
            (c.width, c.height),
            (2, 2),
            "never zero: SCK rejects an empty size"
        );
    }
}
