//! Frame glyphs → GPU glyph instances. Rows are laid out independently and cached by
//! a hash of their content, so an unchanged row costs one hash and a memcpy and
//! shaping (ligatures, clusters, emoji) only runs for rows that changed.

use std::collections::HashMap;

use super::{Face, Font, Slot, face_ref};
use crate::colors::Rgba;
use crate::vt::{Frame, GlyphCell};

/// One drawn glyph quad: pixel position/size, atlas pixel origin, tint color.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphInst {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// Instances for one row, `y` relative to the row's top.
#[derive(Default)]
struct RowInsts {
    mono: Vec<GlyphInst>,
    color: Vec<GlyphInst>,
    used: u64,
}

/// Most rows kept; past this the cache drops everything not used this frame.
const MAX_ROWS: usize = 2048;

/// FxHash-style mixing: the row hash runs on every cell of every frame.
#[derive(Clone, Copy)]
struct Hash(u64);

impl Hash {
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

pub struct TextLayout {
    pub font: Font,
    ligatures: bool,
    rows: HashMap<u64, RowInsts>,
    /// Recycled instance vectors from evicted rows.
    spare: Vec<RowInsts>,
    /// Layout scratch (reused across rows/frames).
    run: String,
    /// Emoji cluster text being assembled (scratch).
    cluster: String,
    /// Grid width of the last frame laid out (bounds the VS16 overflow).
    cols: u16,
    run_cols: Vec<u16>,
    /// Tint per char of `run` (parallel to `run_cols`).
    run_fgs: Vec<Rgba>,
    run_starts: Vec<u32>,
    /// (source byte, glyph id, x offset in the cluster) per shaped glyph.
    shaped: Vec<(u32, u16, f32)>,
    frame_no: u64,
    pub mono: Vec<GlyphInst>,
    pub color: Vec<GlyphInst>,
}

impl TextLayout {
    pub fn new(font: Font) -> Self {
        Self {
            font,
            ligatures: true,
            rows: HashMap::new(),
            spare: Vec::new(),
            run: String::new(),
            cluster: String::new(),
            cols: 0,
            run_cols: Vec::new(),
            run_fgs: Vec::new(),
            run_starts: Vec::new(),
            shaped: Vec::new(),
            frame_no: 0,
            mono: Vec::new(),
            color: Vec::new(),
        }
    }

    pub fn ligatures(&self) -> bool {
        self.ligatures
    }

    /// Toggle `calt`/`liga` shaping. Cached rows were shaped under the old setting.
    pub fn set_ligatures(&mut self, on: bool) {
        if self.ligatures != on {
            self.ligatures = on;
            self.rows.clear();
        }
    }

    /// Lay out every glyph of `f` into `self.mono` / `self.color` (cleared first).
    pub fn layout(&mut self, f: &Frame) {
        self.frame_no += 1;
        self.cols = f.cols.min(u16::MAX as u32) as u16;
        // Draw twice at most: an atlas wipe mid-frame invalidates the slots handed out
        // earlier in the same pass.
        for _ in 0..2 {
            let generation = self.font.generation;
            self.layout_rows(f);
            if generation == self.font.generation {
                break;
            }
            self.rows.clear();
        }
        if self.rows.len() > MAX_ROWS {
            let now = self.frame_no;
            let spare = &mut self.spare;
            self.rows.retain(|_, r| {
                let keep = r.used == now;
                if !keep {
                    spare.push(RowInsts {
                        mono: std::mem::take(&mut r.mono),
                        color: std::mem::take(&mut r.color),
                        used: 0,
                    });
                }
                keep
            });
        }
    }

    fn layout_rows(&mut self, f: &Frame) {
        self.mono.clear();
        self.color.clear();
        let ch = self.font.cell[1];
        let mut i = 0;
        while i < f.glyphs.len() {
            let row = f.glyphs[i].row;
            let end = i + f.glyphs[i..].iter().take_while(|g| g.row == row).count();
            let cells = &f.glyphs[i..end];
            i = end;
            let mut h = Hash(self.ligatures as u64);
            h.add(self.cols as u64);
            for g in cells {
                h.add(g.col as u64 | (g.style as u64) << 16 | (g.wide as u64) << 24);
                h.add(g.ch as u64);
                h.add(u32::from_ne_bytes(g.fg) as u64);
                if let Some(x) = &g.extra {
                    for b in x.bytes() {
                        h.add(b as u64);
                    }
                }
            }
            if !self.rows.contains_key(&h.0) {
                let mut r = self.spare.pop().unwrap_or_default();
                r.mono.clear();
                r.color.clear();
                self.layout_row(cells, &mut r);
                self.rows.insert(h.0, r);
            }
            let r = self.rows.get_mut(&h.0).expect("just inserted");
            r.used = self.frame_no;
            let y = row as f32 * ch;
            for (src, dst) in [(&r.mono, &mut self.mono), (&r.color, &mut self.color)] {
                dst.extend(src.iter().map(|g| GlyphInst {
                    pos: [g.pos[0], g.pos[1] + y],
                    ..*g
                }));
            }
        }
    }

    /// Lay out one row's cells (sorted by column) with `y` relative to the row top.
    fn layout_row(&mut self, cells: &[GlyphCell], out: &mut RowInsts) {
        let mut i = 0;
        while i < cells.len() {
            let g = &cells[i];
            if let Some(end) = self.place_cluster(cells, i, out) {
                i = end;
                continue;
            }
            if self.place_special(g, out) {
                i += 1;
                continue;
            }
            // Text run: adjacent plain-text cells of one style, shaped together.
            let style = g.style;
            let mut j = i;
            self.run.clear();
            self.run_cols.clear();
            self.run_fgs.clear();
            while j < cells.len() {
                let c = &cells[j];
                let adjacent = j == i || c.col == cells[j - 1].col + 1;
                if !adjacent || c.style != style || self.is_special(c) {
                    break;
                }
                self.run.push(c.ch);
                self.run_cols.push(c.col);
                self.run_fgs.push(c.fg);
                if let Some(x) = &c.extra {
                    self.run.push_str(x);
                    // Marks share the base cell's column.
                    let marks = x.chars().count();
                    self.run_cols.extend(std::iter::repeat_n(c.col, marks));
                    self.run_fgs.extend(std::iter::repeat_n(c.fg, marks));
                }
                j += 1;
            }
            if j == i {
                // Not special but not plain text either (no face has it): skip the cell.
                i += 1;
                continue;
            }
            self.shape_run(style, out);
            i = j;
        }
    }

    /// Merge `cells[i..]` into one emoji grapheme cluster and draw it as a single color
    /// glyph. The VT keeps each emoji, regional indicator and skin-tone modifier in its
    /// own cell and only puts ZWJ/VS16 into the previous cell's marks, so sequences are
    /// rejoined here: a cell is absorbed when the cluster so far ends in ZWJ, when it
    /// is an emoji modifier, or when it completes a regional-indicator pair. Absorbed
    /// cells draw nothing (their backgrounds are untouched). Returns the index after
    /// the last cell consumed, or `None` when `cells[i]` is not an emoji cluster (or the
    /// emoji font has no glyph for it, leaving the per-cell fallback to handle it).
    fn place_cluster(
        &mut self,
        cells: &[GlyphCell],
        i: usize,
        out: &mut RowInsts,
    ) -> Option<usize> {
        let g = &cells[i];
        if is_plain_ascii(g) || !self.wants_emoji_start(g, cells.get(i + 1)) {
            return None;
        }
        self.cluster.clear();
        push_cell(&mut self.cluster, g);
        let mut end = i + 1;
        let mut ri = is_regional(g.ch) as u8; // regional indicators in the cluster so far
        let mut last = g;
        while let Some(n) = cells.get(end) {
            // Wide cells own the spacer column after them.
            let span = 1 + last.wide as u16;
            let joins = n.col == last.col + span
                && n.style == g.style
                && (self.cluster.ends_with('\u{200D}')
                    || is_modifier(n.ch)
                    || (is_regional(n.ch) && ri == 1));
            if !joins {
                break;
            }
            ri += is_regional(n.ch) as u8;
            push_cell(&mut self.cluster, n);
            last = n;
            end += 1;
        }
        let merged = end > i + 1;
        let span = if g.wide || merged {
            2
        } else if self.cluster.contains('\u{FE0F}') && self.blank_after(cells, i) {
            // Emoji presentation in a 1-wide cell: overflow into the empty next column.
            2
        } else {
            1
        };
        let s = self.font.emoji(&self.cluster, span)?;
        let [cw, _] = self.font.cell;
        push(out, s, g.col as f32 * cw, 0.0, self.font.baseline, g.fg);
        Some(end)
    }

    /// Is the column right after `cells[i]` empty (no glyph drawn there)?
    fn blank_after(&self, cells: &[GlyphCell], i: usize) -> bool {
        cells.get(i + 1).is_none_or(|n| n.col != cells[i].col + 1) && cells[i].col + 1 < self.cols
    }

    /// Could `g` start an emoji cluster? Like `wants_emoji`, plus bare regional
    /// indicators and bases followed by a modifier cell, which the per-cell test misses.
    fn wants_emoji_start(&self, g: &GlyphCell, next: Option<&GlyphCell>) -> bool {
        self.wants_emoji(g)
            || self.font.has_emoji()
                && (is_regional(g.ch)
                    || next
                        .is_some_and(|n| is_modifier(n.ch) && n.col == g.col + 1 + g.wide as u16))
    }

    /// Sprites, color emoji clusters and symbol-font glyphs: drawn one cell at a time.
    fn is_special(&self, g: &GlyphCell) -> bool {
        if is_plain_ascii(g) {
            return false;
        }
        super::sprite::handles(g.ch)
            || self.wants_emoji(g)
            || self.font.text_face(g.ch, g.style) != Some(Face::Mono(g.style & 3))
    }

    /// Does this cell render from the emoji font?
    fn wants_emoji(&self, g: &GlyphCell) -> bool {
        if !self.font.has_emoji() {
            return false;
        }
        let vs16 = g
            .extra
            .as_deref()
            .is_some_and(|x| x.contains(['\u{FE0F}', '\u{200D}', '\u{20E3}']));
        let base_in_text_font = self.font.text_face(g.ch, g.style).is_some();
        vs16 || g.wide && !base_in_text_font || is_emoji_scalar(g.ch) && g.wide
    }

    /// Draw `g` if it is not ordinary text; `false` = leave it to the run shaper.
    fn place_special(&mut self, g: &GlyphCell, out: &mut RowInsts) -> bool {
        if is_plain_ascii(g) {
            return false;
        }
        let [cw, _] = self.font.cell;
        let base = self.font.baseline;
        if let Some(s) = self.font.sprite(g.ch) {
            push(out, s, g.col as f32 * cw, 0.0, base, g.fg);
            return true;
        }
        if super::sprite::handles(g.ch) {
            return true; // sprite failed to allocate: draw nothing this frame
        }
        if self.wants_emoji(g) {
            // `place_cluster` found no color glyph for this cell: draw its base character
            // from the text faces if any has it, otherwise nothing.
            return self.font.text_face(g.ch, g.style).is_none();
        }
        match self.font.text_face(g.ch, g.style) {
            Some(Face::Mono(_)) => false,
            Some(face) => {
                if let Some(id) = face_ref(face).map(|f| f.charmap().map(g.ch))
                    && let Some(s) = self.font.glyph_id(face, id)
                {
                    push(out, s, g.col as f32 * cw, 0.0, base, g.fg);
                }
                true
            }
            None => true,
        }
    }

    /// Shape `self.run` (one `cells` slice, one style) and place the glyphs. Calt
    /// ligatures keep one glyph per source char, the first drawn wide over the next
    /// cells (whose own glyphs become empty spacers); true ligatures yield one glyph
    /// for the whole cluster, drawn from its first cell.
    fn shape_run(&mut self, style: u8, out: &mut RowInsts) {
        let [cw, _] = self.font.cell;
        let base = self.font.baseline;
        let face = Face::Mono(style & 3);
        let Some(fr) = face_ref(face) else { return };
        // Alphanumeric-only runs never ligate (probed over every ASCII 2/3-gram of the four
        // faces) and carry no marks: map through the cmap and skip the shaper.
        if self.run.bytes().all(|b| b.is_ascii_alphanumeric()) {
            let cm = fr.charmap();
            for (n, ch) in self.run.chars().enumerate() {
                if let Some(s) = self.font.glyph_id(face, cm.map(ch)) {
                    let col = self.run_cols[n];
                    push(out, s, col as f32 * cw, 0.0, base, self.run_fgs[n]);
                }
            }
            return;
        }
        let on = self.ligatures as u16;
        let feats = [("calt", on), ("liga", on)];
        // Shape into `shaped` first: rasterizing needs `&mut self.font`, which the
        // shaper (built from `self.font.shaper`) borrows.
        self.shaped.clear();
        {
            let shaped = &mut self.shaped;
            let mut shaper = self
                .font
                .shaper
                .builder(fr)
                .size(self.font.ppem)
                .features(&feats)
                .build();
            shaper.add_str(&self.run);
            shaper.shape_with(|cl| {
                let mut pen = 0.0f32;
                for g in cl.glyphs {
                    shaped.push((cl.source.start, g.id, pen + g.x));
                    pen += g.advance;
                }
            });
        }
        // `run_cols[n]` is the column of the n-th char; map byte offsets to char indices.
        self.run_starts.clear();
        self.run_starts
            .extend(self.run.char_indices().map(|(b, _)| b as u32));
        for &(src, id, dx) in &self.shaped {
            let Ok(n) = self.run_starts.binary_search(&src) else {
                continue;
            };
            let (col, fg) = (self.run_cols[n], self.run_fgs[n]);
            if let Some(s) = self.font.glyph_id(face, id) {
                push(out, s, col as f32 * cw + dx, 0.0, base, fg);
            }
        }
    }
}

/// Append a quad for `s` with its origin `x` px from the row left and the cell top at `y`.
fn push(out: &mut RowInsts, s: Slot, x: f32, y: f32, baseline: f32, fg: Rgba) {
    let inst = GlyphInst {
        pos: [x + s.left as f32, y + baseline - s.top as f32],
        size: [s.w as f32, s.h as f32],
        uv: [s.x as f32, s.y as f32],
        color: fg,
    };
    if s.color {
        out.color.push(inst);
    } else {
        out.mono.push(inst);
    }
}

fn push_cell(out: &mut String, g: &GlyphCell) {
    out.push(g.ch);
    if let Some(x) = &g.extra {
        out.push_str(x);
    }
}

fn is_regional(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Emoji skin-tone modifiers (Fitzpatrick).
fn is_modifier(c: char) -> bool {
    matches!(c as u32, 0x1F3FB..=0x1F3FF)
}

/// Printable ASCII without marks: always JetBrains Mono text, never a sprite/emoji.
fn is_plain_ascii(g: &GlyphCell) -> bool {
    g.ch.is_ascii_graphic() && g.extra.is_none() && !g.wide
}

/// Characters with default emoji presentation that a wide cell implies (the VT sizes
/// them 2 cells): the main emoji blocks plus a few legacy symbols.
fn is_emoji_scalar(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0x2B50 | 0x2B55 | 0x231A..=0x231B
        | 0x23E9..=0x23F3 | 0x1F1E6..=0x1F1FF)
}

#[cfg(test)]
mod tests;
