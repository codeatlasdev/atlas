//! Fonts: Segoe UI when Windows has it (the system's own UI face, so the setup looks native), else
//! the JetBrains Mono the terminal bundles. Glyph outlines come from `skrifa` and are filled by the
//! same anti-aliased rasterizer as the rest of the window, so text is crisp at any DPI scale.

use std::borrow::Cow;
use std::path::Path;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FontRef;
use skrifa::{GlyphId, MetadataProvider};
use tiny_skia::{Path as SkPath, PathBuilder};

const BUNDLED_REGULAR: &[u8] =
    include_bytes!("../../../atlas-term/fonts/JetBrainsMono-Regular.ttf");
const BUNDLED_BOLD: &[u8] = include_bytes!("../../../atlas-term/fonts/JetBrainsMono-Bold.ttf");

/// One font file. Parsing is cheap (tables are read lazily), so a `FontRef` is made per call.
struct Face {
    data: Cow<'static, [u8]>,
}

impl Face {
    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::new(&self.data).ok()
    }

    fn glyph(font: &FontRef<'_>, ch: char) -> GlyphId {
        font.charmap().map(ch).unwrap_or_default()
    }

    /// Advance of `ch` in px at `size` px/em.
    fn advance(&self, ch: char, size: f32) -> f32 {
        let Some(font) = self.font() else { return 0.0 };
        font.glyph_metrics(Size::new(size), LocationRef::default())
            .advance_width(Self::glyph(&font, ch))
            .unwrap_or(0.0)
    }

    /// Outline of `ch` at `size` px/em with the origin at the baseline, y down (screen space).
    fn outline(&self, ch: char, size: f32) -> Option<SkPath> {
        let font = self.font()?;
        let glyph = font.outline_glyphs().get(Self::glyph(&font, ch))?;
        let mut pen = FlipPen(PathBuilder::new());
        glyph
            .draw(
                DrawSettings::unhinted(Size::new(size), LocationRef::default()),
                &mut pen,
            )
            .ok()?;
        pen.0.finish()
    }
}

/// Collects an outline into a tiny-skia path, flipping y (font space is y-up).
struct FlipPen(PathBuilder);

impl OutlinePen for FlipPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, -cy0, x, -y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// The two weights the window uses.
pub struct Fonts {
    regular: Face,
    bold: Face,
}

impl Fonts {
    /// The bundled fonts only (tests, and a Windows without Segoe UI).
    pub fn bundled() -> Self {
        Self {
            regular: Face {
                data: Cow::Borrowed(BUNDLED_REGULAR),
            },
            bold: Face {
                data: Cow::Borrowed(BUNDLED_BOLD),
            },
        }
    }

    /// Segoe UI (regular + semibold) read from `dir` (`%WINDIR%\Fonts`), else the bundled ones.
    pub fn from_dir(dir: &Path) -> Self {
        let load = |name: &str| -> Option<Face> {
            let face = Face {
                data: Cow::Owned(std::fs::read(dir.join(name)).ok()?),
            };
            face.font().is_some().then_some(face)
        };
        match (
            load("segoeui.ttf"),
            load("seguisb.ttf").or_else(|| load("segoeuib.ttf")),
        ) {
            (Some(regular), Some(bold)) => Self { regular, bold },
            _ => Self::bundled(),
        }
    }

    fn face(&self, bold: bool) -> &Face {
        if bold { &self.bold } else { &self.regular }
    }

    /// Width in px of `text` at `size` px/em.
    pub fn measure(&self, text: &str, size: f32, bold: bool) -> f32 {
        let face = self.face(bold);
        text.chars().map(|c| face.advance(c, size)).sum()
    }

    /// The glyphs of `text` as `(outline, x offset)` at `size` px/em, baseline at y = 0.
    pub fn glyphs(&self, text: &str, size: f32, bold: bool) -> Vec<(SkPath, f32)> {
        let face = self.face(bold);
        let mut x = 0.0;
        let mut out = Vec::new();
        for ch in text.chars() {
            if let Some(p) = face.outline(ch, size) {
                out.push((p, x));
            }
            x += face.advance(ch, size);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_the_text_and_the_size() {
        let f = Fonts::bundled();
        assert!(f.measure("Atlas is ready", 13.0, false) > f.measure("Atlas", 13.0, false));
        assert!(f.measure("Atlas", 26.0, false) > f.measure("Atlas", 13.0, false) * 1.9);
        assert_eq!(f.measure("", 13.0, true), 0.0);
    }

    #[test]
    fn glyph_outlines_sit_above_the_baseline_and_spaces_have_none() {
        let f = Fonts::bundled();
        let g = f.glyphs("A b", 20.0, false);
        assert_eq!(g.len(), 2, "the space draws nothing");
        for (p, _) in &g {
            let b = p.bounds();
            assert!(
                b.top() < 0.0 && b.bottom() <= 1.0,
                "y is down, baseline at 0: {b:?}"
            );
        }
        assert!(g[1].1 > g[0].1, "the next glyph starts to the right");
    }

    #[test]
    fn a_missing_system_font_directory_falls_back_to_the_bundled_faces() {
        let f = Fonts::from_dir(Path::new("Z:/definitely/not/here"));
        assert!(f.measure("Atlas", 13.0, true) > 0.0);
    }

    #[test]
    fn a_non_font_file_is_not_taken_for_segoe() {
        // Incident guard: a truncated or wrong file in the fonts folder must not blank the text.
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("segoeui.ttf"), b"not a font").unwrap();
        std::fs::write(t.path().join("seguisb.ttf"), b"not a font").unwrap();
        let f = Fonts::from_dir(t.path());
        assert!(f.measure("Atlas", 13.0, false) > 0.0);
    }
}
