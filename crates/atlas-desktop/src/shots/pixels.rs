//! Pixel work that needs no OS: decoding a PNG to RGBA, cutting a block out of it for the magnifier, and encoding
//! a block back as a PNG data URL. Pure, so the magnifier's correctness (which pixels, which edge behaviour) is
//! tested without a display.

use base64::Engine;

use super::geom::WindowLook;

/// RGBA pixels of an image.
pub struct Rgba {
    pub data: Vec<u8>,
    pub size: (u32, u32),
}

/// Decode PNG bytes to RGBA8 (any PNG colour type ScreenCaptureKit and `png` produce).
pub fn decode_png(bytes: &[u8]) -> Result<Rgba, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("the image is too large")?];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let data = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
    };
    Ok(Rgba {
        data,
        size: (info.width, info.height),
    })
}

/// Where the red, green and blue bytes sit inside one 32-bit pixel of a `CGImage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order(pub [usize; 3]);

impl Order {
    /// Little-endian 32-bit with alpha first/last or skipped: what ScreenCaptureKit and AVFoundation hand out.
    pub const BGRA: Order = Order([2, 1, 0]);
    pub const RGBA: Order = Order([0, 1, 2]);
    pub const ARGB: Order = Order([1, 2, 3]);
    pub const ABGR: Order = Order([3, 2, 1]);

    /// The byte order of a 32-bit CoreGraphics pixel from its `CGImageAlphaInfo` (`alpha`, 0..=7) and whether the
    /// words are little-endian (`kCGImageByteOrder32Little`). `None` for formats this does not read (alpha only,
    /// big-endian words that are not the default order is treated as big).
    pub fn of(alpha: u32, little: bool) -> Option<Order> {
        // alpha first: the colour channels follow it; alpha last or skipped-last: they come first
        let first = match alpha {
            2 | 4 | 6 => true,
            1 | 3 | 5 => false,
            _ => return None,
        };
        Some(match (little, first) {
            (true, true) => Order::BGRA,
            (true, false) => Order::ABGR,
            (false, true) => Order::ARGB,
            (false, false) => Order::RGBA,
        })
    }
}

/// Tight RGBA from the 32-bit rows of a `CGImage` (`stride` bytes per row, possibly padded) whose colour bytes sit at
/// `order`. Alpha is forced to 255: this is for the frozen screen and video frames, which are opaque, and
/// premultiplied pixels must not read as darkened.
pub fn rgba_from_rows(
    src: &[u8],
    stride: usize,
    w: usize,
    h: usize,
    order: Order,
) -> Result<Vec<u8>, String> {
    let row = w.checked_mul(4).ok_or("the image is too large")?;
    if h > 0 && (stride < row || src.len() < stride * (h - 1) + row) {
        return Err(format!(
            "{} bytes cannot hold {w}x{h} pixels at stride {stride}",
            src.len()
        ));
    }
    let [r, g, b] = order.0;
    let mut out = vec![0u8; row * h];
    for (y, dst) in out.chunks_exact_mut(row.max(1)).enumerate().take(h) {
        let s = &src[y * stride..y * stride + row];
        for (d, p) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(s.as_chunks::<4>().0)
        {
            *d = [p[r], p[g], p[b], 255];
        }
    }
    Ok(out)
}

/// The `w`×`h` block at `(x, y)` of `img`, RGBA. The caller keeps the block inside the image
/// (`geom::loupe_source` clamps); a block that is not is an error, not a panic.
pub fn block(
    data: &[u8],
    size: (u32, u32),
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, String> {
    if x.checked_add(w).is_none_or(|r| r > size.0)
        || y.checked_add(h).is_none_or(|b| b > size.1)
        || data.len() < (size.0 as usize) * (size.1 as usize) * 4
    {
        return Err(format!(
            "block {w}x{h} at {x},{y} is outside the {}x{} image",
            size.0, size.1
        ));
    }
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * size.0 + x) * 4) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    Ok(out)
}

/// The colour of the middle pixel (`w/2`, `h/2`) of an RGBA block as `#rrggbb`: what the loupe prints under the
/// magnifier. `None` for an empty block or one whose data is shorter than `w`×`h`.
pub fn centre_hex(rgba: &[u8], w: u32, h: u32) -> Option<String> {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let i = ((h / 2) * w + w / 2) * 4;
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        rgba[i],
        rgba[i + 1],
        rgba[i + 2]
    ))
}

/// RGBA pixels as a PNG `data:` URL.
pub fn data_url(rgba: &[u8], w: u32, h: u32) -> Result<String, String> {
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encode_png(rgba, w, h)?)
    ))
}

/// RGBA8 pixels as PNG bytes. Pixel data of the wrong length is an error.
pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// A box-filtered thumbnail no larger than `max` on its longest side (never scaled up), as RGBA.
pub fn thumbnail(img: &Rgba, max: u32) -> (Vec<u8>, (u32, u32)) {
    let longest = img.size.0.max(img.size.1).max(1);
    if longest <= max {
        return (img.data.clone(), img.size);
    }
    let s = f64::from(max) / f64::from(longest);
    let dst = (
        ((f64::from(img.size.0) * s).round() as u32).max(1),
        ((f64::from(img.size.1) * s).round() as u32).max(1),
    );
    (super::gif::downscale(&img.data, img.size, dst), dst)
}

/// Strongest alpha of a window's shadow, right under its edge.
const SHADOW_ALPHA: f32 = 0.5;
/// The padded background (#ECECEC, the light grey of a Finder window).
const PAD_BG: [u8; 4] = [236, 236, 236, 255];

/// `win` (RGBA) centred on a canvas with `inset` pixels on every side: a transparent margin holding a soft drop
/// shadow ([`WindowLook::Shadow`]) or the same shadow on a light opaque background ([`WindowLook::Padded`]).
/// `Plain` (or no inset) returns the window as it is. The window's own alpha is kept, so its rounded corners show
/// the shadow behind them. `scale` is the capture's pixels per point; it sizes the shadow's offset and blur to the
/// window, not to the margin.
pub fn frame_window(win: &Rgba, look: WindowLook, inset: u32, scale: f64) -> Result<Rgba, String> {
    let (ww, wh) = (win.size.0 as usize, win.size.1 as usize);
    if win.data.len() != ww * wh * 4 {
        return Err("the window image is not RGBA pixels".into());
    }
    if look == WindowLook::Plain || inset == 0 {
        return Ok(Rgba {
            data: win.data.clone(),
            size: win.size,
        });
    }
    let pad = inset as usize;
    let (cw, ch) = (
        pad.checked_mul(2).and_then(|p| p.checked_add(ww)),
        pad.checked_mul(2).and_then(|p| p.checked_add(wh)),
    );
    let (cw, ch) = match (cw, ch) {
        (Some(w), Some(h))
            if w.checked_mul(h)
                .and_then(|n| n.checked_mul(4))
                .is_some_and(|n| n <= 1 << 31) =>
        {
            (w, h)
        }
        _ => return Err("the window is too large to frame".into()),
    };

    // The shadow: the window's silhouette, moved down, blurred.
    let offset = ((pad as f32 / 4.0) * (scale as f32).clamp(1.0, 3.0).sqrt()).round() as isize;
    let radius = (pad / 3).max(1);
    let mut mask = vec![0f32; cw * ch];
    for y in 0..wh {
        for x in 0..ww {
            let a = f32::from(win.data[(y * ww + x) * 4 + 3]) / 255.0;
            let (cx, cy) = (x + pad, y as isize + pad as isize + offset);
            if cy >= 0 && (cy as usize) < ch {
                mask[cy as usize * cw + cx] = a;
            }
        }
    }
    for _ in 0..3 {
        box_blur(&mut mask, cw, ch, radius);
    }

    let mut out = vec![0u8; cw * ch * 4];
    for (i, m) in mask.iter().enumerate() {
        let a = (m * SHADOW_ALPHA).clamp(0.0, 1.0);
        let px = &mut out[i * 4..i * 4 + 4];
        if look == WindowLook::Padded {
            // black shadow over the light background
            for c in 0..3 {
                px[c] = (f32::from(PAD_BG[c]) * (1.0 - a)).round() as u8;
            }
            px[3] = 255;
        } else {
            px[3] = (a * 255.0).round() as u8;
        }
    }
    // The window over it (source-over, straight alpha).
    for y in 0..wh {
        for x in 0..ww {
            let s = &win.data[(y * ww + x) * 4..(y * ww + x) * 4 + 4];
            let d = &mut out[((y + pad) * cw + x + pad) * 4..((y + pad) * cw + x + pad) * 4 + 4];
            over(d, s);
        }
    }
    Ok(Rgba {
        data: out,
        size: (cw as u32, ch as u32),
    })
}

/// Source-over of straight-alpha `s` onto straight-alpha `d`, in place.
fn over(d: &mut [u8], s: &[u8]) {
    let sa = f32::from(s[3]) / 255.0;
    let da = f32::from(d[3]) / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        d.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for c in 0..3 {
        let v = (f32::from(s[c]) * sa + f32::from(d[c]) * da * (1.0 - sa)) / oa;
        d[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    d[3] = (oa * 255.0).round() as u8;
}

/// One separable box blur of radius `r` over a `w`×`h` plane; outside the plane counts as 0.
fn box_blur(p: &mut [f32], w: usize, h: usize, r: usize) {
    let n = (2 * r + 1) as f32;
    let mut tmp = vec![0f32; p.len()];
    for y in 0..h {
        let row = &p[y * w..(y + 1) * w];
        let mut sum: f32 = row.iter().take(r + 1).sum();
        for x in 0..w {
            tmp[y * w + x] = sum / n;
            if x + r + 1 < w {
                sum += row[x + r + 1];
            }
            if x >= r {
                sum -= row[x - r];
            }
        }
    }
    for x in 0..w {
        let mut sum: f32 = (0..=r.min(h - 1)).map(|y| tmp[y * w + x]).sum();
        for y in 0..h {
            p[y * w + x] = sum / n;
            if y + r + 1 < h {
                sum += tmp[(y + r + 1) * w + x];
            }
            if y >= r {
                sum -= tmp[(y - r) * w + x];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Rgba {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        Rgba { data, size: (w, h) }
    }

    fn png_of(img: &Rgba) -> Vec<u8> {
        let url = data_url(&img.data, img.size.0, img.size.1).unwrap();
        base64::engine::general_purpose::STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap()
    }

    /// ScreenCaptureKit hands back BGRA, premultiplied, rows padded to `stride` bytes: the loupe needs tight RGBA.
    #[test]
    fn bgra_rows_with_padding_become_tight_rgba() {
        // 2x2, stride 12 (8 bytes of pixels + 4 of padding per row)
        let src = [
            1, 2, 3, 255, 4, 5, 6, 255, 9, 9, 9, 9, //
            7, 8, 9, 255, 10, 11, 12, 255, 9, 9, 9, 9,
        ];
        let got = rgba_from_rows(&src, 12, 2, 2, Order::BGRA).unwrap();
        assert_eq!(
            got,
            [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
        );
    }

    #[test]
    fn bgra_with_a_stride_equal_to_the_row_is_a_plain_swizzle() {
        let src = [10, 20, 30, 255, 40, 50, 60, 255];
        assert_eq!(
            rgba_from_rows(&src, 8, 2, 1, Order::BGRA).unwrap(),
            [30, 20, 10, 255, 60, 50, 40, 255]
        );
    }

    #[test]
    fn bgra_data_shorter_than_the_image_is_an_error_not_a_panic() {
        assert!(rgba_from_rows(&[0; 15], 8, 2, 2, Order::BGRA).is_err());
        assert!(
            rgba_from_rows(&[0; 16], 4, 2, 2, Order::BGRA).is_err(),
            "stride below a row"
        );
        assert!(
            rgba_from_rows(&[], 0, 0, 0, Order::BGRA).is_ok(),
            "an empty image is empty"
        );
    }

    /// The frozen frame is opaque; premultiplied alpha must not darken it, and the converter forces alpha to 255.
    #[test]
    fn bgra_alpha_is_forced_opaque_for_the_frozen_frame() {
        let src = [10, 20, 30, 0];
        assert_eq!(
            rgba_from_rows(&src, 4, 1, 1, Order::BGRA).unwrap(),
            [30, 20, 10, 255]
        );
    }

    #[test]
    fn every_32_bit_layout_reads_its_own_channels() {
        let px = [11, 22, 33, 44];
        let rgb = |o| rgba_from_rows(&px, 4, 1, 1, o).unwrap();
        assert_eq!(rgb(Order::RGBA), [11, 22, 33, 255]);
        assert_eq!(rgb(Order::BGRA), [33, 22, 11, 255]);
        assert_eq!(rgb(Order::ARGB), [22, 33, 44, 255]);
        assert_eq!(rgb(Order::ABGR), [44, 33, 22, 255]);
    }

    /// CoreGraphics' alpha info + byte order decide the layout: BGRA premultiplied-first little-endian is what
    /// ScreenCaptureKit returns; the others are what a decoder may hand back.
    #[test]
    fn the_layout_comes_from_the_alpha_info_and_the_byte_order() {
        // premultiplied first / first / skip first, little-endian words: B G R A in memory
        for a in [2, 4, 6] {
            assert_eq!(Order::of(a, true), Some(Order::BGRA), "alpha {a}");
            assert_eq!(Order::of(a, false), Some(Order::ARGB), "alpha {a}");
        }
        // alpha last: R G B A in memory big-endian, A B G R little-endian
        for a in [1, 3, 5] {
            assert_eq!(Order::of(a, false), Some(Order::RGBA), "alpha {a}");
            assert_eq!(Order::of(a, true), Some(Order::ABGR), "alpha {a}");
        }
        // none / alpha only: not a colour layout this reads
        assert_eq!(Order::of(0, true), None);
        assert_eq!(Order::of(7, true), None);
        assert_eq!(Order::of(99, true), None);
    }

    /// The loupe's colour readout is the middle pixel of the block (the one its reticle marks), as `#rrggbb`.
    #[test]
    fn the_centre_of_a_block_is_its_middle_pixel_as_lowercase_hex() {
        // 3x3, centre (1,1) = 10,171,255
        let mut b = vec![0u8; 3 * 3 * 4];
        let at = |x: usize, y: usize| (y * 3 + x) * 4;
        b[at(1, 1)..at(1, 1) + 4].copy_from_slice(&[10, 171, 255, 255]);
        assert_eq!(centre_hex(&b, 3, 3).as_deref(), Some("#0aabff"));
        // even sizes take (w/2, h/2), like the page's Math.floor(natural / 2) did
        let mut e = vec![0u8; 4 * 2 * 4];
        let at = |x: usize, y: usize| (y * 4 + x) * 4;
        e[at(2, 1)..at(2, 1) + 4].copy_from_slice(&[1, 2, 3, 255]);
        assert_eq!(centre_hex(&e, 4, 2).as_deref(), Some("#010203"));
    }

    #[test]
    fn the_centre_of_nothing_or_of_short_data_is_none() {
        assert_eq!(centre_hex(&[], 0, 0), None);
        assert_eq!(centre_hex(&[0; 8], 3, 3), None);
    }

    #[test]
    fn a_png_round_trips_through_encode_and_decode() {
        let img = gradient(20, 10);
        let back = decode_png(&png_of(&img)).unwrap();
        assert_eq!(back.size, (20, 10));
        assert_eq!(back.data, img.data);
    }

    /// The magnifier shows exactly the pixels around the pointer: the block's first pixel is the one at (x, y).
    #[test]
    fn a_block_is_the_exact_pixels_at_that_position() {
        let img = gradient(50, 40);
        let b = block(&img.data, img.size, 10, 20, 3, 2).unwrap();
        assert_eq!(b.len(), 3 * 2 * 4);
        assert_eq!(&b[..4], &[10, 20, 7, 255], "top-left of the block");
        assert_eq!(&b[8..12], &[12, 20, 7, 255], "third pixel of the first row");
        assert_eq!(
            &b[12..16],
            &[10, 21, 7, 255],
            "first pixel of the second row"
        );
    }

    #[test]
    fn a_block_touching_the_last_row_and_column_is_fine_and_one_past_is_an_error() {
        let img = gradient(50, 40);
        assert!(block(&img.data, img.size, 47, 37, 3, 3).is_ok());
        assert!(block(&img.data, img.size, 48, 37, 3, 3).is_err());
        assert!(block(&img.data, img.size, 47, 38, 3, 3).is_err());
        assert!(
            block(&img.data, img.size, u32::MAX, 0, 2, 2).is_err(),
            "overflow is an error, not a panic"
        );
    }

    /// The pointer block and `geom::loupe_source` agree: whatever the pointer position, the source rectangle is
    /// inside the frame, so `block` never fails for a position the overlay can produce.
    #[test]
    fn every_pointer_position_gives_a_block_inside_the_frame() {
        use crate::shots::geom::{Rect, loupe_source};
        let display = Rect::new(0.0, 0.0, 40.0, 30.0);
        let img = gradient(80, 60);
        for px in [0.0, 0.4, 19.0, 39.0, 39.99, 40.0, 100.0, -5.0] {
            for py in [0.0, 14.5, 29.0, 29.99, 30.0, 99.0, -5.0] {
                let (x, y, w, h) = loupe_source((px, py), display, 2.0, 15);
                assert!(
                    block(&img.data, img.size, x, y, w, h).is_ok(),
                    "pointer {px},{py} → {x},{y},{w},{h}"
                );
            }
        }
    }

    #[test]
    fn a_thumbnail_keeps_the_aspect_ratio_and_never_scales_up() {
        let img = gradient(400, 200);
        let (data, size) = thumbnail(&img, 100);
        assert_eq!(size, (100, 50));
        assert_eq!(data.len(), 100 * 50 * 4);
        let (_, same) = thumbnail(&gradient(60, 30), 100);
        assert_eq!(same, (60, 30));
    }

    #[test]
    fn the_data_url_has_the_png_prefix_and_decodes() {
        let url = data_url(&[255, 0, 0, 255], 1, 1).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(
            data_url(&[1, 2, 3], 1, 1).is_err(),
            "pixel data of the wrong length is refused"
        );
    }

    // ───────── window on a transparent margin (shadow) or a padded background ─────────

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Rgba {
        Rgba {
            data: px
                .iter()
                .copied()
                .cycle()
                .take((w * h * 4) as usize)
                .collect(),
            size: (w, h),
        }
    }

    fn at(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.size.0 + x) * 4) as usize;
        [
            img.data[i],
            img.data[i + 1],
            img.data[i + 2],
            img.data[i + 3],
        ]
    }

    #[test]
    fn the_window_lands_untouched_at_the_inset() {
        let win = gradient(20, 10);
        let out = frame_window(&win, WindowLook::Shadow, 16, 1.0).unwrap();
        assert_eq!(out.size, (20 + 32, 10 + 32));
        for (x, y) in [(0, 0), (19, 0), (0, 9), (19, 9), (7, 4)] {
            assert_eq!(at(&out, 16 + x, 16 + y), at(&win, x, y), "pixel {x},{y}");
        }
    }

    /// The margin of a shadow capture is transparent at the canvas edge (a PNG pasted on any background), darker
    /// and more opaque the closer to the window, and heavier below than above (a window casts down).
    #[test]
    fn the_shadow_margin_fades_to_transparent_and_falls_below_the_window() {
        let win = solid(40, 40, [200, 200, 200, 255]);
        let out = frame_window(&win, WindowLook::Shadow, 24, 1.0).unwrap();
        assert_eq!(at(&out, 0, 0)[3], 0, "the canvas corner is clear");
        assert_eq!(at(&out, 0, out.size.1 - 1)[3], 0);
        let below_near = at(&out, 24 + 20, 24 + 40 + 2)[3];
        let below_far = at(&out, 24 + 20, 24 + 40 + 20)[3];
        let above_near = at(&out, 24 + 20, 24 - 3)[3];
        assert!(below_near > below_far, "{below_near} > {below_far}");
        assert!(
            below_near > above_near,
            "shadow is heavier below: {below_near} > {above_near}"
        );
        // the shadow is dark, not a tinted copy of the window
        let c = at(&out, 24 + 20, 24 + 40 + 2);
        assert!(c[0] < 40 && c[1] < 40 && c[2] < 40, "{c:?}");
    }

    #[test]
    fn padding_is_an_opaque_light_background_behind_the_window() {
        let win = solid(30, 30, [10, 20, 30, 255]);
        let out = frame_window(&win, WindowLook::Padded, 32, 1.0).unwrap();
        let corner = at(&out, 0, 0);
        assert_eq!(corner[3], 255, "opaque");
        assert!(
            corner[0] > 200 && corner[1] > 200 && corner[2] > 200,
            "light: {corner:?}"
        );
    }

    /// A window with transparent corners (macOS draws them rounded) must keep them: the shadow shows through
    /// instead of a black square.
    #[test]
    fn translucent_window_pixels_blend_over_the_shadow_not_replace_it() {
        let mut win = solid(20, 20, [255, 255, 255, 255]);
        win.data[0..4].copy_from_slice(&[0, 0, 0, 0]);
        let out = frame_window(&win, WindowLook::Padded, 16, 1.0).unwrap();
        let corner = at(&out, 16, 16);
        assert_eq!(corner[3], 255);
        assert!(corner[0] > 150, "the background, not black: {corner:?}");
    }

    #[test]
    fn plain_returns_the_window_itself_and_a_zero_inset_is_the_same() {
        let win = gradient(8, 6);
        let a = frame_window(&win, WindowLook::Plain, 0, 2.0).unwrap();
        assert_eq!((a.size, &a.data), (win.size, &win.data));
        let b = frame_window(&win, WindowLook::Shadow, 0, 2.0).unwrap();
        assert_eq!(b.size, win.size);
    }

    #[test]
    fn an_image_too_big_to_frame_is_an_error_not_a_panic() {
        let win = Rgba {
            data: vec![0; 4],
            size: (1, 1),
        };
        assert!(frame_window(&win, WindowLook::Shadow, u32::MAX / 2, 1.0).is_err());
        let bad = Rgba {
            data: vec![0; 3],
            size: (2, 2),
        };
        assert!(frame_window(&bad, WindowLook::Shadow, 4, 1.0).is_err());
    }
}
