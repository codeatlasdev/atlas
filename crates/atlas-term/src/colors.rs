//! 269-entry palette (256 indexed + alacritty's named slots), sRGB bytes.
//! The 16 ANSI base colors can be overridden by the frontend (theme palette).

use alacritty_terminal::term::color::Colors;
pub use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

pub type Rgba = [u8; 4];

/// Base palette; OSC 4/10/11 overrides in `Colors` win over it.
#[derive(Clone)]
pub struct Palette {
    pub base: [Rgb; 269],
    /// Minimum WCAG contrast ratio between fg and bg (0 = disabled).
    pub min_contrast: f32,
}

/// Ghostty's default 16 ANSI colors (used when the theme provides none).
const ANSI_DEFAULT: [u32; 16] = [
    0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6, 0x666666,
    0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xeaeaea,
];

fn rgb(v: u32) -> Rgb {
    Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    }
}

fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (v as u16 * 2 / 3) as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

impl Palette {
    /// `ansi16`: optional 16-color theme override (hex strings); `None` slots use `ANSI_DEFAULT`.
    pub fn new(fg: Rgb, bg: Rgb, ansi16: Option<&[Option<Rgb>; 16]>) -> Self {
        let mut p = [Rgb::default(); 269];
        for (i, v) in ANSI_DEFAULT.iter().enumerate() {
            p[i] = ansi16.and_then(|a| a[i]).unwrap_or_else(|| rgb(*v));
        }
        let level = |i: usize| if i == 0 { 0 } else { (i * 40 + 55) as u8 };
        for i in 0..216 {
            p[16 + i] = Rgb {
                r: level(i / 36),
                g: level(i / 6 % 6),
                b: level(i % 6),
            };
        }
        for i in 0..24 {
            let v = (8 + i * 10) as u8;
            p[232 + i] = Rgb { r: v, g: v, b: v };
        }
        p[NamedColor::Foreground as usize] = fg;
        p[NamedColor::Background as usize] = bg;
        p[NamedColor::Cursor as usize] = fg;
        for i in 0..8 {
            p[NamedColor::DimBlack as usize + i] = dim(p[i]);
        }
        p[NamedColor::BrightForeground as usize] = fg;
        p[NamedColor::DimForeground as usize] = dim(fg);
        Self {
            base: p,
            min_contrast: 0.0,
        }
    }

    fn index(&self, i: usize, over: &Colors) -> Rgb {
        over[i].unwrap_or(self.base[i])
    }

    pub fn resolve(&self, c: Color, over: &Colors) -> Rgb {
        match c {
            Color::Spec(rgb) => rgb,
            Color::Indexed(i) => self.index(i as usize, over),
            Color::Named(n) => self.index(n as usize, over),
        }
    }

    pub fn bg(&self, over: &Colors) -> Rgb {
        self.index(NamedColor::Background as usize, over)
    }

    pub fn cursor(&self, over: &Colors) -> Rgb {
        self.index(NamedColor::Cursor as usize, over)
    }

    /// Return `fg` adjusted so contrast(fg, `bg`) ≥ `self.min_contrast`.
    /// No-op when `min_contrast` is 0 or already satisfied.
    pub fn enforce_contrast(&self, fg: Rgba, bg: Rgba) -> Rgba {
        if self.min_contrast <= 0.0 {
            return fg;
        }
        let [fr, fg_, fb, fa] = fg;
        let [br, bg_, bb, _] = bg;
        if contrast_ratio([fr, fg_, fb], [br, bg_, bb]) >= self.min_contrast {
            return fg;
        }
        // Shift fg toward white or black, whichever gives higher contrast.
        let bg_luma = luma([br, bg_, bb]);
        let target = if bg_luma > 0.18 {
            [0u8, 0, 0]
        } else {
            [255u8, 255, 255]
        };
        // Lerp in 16 steps until the ratio is satisfied or we hit the target.
        let mut c = [fr, fg_, fb];
        for step in 1u8..=16 {
            c = [
                lerp(c[0], target[0], step),
                lerp(c[1], target[1], step),
                lerp(c[2], target[2], step),
            ];
            if contrast_ratio(c, [br, bg_, bb]) >= self.min_contrast {
                break;
            }
        }
        [c[0], c[1], c[2], fa]
    }
}

/// sRGB component → linearized.
fn lin(v: u8) -> f32 {
    let f = v as f32 / 255.0;
    if f <= 0.04045 {
        f / 12.92
    } else {
        ((f + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance.
fn luma(c: [u8; 3]) -> f32 {
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio (1..21).
fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (la, lb) = (luma(a), luma(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn lerp(a: u8, b: u8, step: u8) -> u8 {
    let t = step as f32 / 16.0;
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

pub fn rgba(c: Rgb) -> Rgba {
    [c.r, c.g, c.b, 255]
}

/// `#rrggbb` -> Rgb (the frontend normalizes theme colors to hex).
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().strip_prefix('#')?;
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok().map(rgb))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_and_grays() {
        let p = Palette::new(Rgb::default(), Rgb::default(), None);
        assert_eq!(p.base[16], Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            p.base[231],
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            p.base[255],
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }

    #[test]
    fn min_contrast_lifts_similar_colors() {
        let p = Palette {
            base: [Rgb::default(); 269],
            min_contrast: 4.5,
        };
        // Near-identical dark colors: contrast ~1; should be lifted to white.
        let fg = [0x1a, 0x1a, 0x1a, 255u8];
        let bg = [0x18, 0x18, 0x18, 255u8];
        let out = p.enforce_contrast(fg, bg);
        assert!(
            contrast_ratio([out[0], out[1], out[2]], [bg[0], bg[1], bg[2]]) >= 4.5,
            "contrast {}",
            contrast_ratio([out[0], out[1], out[2]], [bg[0], bg[1], bg[2]])
        );
    }

    #[test]
    fn theme_ansi_overrides() {
        let mut ansi: [Option<Rgb>; 16] = [None; 16];
        ansi[1] = Some(Rgb { r: 255, g: 0, b: 0 });
        let p = Palette::new(Rgb::default(), Rgb::default(), Some(&ansi));
        assert_eq!(p.base[1], Rgb { r: 255, g: 0, b: 0 });
        // Unset slot stays at default.
        assert_ne!(p.base[2], Rgb { r: 255, g: 0, b: 0 });
    }
}
