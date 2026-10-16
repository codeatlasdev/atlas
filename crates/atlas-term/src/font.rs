//! Bundled JetBrains Mono (OFL) + Symbols Nerd Font Mono (MIT, `fonts/`) rasterized
//! with swash into shelf-packed atlases: an R8 coverage atlas (text, sprites) and an
//! RGBA atlas (color emoji, loaded at runtime from the system's emoji font).

mod layout;
mod sprite;
mod system;

use std::collections::HashMap;
use std::sync::LazyLock;

use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content, image::Image};
use swash::shape::ShapeContext;
use swash::zeno::Format;
use swash::{FontRef, GlyphId, NormalizedCoord};

use system::Family;

pub use layout::{GlyphInst, TextLayout};

use crate::vt::{BOLD, ITALIC};

const FACES: [&[u8]; 4] = [
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../fonts/JetBrainsMono-BoldItalic.ttf"),
];
/// Symbol fallback (powerline, nerd icons); no bold/italic variant.
const NF: &[u8] = include_bytes!("../fonts/SymbolsNerdFontMono-Regular.ttf");

/// System color-emoji font. Not bundled (Apple's license); other platforms have none.
const EMOJI_PATH: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";

/// Side of the R8 (coverage) atlas.
pub const ATLAS: u32 = 2048;
/// Side of the RGBA (color emoji) atlas.
pub const COLOR_ATLAS: u32 = 1024;

/// A rasterized glyph: atlas rect + bearing, physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub left: i16,
    pub top: i16,
    /// Lives in the RGBA atlas: sampled as-is instead of tinted by the cell color.
    pub color: bool,
}

/// Which face a glyph id belongs to (cache key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// JetBrains Mono, style bits (`BOLD | ITALIC`).
    Mono(u8),
    Symbols,
}

/// A shelf-packed texture atlas; rows `dirty.0..dirty.1` changed since the last upload.
pub struct Atlas {
    pub pixels: Vec<u8>,
    pub side: u32,
    pub bpp: u32,
    pub dirty: Option<(u32, u32)>,
    shelf: (u32, u32, u32),
}

impl Atlas {
    fn new(side: u32, bpp: u32) -> Self {
        Self {
            pixels: vec![0; (side * side * bpp) as usize],
            side,
            bpp,
            dirty: None,
            shelf: (0, 0, 0),
        }
    }

    /// Shelf packing: (x, y, shelf height); 1px gutter against sampling bleed.
    /// `None` = full (the next call after a `clear` starts over).
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (mut x, mut y, mut sh) = self.shelf;
        if x + w + 1 > self.side {
            (x, y, sh) = (0, y + sh + 1, 0);
        }
        if y + h + 1 > self.side {
            self.shelf.1 = u32::MAX;
            return None;
        }
        self.shelf = (x + w + 1, y, sh.max(h));
        Some((x, y))
    }

    fn full(&self) -> bool {
        self.shelf.1 == u32::MAX
    }

    fn clear(&mut self) {
        self.pixels.fill(0);
        self.shelf = (0, 0, 0);
        self.dirty = Some((0, self.side));
    }

    /// Copy a `w × h` bitmap (`bpp` bytes per pixel, tightly packed) into a new slot.
    fn put(&mut self, data: &[u8], w: u32, h: u32) -> Option<(u32, u32)> {
        let (x, y) = self.pack(w, h)?;
        let bpp = self.bpp;
        for r in 0..h {
            let src = &data[(r * w * bpp) as usize..((r + 1) * w * bpp) as usize];
            let at = (((y + r) * self.side + x) * bpp) as usize;
            self.pixels[at..at + (w * bpp) as usize].copy_from_slice(src);
        }
        let d = self.dirty.get_or_insert((y, y + h));
        *d = (d.0.min(y), d.1.max(y + h));
        Some((x, y))
    }
}

/// The user's text font request (`font-family` candidates + `font-thicken`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontSpec {
    /// Candidates in order; the first installed family wins, none = bundled JetBrains Mono.
    pub families: Vec<String>,
    /// Embolden text glyphs slightly (Ghostty `font-thicken`).
    pub thicken: bool,
}

/// What the requested families resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontResolution {
    /// Family in use ("JetBrains Mono" for the bundled font).
    pub family: String,
    /// Index of the matching candidate; `None` = bundled fallback.
    pub matched: Option<usize>,
}

pub struct Font {
    ppem: f32,
    /// System family used for the text faces; `None` = bundled JetBrains Mono.
    family: Option<Family>,
    thicken: bool,
    /// Cell width/height and baseline from the top of the cell, physical pixels.
    pub cell: [f32; 2],
    pub baseline: f32,
    /// Thickness and vertical position (from baseline) for underline, physical px.
    pub underline: (f32, f32),
    /// Thickness and vertical position (from baseline) for strikethrough, physical px.
    pub strikethrough: (f32, f32),
    ctx: ScaleContext,
    pub(crate) shaper: ShapeContext,
    /// Glyph id → slot (`None` = blank / not rasterizable).
    cache: HashMap<(Face, GlyphId), Option<Slot>>,
    sprites: HashMap<char, Option<Slot>>,
    /// Emoji clusters are keyed by their text and the cell span they were fitted to.
    emoji: HashMap<(Box<str>, u8), Option<Slot>>,
    pub gray: Atlas,
    pub color: Atlas,
    /// Bumped whenever an atlas is wiped: slots handed out earlier are stale.
    pub generation: u32,
    image: Image,
}

/// Name reported when the bundled font is in use.
pub const BUNDLED_FAMILY: &str = "JetBrains Mono";

fn bundled(style: u8) -> FontRef<'static> {
    FontRef::from_index(FACES[style as usize & 3], 0).expect("bundled font")
}

fn symbols() -> FontRef<'static> {
    FontRef::from_index(NF, 0).expect("bundled nerd font")
}

/// The system emoji font bytes, read on first use (the file is ~190 MB, so never at
/// startup). The static owns the buffer for the process lifetime.
static EMOJI_DATA: LazyLock<Option<Vec<u8>>> = LazyLock::new(|| std::fs::read(EMOJI_PATH).ok());

fn emoji_face() -> Option<FontRef<'static>> {
    FontRef::from_index(EMOJI_DATA.as_deref()?, 0)
}

/// A face's font plus the variation coordinates to render it with.
#[derive(Clone, Copy)]
pub struct FaceRef<'a> {
    pub font: FontRef<'a>,
    pub coords: &'a [NormalizedCoord],
}

/// `face` of `family` (the bundled JetBrains Mono when `None`); symbols are always bundled.
fn face_of(family: Option<&Family>, face: Face) -> Option<FaceRef<'_>> {
    match face {
        Face::Symbols => Some(FaceRef {
            font: symbols(),
            coords: &[],
        }),
        Face::Mono(s) => Some(match family {
            Some(f) => {
                let t = &f.faces[s as usize & 3];
                FaceRef {
                    font: t.font(),
                    coords: &t.coords,
                }
            }
            None => FaceRef {
                font: bundled(s),
                coords: &[],
            },
        }),
    }
}

impl Font {
    /// The bundled font. `size` in points, `scale` = backing scale factor.
    #[cfg(test)]
    pub fn new(size: f32, scale: f32) -> Self {
        Self::with_family(size, scale, None, false)
    }

    /// `spec`'s first installed family (else the bundled font); also reports which one.
    pub fn with_spec(size: f32, scale: f32, spec: &FontSpec) -> (Self, FontResolution) {
        let r = system::resolve(&spec.families);
        let family = r.family.as_ref().map(|f| f.name.clone());
        let font = Self::with_family(size, scale, r.family, spec.thicken);
        let res = FontResolution {
            family: family.unwrap_or_else(|| BUNDLED_FAMILY.to_string()),
            matched: r.matched,
        };
        (font, res)
    }

    /// Cell metrics come from the regular face of `family`.
    fn with_family(size: f32, scale: f32, family: Option<Family>, thicken: bool) -> Self {
        let ppem = size * scale;
        let r = face_of(family.as_ref(), Face::Mono(0)).expect("regular face");
        let (f, coords) = (r.font, r.coords);
        let m = f.metrics(coords).scale(ppem);
        let advance = f
            .glyph_metrics(coords)
            .scale(ppem)
            .advance_width(f.charmap().map('0'));
        let height = (m.ascent + m.descent + m.leading).ceil();
        // Underline: 1px below baseline; thickness ~= 1/14 em, min 1.
        let ul_thick = ((ppem / 14.0).round()).max(1.0);
        let ul_off = (m.underline_offset).round().abs().max(1.0);
        // Strikethrough: ~40% up from baseline.
        let st_off = (m.strikeout_offset).round().abs().max(1.0);
        Self {
            ppem,
            family,
            thicken,
            cell: [advance.round().max(1.0), height.max(1.0)],
            baseline: m.ascent.round(),
            underline: (ul_thick, ul_off),
            strikethrough: (ul_thick, st_off),
            ctx: ScaleContext::new(),
            shaper: ShapeContext::new(),
            cache: HashMap::new(),
            sprites: HashMap::new(),
            emoji: HashMap::new(),
            gray: Atlas::new(ATLAS, 1),
            color: Atlas::new(COLOR_ATLAS, 4),
            generation: 0,
            image: Image::new(),
        }
    }

    /// Face data (font + variation coordinates) for `face`.
    pub fn face(&self, face: Face) -> Option<FaceRef<'_>> {
        face_of(self.family.as_ref(), face)
    }

    /// The shaper context together with `face`'s data: disjoint borrows of `self`.
    pub(crate) fn shaping(&mut self, face: Face) -> Option<(&mut ShapeContext, FaceRef<'_>)> {
        let f = face_of(self.family.as_ref(), face)?;
        Some((&mut self.shaper, f))
    }

    pub fn ppem(&self) -> f32 {
        self.ppem
    }

    /// Is the bundled JetBrains Mono the text font?
    pub fn is_bundled(&self) -> bool {
        self.family.is_none()
    }

    /// Which face renders `ch` (`None` = no face has it). Emoji-presentation text is
    /// chosen by the caller; this is the plain text path: the text font, then symbols.
    pub fn text_face(&self, ch: char, style: u8) -> Option<Face> {
        let style = style & (BOLD | ITALIC);
        if self.face(Face::Mono(style))?.font.charmap().map(ch) != 0 {
            Some(Face::Mono(style))
        } else if symbols().charmap().map(ch) != 0 {
            Some(Face::Symbols)
        } else {
            None
        }
    }

    /// Is there a color emoji font on this system?
    pub fn has_emoji(&self) -> bool {
        emoji_face().is_some()
    }

    /// Atlas slot for `ch` in `style` through the plain text path (`None` = blank or
    /// not in any bundled font).
    #[cfg(test)]
    pub fn glyph(&mut self, ch: char, style: u8) -> Option<Slot> {
        let face = self.text_face(ch, style)?;
        let id = self.face(face)?.font.charmap().map(ch);
        self.glyph_id(face, id)
    }

    /// Atlas slot for glyph `id` of `face`.
    pub fn glyph_id(&mut self, face: Face, id: GlyphId) -> Option<Slot> {
        if let Some(s) = self.cache.get(&(face, id)) {
            return *s;
        }
        if self.gray.full() {
            self.reset();
            return None;
        }
        let slot = self.raster(face, id);
        if self.gray.full() {
            // The shelf ran out while packing this glyph: start over (next frame redraws).
            self.reset();
            return None;
        }
        self.cache.insert((face, id), slot);
        slot
    }

    /// Wipe both atlases and every cached slot.
    fn reset(&mut self) {
        self.cache.clear();
        self.sprites.clear();
        self.emoji.clear();
        self.gray.clear();
        self.color.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    fn raster(&mut self, face: Face, id: GlyphId) -> Option<Slot> {
        let r = face_of(self.family.as_ref(), face)?;
        let mut scaler = self
            .ctx
            .builder(r.font)
            .size(self.ppem)
            .hint(true)
            .normalized_coords(r.coords)
            .build();
        self.image.clear();
        let mut render = Render::new(&[Source::Outline]);
        render.format(Format::Alpha);
        // Ghostty `font-thicken`: text faces only (symbols keep their drawn weight).
        if self.thicken && matches!(face, Face::Mono(_)) {
            render.embolden(thicken_strength(self.ppem));
        }
        if !render.render_into(&mut scaler, id, &mut self.image) {
            return None;
        }
        let p = self.image.placement;
        if p.width == 0 || p.height == 0 {
            return None;
        }
        let (x, y) = self.gray.put(&self.image.data, p.width, p.height)?;
        Some(Slot {
            x: x as u16,
            y: y as u16,
            w: p.width as u16,
            h: p.height as u16,
            left: p.left as i16,
            top: p.top as i16,
            color: false,
        })
    }

    /// Cell-sized procedural sprite for box drawing, blocks, braille, … (`None` = `ch`
    /// has no sprite). The slot's bearing puts it exactly on the cell.
    pub fn sprite(&mut self, ch: char) -> Option<Slot> {
        if !sprite::handles(ch) {
            return None;
        }
        if let Some(s) = self.sprites.get(&ch) {
            return *s;
        }
        if self.gray.full() {
            self.reset();
            return None;
        }
        let [cw, chp] = self.cell;
        let slot = sprite::draw(ch, cw as u32, chp as u32).and_then(|b| {
            let (x, y) = self.gray.put(&b.px, b.w, b.h)?;
            Some(Slot {
                x: x as u16,
                y: y as u16,
                w: b.w as u16,
                h: b.h as u16,
                left: 0,
                // `top` is measured up from the baseline: the sprite's top is the cell's top.
                top: self.baseline as i16,
                color: false,
            })
        });
        if self.gray.full() {
            self.reset();
            return None;
        }
        self.sprites.insert(ch, slot);
        slot
    }

    /// Color glyph for a grapheme cluster (emoji, ZWJ sequence, flag, keycap, skin
    /// tone) fitted into `span` cells. `None` = no emoji font or the cluster has no
    /// color glyph there.
    pub fn emoji(&mut self, cluster: &str, span: u8) -> Option<Slot> {
        let key = (Box::<str>::from(cluster), span);
        if let Some(s) = self.emoji.get(&key) {
            return *s;
        }
        if self.color.full() {
            self.reset();
            return None;
        }
        let slot = self.raster_emoji(cluster, span);
        if self.color.full() {
            self.reset();
            return None;
        }
        self.emoji.insert(key, slot);
        slot
    }

    fn raster_emoji(&mut self, cluster: &str, span: u8) -> Option<Slot> {
        let f = emoji_face()?;
        // Shape: Apple's font maps ZWJ/flag/skin-tone/keycap sequences to one glyph.
        let mut shaper = self.shaper.builder(f).size(self.ppem).build();
        shaper.add_str(cluster);
        let mut gid = None;
        let mut glyphs = 0;
        shaper.shape_with(|c| {
            for g in c.glyphs {
                glyphs += 1;
                gid.get_or_insert(g.id);
            }
        });
        let gid = gid.filter(|g| *g != 0)?;
        // A sequence the font cannot join shapes to several glyphs; draw the first
        // (the base emoji) rather than a row of overlapping ones.
        let _ = glyphs;
        let mut scaler = self.ctx.builder(f).size(self.ppem).build();
        self.image.clear();
        if !Render::new(&[
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
        ])
        .format(Format::Alpha)
        .render_into(&mut scaler, gid, &mut self.image)
            || self.image.content != Content::Color
        {
            return None;
        }
        let p = self.image.placement;
        if p.width == 0 || p.height == 0 {
            return None;
        }
        // Fit the (square) bitmap into `span` cells × 1 cell height, centered.
        let [cw, chp] = self.cell;
        let (bw, bh) = (cw * span as f32, chp);
        let k = (bw / p.width as f32).min(bh / p.height as f32);
        let (tw, th) = (
            ((p.width as f32 * k).round() as u32).max(1),
            ((p.height as f32 * k).round() as u32).max(1),
        );
        let data = if (tw, th) == (p.width, p.height) {
            std::mem::take(&mut self.image.data)
        } else {
            resample(&self.image.data, p.width, p.height, tw, th)
        };
        let (x, y) = self.color.put(&data, tw, th)?;
        Some(Slot {
            x: x as u16,
            y: y as u16,
            w: tw as u16,
            h: th as u16,
            left: ((bw - tw as f32) / 2.0).round() as i16,
            // Vertically centered in the cell (`top` is up from the baseline).
            top: (self.baseline - ((bh - th as f32) / 2.0).round()) as i16,
            color: true,
        })
    }
}

/// Faux-bold outline growth in pixels for `font-thicken`: ~1/48 em, so a 13pt@2x face
/// gains about half a pixel of stem, the same subtle weight macOS font smoothing adds.
fn thicken_strength(ppem: f32) -> f32 {
    (ppem / 48.0).max(0.25)
}

/// Area-average (box) resample of straight-alpha RGBA, premultiplying while
/// averaging so transparent edges do not bleed color.
fn resample(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        let (y0, y1) = (dy * sh / dh, ((dy + 1) * sh).div_ceil(dh).min(sh));
        for dx in 0..dw {
            let (x0, x1) = (dx * sw / dw, ((dx + 1) * sw).div_ceil(dw).min(sw));
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for y in y0..y1.max(y0 + 1) {
                for x in x0..x1.max(x0 + 1) {
                    let i = ((y.min(sh - 1) * sw + x.min(sw - 1)) * 4) as usize;
                    let al = src[i + 3] as u32;
                    r += src[i] as u32 * al;
                    g += src[i + 1] as u32 * al;
                    b += src[i + 2] as u32 * al;
                    a += al;
                    n += 1;
                }
            }
            let o = ((dy * dw + dx) * 4) as usize;
            if let (Some(r), Some(g), Some(b), Some(al)) = (
                r.checked_div(a),
                g.checked_div(a),
                b.checked_div(a),
                a.checked_div(n),
            ) {
                out[o..o + 4].copy_from_slice(&[r as u8, g as u8, b as u8, al as u8]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_and_cache() {
        let mut f = Font::new(13.0, 2.0);
        assert!(f.cell[0] > 10.0 && f.cell[1] > f.cell[0], "{:?}", f.cell);
        let a = f.glyph('A', 0).unwrap();
        let again = f.glyph('A', 0).unwrap();
        assert_eq!((a.x, a.y), (again.x, again.y));
        let b = f.glyph('A', BOLD).unwrap();
        assert_ne!((a.x, a.y), (b.x, b.y));
        assert!(f.gray.dirty.is_some());
        assert!(!a.color && !b.color);
        // Not in any bundled font, and blank glyphs have no slot.
        assert!(f.glyph('\u{10FFFD}', 0).is_none());
        assert!(f.glyph(' ', 0).is_none());
    }

    // Acceptance of #68: the cell width is the font's own advance at every DPR (measured once in physical px, not
    // scaled again), the same for every glyph of the face, so text never spreads into wide cells.
    #[test]
    fn cell_width_is_the_font_advance_at_dpr_1_and_2() {
        let (one, two) = (Font::new(13.0, 1.0), Font::new(13.0, 2.0));
        assert!(
            (two.cell[0] - 2.0 * one.cell[0]).abs() <= 1.0,
            "DPR 2 cell {:?} is not twice DPR 1 {:?}",
            two.cell,
            one.cell
        );
        for f in [&one, &two] {
            let face = face_of(None, Face::Mono(0)).expect("regular face");
            for ch in ['i', 'l', 'W', 'M', '0', '.'] {
                let adv = face
                    .font
                    .glyph_metrics(face.coords)
                    .scale(f.ppem)
                    .advance_width(face.font.charmap().map(ch));
                assert!(
                    (adv - f.cell[0]).abs() <= 1.0,
                    "{ch:?} advance {adv} vs cell {} at ppem {}",
                    f.cell[0],
                    f.ppem
                );
            }
        }
    }

    #[test]
    fn empty_or_unresolvable_family_falls_back_to_the_bundled_font() {
        for families in [vec![], vec!["No Such Family 9000".to_string()]] {
            let spec = FontSpec {
                families,
                thicken: false,
            };
            let (mut f, res) = Font::with_spec(13.0, 2.0, &spec);
            assert!(f.is_bundled());
            assert_eq!(res.family, BUNDLED_FAMILY);
            assert_eq!(res.matched, None);
            assert_eq!(f.cell, Font::new(13.0, 2.0).cell);
            assert!(f.glyph('A', 0).is_some());
        }
    }

    #[test]
    fn thickened_glyphs_rasterize_and_grow() {
        let plain = Font::new(13.0, 2.0).glyph('l', 0).unwrap();
        let mut t = Font::with_family(13.0, 2.0, None, true);
        let thick = t.glyph('l', 0).expect("thickened glyph");
        assert!(
            thick.w >= plain.w && thick.h >= plain.h,
            "{thick:?} vs {plain:?}"
        );
        // Symbols and every style keep working under thicken.
        assert!(t.glyph('\u{F07B}', 0).is_some());
        assert!(t.glyph('A', BOLD | ITALIC).is_some());
        assert!(thicken_strength(26.0) > 0.0 && thicken_strength(1.0) >= 0.25);
    }

    #[test]
    fn system_family_drives_metrics_and_glyphs() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        if !std::path::Path::new(&home)
            .join("Library/Fonts/fira-code-variable.ttf")
            .exists()
        {
            return;
        }
        let spec = FontSpec {
            families: vec!["Missing".into(), "fira code".into()],
            thicken: true,
        };
        let (mut f, res) = Font::with_spec(13.0, 2.0, &spec);
        assert_eq!((res.family.as_str(), res.matched), ("Fira Code", Some(1)));
        assert!(!f.is_bundled());
        let bundled = Font::new(13.0, 2.0).cell;
        assert!(
            f.cell[0] >= 1.0 && f.cell[1] >= 1.0,
            "{:?} vs {bundled:?}",
            f.cell
        );
        let (reg, bold) = (f.glyph('A', 0).unwrap(), f.glyph('A', BOLD).unwrap());
        assert_ne!((reg.x, reg.y), (bold.x, bold.y));
    }

    #[test]
    fn full_atlas_resets_instead_of_failing_forever() {
        let mut f = Font::new(13.0, 2.0);
        f.gray.shelf = (0, ATLAS - 2, 0);
        let gen_before = f.generation;
        assert!(f.glyph('W', 0).is_none());
        assert_eq!(f.generation, gen_before + 1);
        assert!(f.glyph('W', 0).is_some());
    }

    #[test]
    fn sprites_fill_the_cell_exactly() {
        let mut f = Font::new(13.0, 2.0);
        let [cw, ch] = f.cell;
        let s = f.sprite('█').unwrap();
        assert_eq!((s.w as f32, s.h as f32), (cw, ch));
        // Placed at the cell origin: x = left, y = baseline - top = 0.
        assert_eq!((s.left, f.baseline - s.top as f32), (0, 0.0));
        assert!(f.sprite('A').is_none());
        // Cached: same slot again.
        assert_eq!(f.sprite('█'), Some(s));
    }

    #[test]
    fn color_atlas_flag_and_emoji_cluster_fit() {
        let mut f = Font::new(13.0, 2.0);
        if !f.has_emoji() {
            return; // no system emoji font (CI on another OS)
        }
        let one = f.emoji("😀", 2).unwrap();
        assert!(one.color, "emoji must live in the RGBA atlas");
        assert!(f.color.dirty.is_some() && f.gray.dirty.is_none());
        let [cw, ch] = f.cell;
        // Wide emoji fit the 2-cell span without exceeding the cell height.
        assert!(one.w as f32 <= cw * 2.0 && one.h as f32 <= ch);
        // A ZWJ family is one glyph (one slot) and fits the same span.
        let fam = f.emoji("👨\u{200d}👩\u{200d}👧", 2).unwrap();
        assert!(fam.color && fam.w as f32 <= cw * 2.0);
        assert_ne!((fam.x, fam.y), (one.x, one.y));
        // Plain text stays in the coverage atlas.
        assert!(!f.glyph('A', 0).unwrap().color);
    }

    #[test]
    fn resample_downscales_and_keeps_color() {
        // 2×2 opaque red → 1×1 opaque red.
        let src = [255, 0, 0, 255].repeat(4);
        assert_eq!(resample(&src, 2, 2, 1, 1), vec![255, 0, 0, 255]);
        // Fully transparent texels do not tint the result.
        let mut src = [255, 0, 0, 255].repeat(2);
        src.extend([0, 255, 0, 0].repeat(2));
        assert_eq!(resample(&src, 2, 2, 1, 1), vec![255, 0, 0, 127]);
    }
}
