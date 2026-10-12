use std::time::Instant;

use super::*;
use crate::colors::Palette;
use crate::vt::Vt;

/// No ligature-forming punctuation: every cell has its own visible glyph.
const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

const FG: Rgba = [200, 200, 200, 255];

fn pal() -> Palette {
    use alacritty_terminal::vte::ansi::Rgb;
    Palette::new(
        Rgb {
            r: 200,
            g: 200,
            b: 200,
        },
        Rgb {
            r: 10,
            g: 10,
            b: 10,
        },
        None,
    )
}

fn cell(col: u16, ch: char) -> GlyphCell {
    GlyphCell {
        col,
        row: 0,
        ch,
        style: 0,
        fg: FG,
        extra: None,
        wide: false,
    }
}

fn frame(cells: Vec<GlyphCell>) -> Frame {
    Frame {
        cols: 144,
        rows: 44,
        glyphs: cells,
        ..Default::default()
    }
}

fn text(s: &str) -> Vec<GlyphCell> {
    s.chars()
        .enumerate()
        .filter(|(_, c)| *c != ' ')
        .map(|(i, c)| cell(i as u16, c))
        .collect()
}

fn layout(s: &str, ligatures: bool) -> (TextLayout, Vec<GlyphInst>) {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    t.set_ligatures(ligatures);
    t.layout(&frame(text(s)));
    let v = t.mono.clone();
    (t, v)
}

#[test]
fn ligature_arrow_and_ne_draw_wide_over_two_cells() {
    for s in ["->", "!=", "=>", "=="] {
        let (t, on) = layout(s, true);
        let (_, off) = layout(s, false);
        let cw = t.font.cell[0];
        // With ligatures the glyph quads differ from the plain ones…
        assert_ne!(on, off, "{s}: ligatures changed nothing");
        // …and some quad is wider than one cell (the ligature spans both).
        assert!(
            on.iter().any(|g| g.size[0] > cw * 1.5),
            "{s}: no glyph wider than a cell: {on:?}"
        );
        // Plain shaping never exceeds one cell.
        assert!(off.iter().all(|g| g.size[0] <= cw), "{s} plain: {off:?}");
        // The wide glyph is anchored at the first cell and reaches into the second.
        let wide = on.iter().find(|g| g.size[0] > cw * 1.5).unwrap();
        assert!(wide.pos[0] < cw && wide.pos[0] + wide.size[0] > cw, "{s}");
    }
}

#[test]
fn ligature_does_not_cross_gaps_or_plain_text() {
    let (t, with_gap) = layout("- >", true);
    let cw = t.font.cell[0];
    assert!(with_gap.iter().all(|g| g.size[0] <= cw), "{with_gap:?}");
    // Plain words are untouched by the ligature setting.
    let (_, a) = layout("hello", true);
    let (_, b) = layout("hello", false);
    assert_eq!(a, b);
    assert_eq!(a.len(), 5);
}

#[test]
fn toggling_ligatures_invalidates_cached_rows() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    let f = frame(text("->"));
    t.layout(&f);
    let on = t.mono.clone();
    t.set_ligatures(false);
    t.layout(&f);
    assert_ne!(on, t.mono);
    t.set_ligatures(true);
    t.layout(&f);
    assert_eq!(on, t.mono);
}

#[test]
fn box_braille_and_blocks_are_cell_sized_sprites() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    let [cw, ch] = t.font.cell;
    let cells: Vec<GlyphCell> = ['─', '│', '█', '\u{28FF}', '▀']
        .iter()
        .enumerate()
        .map(|(i, c)| cell(i as u16, *c))
        .collect();
    t.layout(&frame(cells));
    assert_eq!(t.mono.len(), 5);
    for (i, g) in t.mono.iter().enumerate() {
        // Exactly one cell, on the cell origin: seamless tiling.
        assert_eq!(g.size, [cw, ch]);
        assert_eq!(g.pos, [i as f32 * cw, 0.0]);
    }
}

#[test]
fn rows_offset_by_cell_height() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    let ch = t.font.cell[1];
    let mut a = cell(0, '█');
    let mut b = cell(0, '█');
    a.row = 0;
    b.row = 3;
    t.layout(&frame(vec![a, b]));
    assert_eq!(t.mono.len(), 2);
    assert_eq!(t.mono[1].pos[1] - t.mono[0].pos[1], 3.0 * ch);
}

#[test]
fn simple_and_vs16_emoji_cells_fit_their_span() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    if !t.font.has_emoji() {
        return;
    }
    let [cw, ch] = t.font.cell;
    let simple = GlyphCell {
        wide: true,
        ..cell(6, '😀')
    };
    // Hand-built cells (as snapshot would emit): a wide emoji and a VS16 heart whose
    // next column is occupied, so it must stay within its own cell.
    let heart = GlyphCell {
        extra: Some("\u{fe0f}".into()),
        ..cell(0, '❤')
    };
    let next = cell(1, 'x');
    t.layout(&frame(vec![heart, next, simple]));
    assert_eq!(t.color.len(), 2, "{:?}", t.color);
    assert!(t.mono.len() == 1, "{:?}", t.mono);
    assert!(t.color[0].size[0] <= cw, "{:?}", t.color[0]);
    let wide = t.color[1];
    assert!(wide.size[0] <= cw * 2.0 && wide.size[1] <= ch, "{wide:?}");
}

#[test]
fn symbol_font_icons_still_render_from_the_coverage_atlas() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    t.layout(&frame(vec![cell(0, '\u{F07B}')]));
    assert_eq!((t.mono.len(), t.color.len()), (1, 0));
}

#[test]
fn unchanged_rows_hit_the_cache() {
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    let f = frame(text("hello world"));
    t.layout(&f);
    let first = t.mono.clone();
    let ptr = t.rows.values().next().unwrap().mono.as_ptr();
    t.layout(&f);
    assert_eq!(first, t.mono);
    assert_eq!(t.rows.len(), 1);
    // The cached row's buffer was reused, not rebuilt.
    assert_eq!(t.rows.values().next().unwrap().mono.as_ptr(), ptr);
    // A color change is a different row.
    let mut g = text("hello world");
    g[0].fg = [1, 2, 3, 255];
    t.layout(&frame(g));
    assert_eq!(t.rows.len(), 2);
}

/// Snapshot + layout of a full 144×44 screen; run with
/// `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench`.
#[test]
#[ignore = "timing bench"]
fn screen_bench() {
    let (cols, rows) = (144u16, 44u16);
    let vt = Vt::new(cols, rows, |_| {});
    let mut screen = String::new();
    for r in 0..rows {
        for c in 0..cols {
            screen.push(ALNUM[(r as usize * 7 + c as usize) % ALNUM.len()] as char);
        }
        if r + 1 < rows {
            screen.push_str("\r\n");
        }
    }
    vt.feed(screen.as_bytes());
    let pal = pal();
    let mut f = Frame::default();
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    // Warm: atlas + row cache.
    vt.snapshot(&pal, &mut f);
    t.layout(&f);
    let n = 400;
    let (mut cached, mut cold) = (Vec::new(), Vec::new());
    for i in 0..n {
        let t0 = Instant::now();
        vt.snapshot(&pal, &mut f);
        t.layout(&f);
        cached.push(t0.elapsed().as_micros());
        // Cold: every row's content changes (cache cleared) but glyphs are in the atlas.
        t.set_ligatures(i % 2 == 0);
        let t0 = Instant::now();
        t.layout(&f);
        cold.push(t0.elapsed().as_micros());
    }
    // Worst case: code-like text where most runs go through the shaper.
    let code = Vt::new(cols, rows, |_| {});
    let line = "if (a->b != c && d >= e) { x => y; } // -> ==> <= !== ";
    let screen: Vec<String> = (0..rows)
        .map(|_| line.repeat(3)[..cols as usize].into())
        .collect();
    code.feed(screen.join("\r\n").as_bytes());
    let mut cf = Frame::default();
    code.snapshot(&pal, &mut cf);
    t.layout(&cf);
    let mut shaped = Vec::new();
    for i in 0..n {
        t.set_ligatures(i % 2 == 0);
        let t0 = Instant::now();
        t.layout(&cf);
        shaped.push(t0.elapsed().as_micros());
    }
    shaped.sort_unstable();
    println!(
        "cold layout, code screen (shaper path): p50 {}µs p95 {}µs",
        shaped[n / 2],
        shaped[n * 95 / 100]
    );
    cached.sort_unstable();
    cold.sort_unstable();
    println!(
        "snapshot+layout cached p50 {}µs p95 {}µs | cold layout p50 {}µs p95 {}µs ({} glyphs)",
        cached[n / 2],
        cached[n * 95 / 100],
        cold[n / 2],
        cold[n * 95 / 100],
        t.mono.len()
    );
    assert!(cached[n * 95 / 100] < 1000, "cached p95 over 1 ms");
}

#[test]
fn full_screen_of_ascii_layouts_without_growth() {
    let (cols, rows) = (144u16, 44u16);
    let vt = Vt::new(cols, rows, |_| {});
    let mut screen = String::new();
    for r in 0..rows {
        for c in 0..cols {
            screen.push(ALNUM[(r as usize * 7 + c as usize) % ALNUM.len()] as char);
        }
        if r + 1 < rows {
            screen.push_str("\r\n");
        }
    }
    vt.feed(screen.as_bytes());
    let mut f = Frame::default();
    let mut t = TextLayout::new(Font::new(13.0, 2.0));
    vt.snapshot(&pal(), &mut f);
    t.layout(&f);
    assert_eq!(t.mono.len(), cols as usize * rows as usize);
    let (cap, rowsn) = (t.mono.capacity(), t.rows.len());
    assert_eq!(rowsn, rows as usize);
    // Repeated identical frames: no new cache rows and the output buffer does not grow.
    for _ in 0..50 {
        vt.snapshot(&pal(), &mut f);
        t.layout(&f);
    }
    assert_eq!((t.mono.capacity(), t.rows.len()), (cap, rowsn));
    assert!(t.spare.is_empty());
}

/// A frame's glyphs for `text` laid out as the VT would: each emoji / regional
/// indicator / modifier in its own cell (wide ones skip their spacer column), with
/// ZWJ and VS16 attached to the previous cell's marks.
fn vt_cells(text: &str, cols: u16) -> Frame {
    let vt = Vt::new(cols, 2, |_| {});
    vt.feed(text.as_bytes());
    let mut f = Frame::default();
    vt.snapshot(&pal(), &mut f);
    f
}

fn emoji_layout(text: &str, cols: u16) -> Option<(TextLayout, Frame)> {
    let t = TextLayout::new(Font::new(13.0, 2.0));
    if !t.font.has_emoji() {
        return None; // no system emoji font (not macOS)
    }
    let f = vt_cells(text, cols);
    Some((t, f))
}

#[test]
fn zwj_family_is_one_glyph_over_the_first_two_cells() {
    let Some((mut t, f)) = emoji_layout("👨\u{200d}👩\u{200d}👧", 20) else {
        return;
    };
    // Precondition: the VT really splits the sequence across three wide cells.
    assert_eq!(f.glyphs.iter().filter(|g| g.wide).count(), 3);
    t.layout(&f);
    // Same atlas: the drawn glyph is the family cluster, not just the first face.
    let family = t.font.emoji("👨\u{200d}👩\u{200d}👧", 2).unwrap();
    let man = t.font.emoji("👨", 2).unwrap();
    assert_eq!(t.color[0].uv, [family.x as f32, family.y as f32]);
    assert_ne!((family.x, family.y), (man.x, man.y));
    let [cw, ch] = t.font.cell;
    assert!(t.mono.is_empty());
    assert_eq!(t.color.len(), 1, "family must be one glyph: {:?}", t.color);
    let g = t.color[0];
    assert!(
        g.pos[0] >= 0.0 && g.pos[0] + g.size[0] <= cw * 2.0 + 0.5,
        "{g:?}"
    );
    assert!(g.size[1] <= ch);
}

#[test]
fn flag_is_one_glyph_not_two_regional_indicators() {
    let Some((mut t, f)) = emoji_layout("🇧🇷", 20) else {
        return;
    };
    // The VT gives each indicator its own 1-wide cell.
    assert_eq!(f.glyphs.len(), 2);
    assert!(f.glyphs.iter().all(|g| !g.wide));
    t.layout(&f);
    let cw = t.font.cell[0];
    assert_eq!(t.color.len(), 1, "{:?}", t.color);
    // Fitted over the two indicator cells.
    assert!(t.color[0].size[0] > cw && t.color[0].size[0] <= cw * 2.0);
    // A lone indicator has no glyph of its own: nothing is drawn for it.
    let (mut one, f1) = emoji_layout("🇧", 20).unwrap();
    one.layout(&f1);
    assert!(one.color.is_empty() && one.mono.is_empty());
}

#[test]
fn regional_indicators_pair_only_once() {
    // Four indicators are two flags (cols 0-1 and 2-3), not one blob or flag + strays.
    let Some((mut t, f)) = emoji_layout("🇧🇷🇺🇸", 20) else {
        return;
    };
    t.layout(&f);
    let cw = t.font.cell[0];
    assert_eq!(t.color.len(), 2, "{:?}", t.color);
    assert!(t.color[0].pos[0] < cw);
    assert!(t.color[1].pos[0] >= 2.0 * cw - 0.5, "{:?}", t.color[1]);
    assert_ne!(t.color[0].uv, t.color[1].uv, "two different flags");
    // Three indicators: one flag, and the stray third is not merged into it.
    let (mut three, f3) = emoji_layout("🇧🇷🇺", 20).unwrap();
    three.layout(&f3);
    assert_eq!(three.color.len(), 1);
}

#[test]
fn skin_tone_modifier_merges_into_the_base_emoji() {
    let Some((mut t, f)) = emoji_layout("👍🏽", 20) else {
        return;
    };
    assert_eq!(f.glyphs.iter().filter(|g| g.wide).count(), 2);
    t.layout(&f);
    let cw = t.font.cell[0];
    assert_eq!(
        t.color.len(),
        1,
        "thumb + tone must be one glyph: {:?}",
        t.color
    );
    assert!(t.color[0].size[0] <= cw * 2.0);
    // The toned glyph is a different atlas entry from the plain thumb (same atlas).
    let toned = t.font.emoji("👍🏽", 2).unwrap();
    let plain = t.font.emoji("👍", 2).unwrap();
    assert_eq!(
        [t.color[0].uv[0], t.color[0].uv[1]],
        [toned.x as f32, toned.y as f32]
    );
    assert_ne!((toned.x, toned.y), (plain.x, plain.y));
    // A modifier with no emoji before it is drawn on its own (one glyph, no panic).
    let (mut lone, fl) = emoji_layout("a🏽", 20).unwrap();
    lone.layout(&fl);
    assert_eq!(lone.mono.len(), 1);
    assert_eq!(lone.color.len(), 1);
}

#[test]
fn text_after_a_cluster_is_untouched() {
    let Some((mut t, f)) = emoji_layout("👨\u{200d}👩 hi 🇧🇷!", 30) else {
        return;
    };
    t.layout(&f);
    // 'h', 'i', '!' stay plain mono glyphs; the two sequences are two color glyphs.
    assert_eq!(t.color.len(), 2, "{:?}", t.color);
    assert_eq!(t.mono.len(), 3, "{:?}", t.mono);
}

#[test]
fn heart_with_vs16_overflows_only_into_a_blank_next_cell() {
    let Some((mut t, alone)) = emoji_layout("❤\u{fe0f}", 20) else {
        return;
    };
    let cw = t.font.cell[0];
    assert!(!alone.glyphs[0].wide);
    t.layout(&alone);
    assert_eq!(t.color.len(), 1);
    let roomy = t.color[0];
    // Blank next cell: drawn at 2-cell size.
    assert!(roomy.size[0] > cw, "{roomy:?} cw {cw}");
    assert!(roomy.size[0] <= cw * 2.0);
    // A glyph right after it: fits its own cell.
    let packed = vt_cells("❤\u{fe0f}x", 20);
    t.layout(&packed);
    assert_eq!((t.color.len(), t.mono.len()), (1, 1));
    assert!(t.color[0].size[0] <= cw, "{:?}", t.color[0]);
    // Last column of the grid: nowhere to overflow.
    let edge = vt_cells("\x1b[1;2H❤\u{fe0f}", 2);
    t.layout(&edge);
    assert_eq!(t.color.len(), 1);
    assert!(t.color[0].size[0] <= cw);
}

#[test]
fn clusters_are_cached_per_row_and_follow_grid_width() {
    let Some((mut t, f)) = emoji_layout("👨\u{200d}👩\u{200d}👧 🇧🇷", 20) else {
        return;
    };
    t.layout(&f);
    let first = t.color.clone();
    assert_eq!(first.len(), 2);
    t.layout(&f);
    assert_eq!(first, t.color);
    assert_eq!(t.rows.len(), 1);
    // The same cells in a narrower grid are a different row (VS16 overflow depends on it).
    let mut narrow = vt_cells("👨\u{200d}👩\u{200d}👧 🇧🇷", 20);
    narrow.cols = 10;
    t.layout(&narrow);
    assert_eq!(t.rows.len(), 2);
}

#[test]
fn reported_line_renders_four_color_glyphs() {
    // The exact line from the live screenshot: family, flag, VS16 heart, toned thumb.
    let Some((mut t, f)) = emoji_layout("emoji: 👨\u{200d}👩\u{200d}👧 🇧🇷 ❤\u{fe0f} 👍🏽", 40)
    else {
        return;
    };
    t.layout(&f);
    let cw = t.font.cell[0];
    // "emoji:" is 6 plain glyphs; everything after is exactly four color glyphs.
    assert_eq!(t.mono.len(), 6, "{:?}", t.mono);
    assert_eq!(t.color.len(), 4, "{:?}", t.color);
    // Left to right, none overlapping the next.
    for w in t.color.windows(2) {
        assert!(w[0].pos[0] + w[0].size[0] <= w[1].pos[0] + 0.5, "{w:?}");
    }
    // The heart overflows to ~2 cells (blank column after it).
    assert!(t.color[2].size[0] > cw, "{:?}", t.color[2]);
}
