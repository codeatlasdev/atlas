//! Procedural glyphs drawn at exact cell size so they tile seamlessly (Ghostty's
//! sprite font): box drawing, block elements, braille, sextants and the powerline
//! triangles / semicircles. Output is an 8-bit coverage bitmap of `cw × ch`.

/// A cell-sized coverage bitmap.
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Bitmap {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }

    /// Fill the half-open pixel rect, clamped to the bitmap.
    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, v: u8) {
        let (x0, x1) = (x0.clamp(0, self.w as i32), x1.clamp(0, self.w as i32));
        let (y0, y1) = (y0.clamp(0, self.h as i32), y1.clamp(0, self.h as i32));
        for y in y0..y1 {
            let row = (y as u32 * self.w) as usize;
            self.px[row + x0 as usize..row + x1 as usize].fill(v);
        }
    }

    /// Per-pixel coverage from `f(pixel center x, y)` in `0..=1`; max-blended.
    fn shade(&mut self, f: impl Fn(f32, f32) -> f32) {
        for y in 0..self.h {
            for x in 0..self.w {
                let c = (f(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
                let p = &mut self.px[(y * self.w + x) as usize];
                *p = (*p).max(c);
            }
        }
    }
}

/// Does `ch` have a procedural sprite?
pub fn handles(ch: char) -> bool {
    matches!(ch as u32,
        0x2500..=0x259F | 0x2800..=0x28FF | 0xE0B0..=0xE0B7 | 0x1FB00..=0x1FB3B)
}

/// Rasterize `ch` into a `cw × ch_px` bitmap (`None` = not a sprite char).
pub fn draw(ch: char, cw: u32, chp: u32) -> Option<Bitmap> {
    if !handles(ch) {
        return None;
    }
    let mut b = Bitmap::new(cw, chp);
    let c = ch as u32;
    match c {
        0x2500..=0x257F => boxdraw(&mut b, c),
        0x2580..=0x259F => block(&mut b, c),
        0x2800..=0x28FF => braille(&mut b, (c - 0x2800) as u8),
        0x1FB00..=0x1FB3B => sextant(&mut b, c),
        _ => powerline(&mut b, c),
    }
    Some(b)
}

/// Light stroke thickness in pixels for a cell `cw` wide.
fn light(b: &Bitmap) -> i32 {
    ((b.w as f32 / 8.0).round() as i32).max(1)
}

fn heavy(b: &Bitmap) -> i32 {
    (light(b) * 2).max(light(b) + 1)
}

// ---- box drawing -----------------------------------------------------------

/// Arm weights per box char `U+2500 + i`: `[left, right, up, down]`,
/// 0 none, 1 light, 2 heavy, 3 double. Dashed/arc/diagonal chars are special-cased.
#[rustfmt::skip]
const BOX: [[u8; 4]; 128] = [
    [1,1,0,0],[2,2,0,0],[0,0,1,1],[0,0,2,2],
    [1,1,0,0],[2,2,0,0],[0,0,1,1],[0,0,2,2], // dashed 2504..2507
    [1,1,0,0],[2,2,0,0],[0,0,1,1],[0,0,2,2], // dashed 2508..250B
    [0,1,0,1],[0,2,0,1],[0,1,0,2],[0,2,0,2], // 250C..250F
    [1,0,0,1],[2,0,0,1],[1,0,0,2],[2,0,0,2], // 2510..2513
    [0,1,1,0],[0,2,1,0],[0,1,2,0],[0,2,2,0], // 2514..2517
    [1,0,1,0],[2,0,1,0],[1,0,2,0],[2,0,2,0], // 2518..251B
    [0,1,1,1],[0,2,1,1],[0,1,2,1],[0,1,1,2], // 251C..251F
    [0,1,2,2],[0,2,2,1],[0,2,1,2],[0,2,2,2], // 2520..2523
    [1,0,1,1],[2,0,1,1],[1,0,2,1],[1,0,1,2], // 2524..2527
    [1,0,2,2],[2,0,2,1],[2,0,1,2],[2,0,2,2], // 2528..252B
    [1,1,0,1],[2,1,0,1],[1,2,0,1],[2,2,0,1], // 252C..252F
    [1,1,0,2],[2,1,0,2],[1,2,0,2],[2,2,0,2], // 2530..2533
    [1,1,1,0],[2,1,1,0],[1,2,1,0],[2,2,1,0], // 2534..2537
    [1,1,2,0],[2,1,2,0],[1,2,2,0],[2,2,2,0], // 2538..253B
    [1,1,1,1],[2,1,1,1],[1,2,1,1],[2,2,1,1], // 253C..253F
    [1,1,2,1],[1,1,1,2],[1,1,2,2],[2,1,2,1], // 2540..2543
    [1,2,2,1],[2,1,1,2],[1,2,1,2],[2,2,2,1], // 2544..2547
    [2,2,1,2],[2,1,2,2],[1,2,2,2],[2,2,2,2], // 2548..254B
    [1,1,0,0],[2,2,0,0],[0,0,1,1],[0,0,2,2], // dashed 254C..254F
    [3,3,0,0],[0,0,3,3],[0,3,0,1],[0,1,0,3], // 2550..2553
    [0,3,0,3],[3,0,0,1],[1,0,0,3],[3,0,0,3], // 2554..2557
    [0,3,1,0],[0,1,3,0],[0,3,3,0],[3,0,1,0], // 2558..255B
    [1,0,3,0],[3,0,3,0],[0,3,1,1],[0,1,3,3], // 255C..255F
    [0,3,3,3],[3,0,1,1],[1,0,3,3],[3,0,3,3], // 2560..2563
    [3,3,0,1],[1,1,0,3],[3,3,0,3],[3,3,1,0], // 2564..2567
    [1,1,3,0],[3,3,3,0],[3,3,1,1],[1,1,3,3], // 2568..256B
    [3,3,3,3],[0,0,0,0],[0,0,0,0],[0,0,0,0], // 256C, arcs 256D..256F
    [0,0,0,0],[0,0,0,0],[0,0,0,0],[0,0,0,0], // arc 2570, diagonals 2571..2573
    [1,0,0,0],[0,0,1,0],[0,1,0,0],[0,0,0,1], // 2574..2577
    [2,0,0,0],[0,0,2,0],[0,2,0,0],[0,0,0,2], // 2578..257B
    [1,2,0,0],[0,0,1,2],[2,1,0,0],[0,0,2,1], // 257C..257F
];

fn boxdraw(b: &mut Bitmap, c: u32) {
    match c {
        0x2504..=0x250B | 0x254C..=0x254F => return dashed(b, c),
        0x256D..=0x2570 => return arc(b, c),
        0x2571..=0x2573 => return diagonal(b, c),
        _ => {}
    }
    let [l, r, u, d] = BOX[(c - 0x2500) as usize];
    let (lt, hv) = (light(b), heavy(b));
    let thick = |w: u8| if w == 2 { hv } else { lt };
    let (w, h) = (b.w as i32, b.h as i32);

    // An axis is double if any of its arms is; its strips are `[start, end)` pairs.
    // A single axis has one strip (listed twice) sized by its heaviest arm.
    let strips = |len: i32, a: u8, z: u8| -> [(i32, i32); 2] {
        if a == 3 || z == 3 {
            let s = (len - 3 * lt) / 2;
            [(s, s + lt), (s + 2 * lt, s + 3 * lt)]
        } else {
            let t = thick(a).max(thick(z));
            let s = (len - t) / 2;
            [(s, s + t); 2]
        }
    };
    let hs = strips(h, l, r); // y ranges of the horizontal strips (top, bottom)
    let vs = strips(w, u, d); // x ranges of the vertical strips (left, right)
    let hdouble = l == 3 || r == 3;
    let vdouble = u == 3 || d == 3;
    let (up, dn, lf, rt) = (u != 0, d != 0, l != 0, r != 0);

    // Own-thickness range of a single arm, centered on the axis.
    let own = |len: i32, wt: u8, dbl: bool, strip: usize, s: [(i32, i32); 2]| -> (i32, i32) {
        if dbl || wt == 0 {
            s[strip]
        } else {
            let t = thick(wt);
            ((len - t) / 2, (len - t) / 2 + t)
        }
    };
    for k in 0..if hdouble { 2 } else { 1 } {
        if rt {
            let (y0, y1) = own(h, r, hdouble, k, hs);
            let x0 = match (up, dn) {
                (true, true) => vs[1].0,
                (false, true) => vs[k].0,
                (true, false) => vs[1 - k].0,
                _ => vs[0].0,
            };
            b.fill(x0, y0, w, y1, 255);
        }
        if lf {
            let (y0, y1) = own(h, l, hdouble, k, hs);
            let x1 = match (up, dn) {
                (true, true) => vs[0].1,
                (false, true) => vs[1 - k].1,
                (true, false) => vs[k].1,
                _ => vs[1].1,
            };
            b.fill(0, y0, x1, y1, 255);
        }
    }
    for j in 0..if vdouble { 2 } else { 1 } {
        if dn {
            let (x0, x1) = own(w, d, vdouble, j, vs);
            let y0 = match (lf, rt) {
                (true, true) => hs[1].0,
                (false, true) => hs[j].0,
                (true, false) => hs[1 - j].0,
                _ => hs[0].0,
            };
            b.fill(x0, y0, x1, h, 255);
        }
        if up {
            let (x0, x1) = own(w, u, vdouble, j, vs);
            let y1 = match (lf, rt) {
                (true, true) => hs[0].1,
                (false, true) => hs[1 - j].1,
                (true, false) => hs[j].1,
                _ => hs[1].1,
            };
            b.fill(x0, 0, x1, y1, 255);
        }
    }
}

/// Dashed lines: `n` dashes per cell with half a gap at each end so cells tile.
fn dashed(b: &mut Bitmap, c: u32) {
    let (n, wt, vertical) = match c {
        0x2504 | 0x2505 => (3, c == 0x2505, false),
        0x2506 | 0x2507 => (3, c == 0x2507, true),
        0x2508 | 0x2509 => (4, c == 0x2509, false),
        0x250A | 0x250B => (4, c == 0x250B, true),
        0x254C | 0x254D => (2, c == 0x254D, false),
        _ => (2, c & 1 == 1, c >= 0x254E),
    };
    let t = if wt { heavy(b) } else { light(b) };
    let (len, cross) = if vertical { (b.h, b.w) } else { (b.w, b.h) };
    let (len, cross, t) = (len as i32, cross as i32, t);
    let gap = (len / (n * 2)).max(1);
    let c0 = (cross - t) / 2;
    for i in 0..n {
        let s = i * len / n + gap / 2;
        let e = (i + 1) * len / n - (gap - gap / 2);
        if e <= s {
            continue;
        }
        if vertical {
            b.fill(c0, s, c0 + t, e, 255);
        } else {
            b.fill(s, c0, e, c0 + t, 255);
        }
    }
}

/// Rounded corners: a quarter ellipse joining the two edge midpoints.
fn arc(b: &mut Bitmap, c: u32) {
    let (w, h) = (b.w as f32, b.h as f32);
    let t = light(b) as f32;
    let (rx, ry) = (w / 2.0, h / 2.0);
    // Ellipse center at the cell corner opposite to the open sides.
    let (cx, cy) = match c {
        0x256D => (w, h), // ╭ right+down
        0x256E => (0.0, h),
        0x256F => (0.0, 0.0),
        _ => (w, 0.0), // ╰ right+up
    };
    let rm = (rx + ry) / 2.0;
    b.shade(|x, y| {
        let d = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
        t / 2.0 - (d - 1.0).abs() * rm + 0.5
    });
}

fn diagonal(b: &mut Bitmap, c: u32) {
    let (w, h) = (b.w as f32, b.h as f32);
    let t = light(b) as f32;
    let norm = (w * w + h * h).sqrt();
    let fwd = c != 0x2572; // ╱ and ╳
    let back = c != 0x2571; // ╲ and ╳
    b.shade(|x, y| {
        let mut cov: f32 = 0.0;
        if fwd {
            cov = cov.max(t / 2.0 - (x / w + y / h - 1.0).abs() * w * h / norm + 0.5);
        }
        if back {
            cov = cov.max(t / 2.0 - (x / w - y / h).abs() * w * h / norm + 0.5);
        }
        cov
    });
}

// ---- block elements --------------------------------------------------------

fn block(b: &mut Bitmap, c: u32) {
    let (w, h) = (b.w as i32, b.h as i32);
    let frac = |len: i32, n: i32| (len * n + 4) / 8; // n eighths, rounded
    match c {
        0x2580 => b.fill(0, 0, w, frac(h, 4), 255),
        0x2581..=0x2588 => b.fill(0, h - frac(h, (c - 0x2580) as i32), w, h, 255),
        0x2589..=0x258F => b.fill(0, 0, frac(w, (0x2590 - c) as i32), h, 255),
        0x2590 => b.fill(w - frac(w, 4), 0, w, h, 255),
        0x2591 => b.fill(0, 0, w, h, 64),
        0x2592 => b.fill(0, 0, w, h, 128),
        0x2593 => b.fill(0, 0, w, h, 192),
        0x2594 => b.fill(0, 0, w, frac(h, 1), 255),
        0x2595 => b.fill(w - frac(w, 1), 0, w, h, 255),
        _ => {
            // Quadrants 2596..259F: bits UL=1 UR=2 LL=4 LR=8.
            let mask = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14][(c - 0x2596) as usize];
            quads(b, mask);
        }
    }
}

fn quads(b: &mut Bitmap, mask: u8) {
    let (w, h) = (b.w as i32, b.h as i32);
    let (mx, my) = (w / 2 + (w & 1), h / 2 + (h & 1)); // odd sizes: extra px to the first half
    if mask & 1 != 0 {
        b.fill(0, 0, mx, my, 255);
    }
    if mask & 2 != 0 {
        b.fill(mx, 0, w, my, 255);
    }
    if mask & 4 != 0 {
        b.fill(0, my, mx, h, 255);
    }
    if mask & 8 != 0 {
        b.fill(mx, my, w, h, 255);
    }
}

// ---- braille / sextants ----------------------------------------------------

fn braille(b: &mut Bitmap, bits: u8) {
    let (w, h) = (b.w as f32, b.h as f32);
    // Dot centers: 2 columns × 4 rows; radius limited by the tighter pitch.
    let r = (w / 4.0).min(h / 8.0) * 0.8;
    let r = r.max(0.75);
    // Bit → (col, row): dots 1-3 left column top→bottom, 4-6 right, 7 left-bottom, 8 right-bottom.
    const POS: [(f32, f32); 8] = [
        (0.0, 0.0),
        (0.0, 1.0),
        (0.0, 2.0),
        (1.0, 0.0),
        (1.0, 1.0),
        (1.0, 2.0),
        (0.0, 3.0),
        (1.0, 3.0),
    ];
    for (i, (cx, cy)) in POS.iter().enumerate() {
        if bits >> i & 1 == 0 {
            continue;
        }
        let (px, py) = (w * (cx * 2.0 + 1.0) / 4.0, h * (cy * 2.0 + 1.0) / 8.0);
        b.shade(|x, y| r - ((x - px).powi(2) + (y - py).powi(2)).sqrt() + 0.5);
    }
}

/// U+1FB00..U+1FB3B: 2×3 block mosaics (the patterns for left/right half and
/// empty/full are omitted from the block, hence the index skips).
fn sextant(b: &mut Bitmap, c: u32) {
    let mut mask = (c - 0x1FB00) as u8 + 1;
    if mask >= 21 {
        mask += 1;
    }
    if mask >= 42 {
        mask += 1;
    }
    let (w, h) = (b.w as i32, b.h as i32);
    let mx = w / 2 + (w & 1);
    for bit in 0..6 {
        if mask >> bit & 1 == 0 {
            continue;
        }
        let (col, row) = (bit & 1, bit / 2);
        let (y0, y1) = (row * h / 3, (row + 1) * h / 3);
        let (x0, x1) = if col == 0 { (0, mx) } else { (mx, w) };
        b.fill(x0, y0, x1, y1, 255);
    }
}

// ---- powerline -------------------------------------------------------------

/// E0B0 ▶ solid, E0B1 ▷ outline, E0B2 ◀ solid, E0B3 ◁ outline,
/// E0B4/E0B6 solid semicircles, E0B5/E0B7 semicircle outlines.
fn powerline(b: &mut Bitmap, c: u32) {
    let (w, h) = (b.w as f32, b.h as f32);
    let t = light(b) as f32;
    let flip = matches!(c, 0xE0B2 | 0xE0B3 | 0xE0B6 | 0xE0B7);
    let outline = c & 1 == 1;
    let round = c >= 0xE0B4;
    b.shade(|x, y| {
        let x = if flip { w - x } else { x };
        // `edge`: signed distance (px) inside the shape's boundary.
        let inside = if round {
            // Half-ellipse with its flat side at x = 0, radii (w, h/2).
            let d = ((x / w).powi(2) + ((y - h / 2.0) / (h / 2.0)).powi(2)).sqrt();
            (1.0 - d) * (w + h / 2.0) / 2.0
        } else {
            // Triangle: apex at (w, h/2); boundary x = w * (1 - |y - h/2| / (h/2)).
            let edge = w * (1.0 - (y - h / 2.0).abs() / (h / 2.0));
            (edge - x) * (h / 2.0) / ((h / 2.0).powi(2) + w * w).sqrt()
        };
        if outline {
            t / 2.0 - (inside - t / 2.0).abs() + 0.5
        } else {
            inside + 0.5
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const CW: u32 = 16;
    const CH: u32 = 34;

    fn bm(c: char) -> Bitmap {
        draw(c, CW, CH).unwrap_or_else(|| panic!("no sprite for {c:?}"))
    }

    fn at(b: &Bitmap, x: u32, y: u32) -> u8 {
        b.px[(y * b.w + x) as usize]
    }

    fn ink(b: &Bitmap) -> usize {
        b.px.iter().filter(|p| **p > 0).count()
    }

    #[test]
    fn covers_box_block_braille_ranges() {
        for c in 0x2500..=0x259Fu32 {
            let ch = char::from_u32(c).unwrap();
            assert!(handles(ch), "{c:#x}");
            assert!(ink(&bm(ch)) > 0, "{c:#x} drew nothing");
        }
        // Braille: blank is empty, every other pattern has ink.
        assert_eq!(ink(&bm('\u{2800}')), 0);
        for c in 0x2801..=0x28FFu32 {
            assert!(ink(&bm(char::from_u32(c).unwrap())) > 0, "{c:#x}");
        }
        assert!(!handles('A') && draw('A', CW, CH).is_none());
    }

    #[test]
    fn straight_lines_reach_cell_edges_so_cells_tile() {
        let h = bm('─');
        let y = CH / 2;
        assert_eq!((at(&h, 0, y), at(&h, CW - 1, y)), (255, 255));
        let v = bm('│');
        let x = CW / 2;
        assert_eq!((at(&v, x, 0), at(&v, x, CH - 1)), (255, 255));
        // Heavy is thicker than light.
        assert!(ink(&bm('━')) > ink(&h));
        // Full block fills every pixel.
        assert!(bm('█').px.iter().all(|p| *p == 255));
    }

    #[test]
    fn corners_and_junctions_join() {
        let c = bm('┌'); // right + down: nothing left of center or above it
        assert_eq!(at(&c, 0, CH / 2), 0);
        assert_eq!(at(&c, CW / 2, 0), 0);
        assert_eq!((at(&c, CW - 1, CH / 2), at(&c, CW / 2, CH - 1)), (255, 255));
        // The corner pixel itself is filled (no notch).
        assert_eq!(at(&c, CW / 2, CH / 2), 255);
        let cross = bm('┼');
        for (x, y) in [(0, CH / 2), (CW - 1, CH / 2), (CW / 2, 0), (CW / 2, CH - 1)] {
            assert_eq!(at(&cross, x, y), 255);
        }
    }

    #[test]
    fn double_lines_have_a_gap_between_strips() {
        let h = bm('═');
        let col: Vec<u8> = (0..CH).map(|y| at(&h, 3, y)).collect();
        let runs = col.windows(2).filter(|w| w[0] == 0 && w[1] == 255).count();
        assert_eq!(runs, 2, "two strips: {col:?}");
        // ╬ has a hole at the center between the strips.
        let x = bm('╬');
        assert_eq!(at(&x, CW / 2, CH / 2), 0);
    }

    #[test]
    fn dashes_leave_gaps_and_tile() {
        let d = bm('┄');
        let y = (CH - light(&d) as u32) / 2;
        let row: Vec<u8> = (0..CW).map(|x| at(&d, x, y)).collect();
        assert!(row.contains(&0) && row.contains(&255), "{row:?}");
        assert!(ink(&d) < ink(&bm('─')));
    }

    #[test]
    fn braille_dots_map_to_bit_positions() {
        // Dot 1 = top-left quadrant only; dot 8 = bottom-right quadrant only.
        let tl = bm('\u{2801}');
        let br = bm('\u{2880}');
        let quad = |b: &Bitmap, x0: u32, y0: u32| -> usize {
            (y0..y0 + CH / 2)
                .flat_map(|y| (x0..x0 + CW / 2).map(move |x| (x, y)))
                .filter(|&(x, y)| at(b, x, y) > 0)
                .count()
        };
        assert!(quad(&tl, 0, 0) > 0 && quad(&tl, CW / 2, 0) == 0 && quad(&tl, 0, CH / 2) == 0);
        assert!(quad(&br, CW / 2, CH / 2) > 0 && quad(&br, 0, 0) == 0);
        // All eight dots = 8× a single dot's ink (dots do not overlap).
        assert_eq!(ink(&bm('\u{28FF}')), 8 * ink(&tl));
    }

    #[test]
    fn quadrants_and_halves() {
        let ul = bm('▘');
        assert_eq!((at(&ul, 0, 0), at(&ul, CW - 1, CH - 1)), (255, 0));
        let lower_half = bm('▄');
        assert_eq!(
            (at(&lower_half, 0, CH / 2 - 1), at(&lower_half, 0, CH / 2)),
            (0, 255)
        );
        assert_eq!(at(&bm('▒'), 5, 5), 128);
        // ▙ = UL + LL + LR: everything except the upper right.
        let q = bm('▙');
        assert_eq!(
            (at(&q, CW - 1, 0), at(&q, 0, 0), at(&q, CW - 1, CH - 1)),
            (0, 255, 255)
        );
    }

    #[test]
    fn sextants_and_powerline_draw() {
        assert!(handles('\u{1FB00}'));
        // 1FB00 = pattern 1 = upper-left cell only.
        let s = bm('\u{1FB00}');
        assert_eq!((at(&s, 0, 0), at(&s, CW - 1, CH - 1)), (255, 0));
        // 1FB3B = pattern 62 (0b111110) = everything but the upper-left.
        let s = bm('\u{1FB3B}');
        assert_eq!((at(&s, 0, 0), at(&s, CW - 1, CH - 1)), (0, 255));
        let tri = bm('\u{E0B0}');
        // Solid right triangle: base on the left edge, apex on the right-middle,
        // empty in the right corners.
        assert!(at(&tri, 0, CH / 2) > 200 && at(&tri, CW - 2, CH / 2) > 100);
        assert_eq!((at(&tri, CW - 1, 0), at(&tri, CW - 1, CH - 1)), (0, 0));
        let left = bm('\u{E0B2}');
        assert!(at(&left, CW - 1, CH / 2) > 200 && at(&left, 0, 0) == 0);
    }
}
