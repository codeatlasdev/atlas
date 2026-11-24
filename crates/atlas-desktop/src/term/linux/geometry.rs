//! The pure arithmetic of placing a `wl_subsurface`: no GTK, no Wayland, so every line of it is tested on any OS.
//!
//! The page measures a slot in CSS pixels, which on GTK are the toplevel surface's logical units (GDK draws the page at
//! an integer buffer scale; the compositor does the rest). A subsurface is placed in whole logical units
//! (`wl_subsurface.set_position`, `wp_viewport.set_destination` take integers) and draws a swapchain of physical
//! pixels: the compositor scales it by `destination / buffer`. The renderer's size must come from the SAME rounded
//! rect, or the picture is stretched by up to a pixel (a soft edge on every glyph at 1.5x).

/// A subsurface rect in the toplevel's logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The rect of a slot at (`x`, `y`) of size `w` x `h` CSS px, in the toplevel's units: `off` is where the webview sits
/// in the toplevel (a menu bar or a frame above it). Edges are rounded, not the size, so two panes that touch in the
/// page still touch on screen. Never smaller than 1 x 1.
pub fn quantize(x: f64, y: f64, w: f64, h: f64, off: (i32, i32)) -> Rect {
    let edge = |v: f64| v.round() as i32;
    let (l, t) = (edge(x), edge(y));
    Rect {
        x: l + off.0,
        y: t + off.1,
        w: (edge(x + w) - l).max(1),
        h: (edge(y + h) - t).max(1),
    }
}

/// Physical pixels of a logical length at `scale` (1, 1.5, 2, or any the compositor reports).
pub fn physical(logical: i32, scale: f64) -> u32 {
    ((f64::from(logical) * scale).round() as u32).max(1)
}

/// The swapchain size for a slot: what [`quantize`] makes of it, in physical pixels.
pub fn pixel_size(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (u32, u32) {
    let r = quantize(x, y, w, h, (0, 0));
    (physical(r.w, scale), physical(r.h, scale))
}

/// `wp_fractional_scale_v1.preferred_scale`: the scale times 120.
pub fn scale_from_120(v: u32) -> f64 {
    f64::from(v) / 120.0
}

/// `cubic-bezier(0, 0, 0.58, 1)`, the page slide's CSS `ease-out`, at `t` in 0..1 (time -> progress), by bisection on x.
pub fn ease_out(t: f64) -> f64 {
    const P1: (f64, f64) = (0.0, 0.0);
    const P2: (f64, f64) = (0.58, 1.0);
    let bez = |u: f64, a: f64, b: f64| {
        let v = 1.0 - u;
        3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
    };
    let t = t.clamp(0.0, 1.0);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, P1.0, P2.0) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez((lo + hi) / 2.0, P1.1, P2.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_rounded_by_its_edges_so_neighbours_still_touch() {
        // Two panes of one row: [10.4, 110.6) and [110.6, 300.2): both share the edge 111.
        let a = quantize(10.4, 0.0, 100.2, 50.0, (0, 0));
        let b = quantize(110.6, 0.0, 189.6, 50.0, (0, 0));
        assert_eq!((a.x, a.w), (10, 101));
        assert_eq!(a.x + a.w, b.x, "no gap and no overlap between neighbours");
    }

    #[test]
    fn the_webview_offset_moves_the_rect_not_its_size() {
        let r = quantize(100.0, 200.0, 640.0, 480.0, (0, 32));
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 232,
                w: 640,
                h: 480
            }
        );
    }

    #[test]
    fn an_empty_or_inverted_slot_is_one_pixel_never_zero() {
        // A zero-sized viewport destination is a protocol error; wgpu cannot configure a zero surface.
        let r = quantize(5.0, 5.0, 0.0, -3.0, (0, 0));
        assert_eq!((r.w, r.h), (1, 1));
        assert_eq!(pixel_size(0.0, 0.0, 0.0, 0.0, 1.5), (2, 2));
    }

    #[test]
    fn physical_pixels_follow_the_rounded_logical_size_at_each_scale() {
        // 1, 1.5 and 2 are the scales the design measured.
        assert_eq!(pixel_size(0.0, 0.0, 780.0, 564.0, 1.0), (780, 564));
        assert_eq!(pixel_size(0.0, 0.0, 780.0, 564.0, 1.5), (1170, 846));
        assert_eq!(pixel_size(0.0, 0.0, 780.0, 564.0, 2.0), (1560, 1128));
        // 101 logical at 1.5 is 151.5 physical: rounded, so the stretch to the destination is under a pixel.
        assert_eq!(physical(101, 1.5), 152);
    }

    #[test]
    fn the_swapchain_size_comes_from_the_same_rounding_as_the_placement() {
        // Regression guard for a stretched picture: the page asks for a fractional slot; the placed width is 101
        // (edges 10 and 111), so the swapchain must be 101 x scale, not round(100.2) x scale.
        let (x, w) = (10.4, 100.2);
        let placed = quantize(x, 0.0, w, 10.0, (0, 0)).w;
        assert_eq!(pixel_size(x, 0.0, w, 10.0, 2.0).0, physical(placed, 2.0));
        assert_ne!(physical(placed, 2.0), physical(w.round() as i32, 2.0));
    }

    #[test]
    fn preferred_scale_is_in_hundred_twentieths() {
        assert_eq!(scale_from_120(120), 1.0);
        assert_eq!(scale_from_120(180), 1.5);
        assert_eq!(scale_from_120(240), 2.0);
    }

    #[test]
    fn ease_out_starts_at_zero_ends_at_one_and_never_goes_back() {
        assert!(ease_out(0.0).abs() < 1e-6);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-6);
        let mut prev = 0.0;
        for i in 1..=100 {
            let p = ease_out(f64::from(i) / 100.0);
            assert!(p >= prev, "monotone at {i}");
            prev = p;
        }
        // ease-out is front-loaded: more than half done at half the time.
        assert!(ease_out(0.5) > 0.6);
    }
}
