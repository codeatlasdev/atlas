//! Render snapshot (Ghostty's `RenderState`): the visible grid copied out under the lock
//! into a `Frame`, drawn without it.

use std::sync::Arc;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;

use super::Vt;
use crate::colors::{Palette, Rgba, rgba};
use crate::search::{self, UrlHit};

/// Glyph style bits in the atlas cache key.
pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphCell {
    pub col: u16,
    pub row: u16,
    pub ch: char,
    pub style: u8,
    pub fg: Rgba,
    /// Zerowidth chars stored in the cell (combining marks, ZWJ, VS16, skin tones),
    /// concatenated: the grapheme cluster is `ch` + `extra`. `None` for plain cells.
    pub extra: Option<Box<str>>,
    /// The cell is a double-width char (WIDE_CHAR): the glyph spans two columns.
    pub wide: bool,
}

/// Underline/strikethrough kind. Values must match the WGSL `deco_fs` switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DecoKind {
    Underline = 0,
    DoubleUnderline = 1,
    Undercurl = 2,
    DottedUnderline = 3,
    DashedUnderline = 4,
    Strikethrough = 5,
}

#[derive(Clone, Copy, Debug)]
pub struct DecoCell {
    pub col: u16,
    pub row: u16,
    pub kind: DecoKind,
    pub color: Rgba,
}

/// Cursor drawn as bars (block cursors are a bg/fg swap in the cells).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorBars {
    Beam,
    Underline,
    Hollow,
}

#[derive(Clone, Copy, Debug)]
pub struct CursorCell {
    pub col: u16,
    pub row: u16,
    pub bars: CursorBars,
    pub color: Rgba,
}

/// An OSC 8 hyperlink cell in the visible viewport.
#[derive(Clone, Debug)]
pub struct HyperlinkCell {
    pub col: u16,
    pub row: u16,
    pub uri: String,
}

/// `p` lies within the (row-major) cell range of a URL hit.
fn in_span(h: &UrlHit, p: Point) -> bool {
    (p.line, p.column) >= (h.start.line, h.start.column)
        && (p.line, p.column) <= (h.end.line, h.end.column)
}

/// Alpha-blend `sel` over `bg` (result opaque).
fn blend(bg: Rgba, sel: Rgba) -> Rgba {
    let a = sel[3] as u32;
    let mix = |b: u8, s: u8| ((b as u32 * (255 - a) + s as u32 * a) / 255) as u8;
    [
        mix(bg[0], sel[0]),
        mix(bg[1], sel[1]),
        mix(bg[2], sel[2]),
        255,
    ]
}

/// Render snapshot (Ghostty's `RenderState`): copied out under the lock, drawn without it.
#[derive(Default)]
pub struct Frame {
    pub cols: u32,
    /// The viewport's rows (the pty grid, the scrollbar). The cells below hold `rows + 2 * over` grid rows.
    pub rows: u32,
    /// Set by the host before `snapshot`: grid rows copied above and below the viewport (smooth scrolling
    /// draws the content between whole rows). Every `row` in this frame is a grid row: viewport row + `over`.
    pub over: u32,
    /// Set by the host: rows (-1..1) the grid is pushed down by (smooth scrolling); needs `over > 0`.
    pub scroll_frac: f32,
    /// Row-major cell backgrounds, `rows + 2 * over` rows.
    pub bg: Vec<Rgba>,
    pub glyphs: Vec<GlyphCell>,
    pub decos: Vec<DecoCell>,
    pub clear: Rgba,
    /// 0 = at the bottom.
    pub display_offset: u32,
    /// Screen + scrollback lines (scrollbar).
    pub total_lines: u32,
    /// Set by the host before `snapshot`: highlight color for selected cells.
    pub selection_color: Rgba,
    /// Set by the host: the view has keyboard focus (unfocused = hollow cursor).
    pub focused: bool,
    /// Set by the host: blink phase; `false` hides a blinking cursor.
    pub blink_on: bool,
    /// Set by the host: background of search matches / of the current match.
    pub search_color: Rgba,
    pub search_current_color: Rgba,
    pub hyperlinks: Vec<HyperlinkCell>,
    pub cursor: Option<CursorCell>,
    /// The app asked for a blinking cursor (the host drives `blink_on`).
    pub cursor_blinks: bool,
    /// Inline images overlapping the viewport, in draw order (z ascending).
    pub images: Vec<ImageDraw>,
}

/// One image placement in viewport cell coordinates (`row` may be negative:
/// partly scrolled off the top).
#[derive(Clone)]
pub struct ImageDraw {
    pub image: u32,
    pub generation: u64,
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<Vec<u8>>,
    pub col: f32,
    pub row: f32,
    /// Size in cells (fractional: natural-size images keep their pixel size).
    pub cols: f32,
    pub rows: f32,
    /// Source rectangle in image pixels.
    pub src: [u32; 4],
    pub z: i32,
}

impl Vt {
    /// Copy the visible screen into `f`. Frames are driven by new bytes (the caller's
    /// wakeup), not `Term::damage`: alacritty damages the cursor line on every call.
    pub fn snapshot(&self, pal: &Palette, f: &mut Frame) {
        let t = self.term.lock();
        let (cols, rows) = (t.columns(), t.screen_lines());
        let c = t.renderable_content();
        let clear = rgba(pal.bg(c.colors));
        f.cols = cols as u32;
        f.rows = rows as u32;
        f.clear = clear;
        let over = f.over as usize;
        f.bg.clear();
        f.bg.resize(cols * (rows + 2 * over), clear);
        f.glyphs.clear();
        f.decos.clear();
        f.hyperlinks.clear();
        f.display_offset = c.display_offset as u32;
        f.total_lines = t.grid().total_lines() as u32;
        let offset = c.display_offset as i32;
        // Viewport row 0 is absolute line `history - offset`.
        let top = t.grid().history_size() as i64 - c.display_offset as i64;
        let alt = t.mode().contains(TermMode::ALT_SCREEN);
        f.images.clear();
        {
            let sh = self.shell.lock();
            for pl in sh.images.placements.iter().filter(|p| p.alt == alt) {
                let row = pl.line - top;
                if row + pl.rows as i64 <= 0 || row >= rows as i64 {
                    continue;
                }
                let Some(img) = sh.images.images.get(&pl.image) else {
                    continue;
                };
                f.images.push(ImageDraw {
                    image: pl.image,
                    generation: img.generation,
                    w: img.w,
                    h: img.h,
                    rgba: img.rgba.clone(),
                    col: pl.col as f32,
                    row: row as f32,
                    cols: pl.size.0,
                    rows: pl.size.1,
                    src: pl.src,
                    z: pl.z,
                });
            }
        }
        f.images.sort_by_key(|i| i.z);
        let blinks = t.cursor_style().blinking;
        f.cursor_blinks = blinks && f.focused;
        let shape = match c.cursor.shape {
            CursorShape::Hidden => None,
            _ if f.cursor_blinks && !f.blink_on => None,
            _ if !f.focused => Some(CursorShape::HollowBlock),
            s => Some(s),
        };
        let cursor_row = c.cursor.point.line.0 + offset;
        f.cursor = shape
            .and_then(|s| match s {
                CursorShape::Beam => Some(CursorBars::Beam),
                CursorShape::Underline => Some(CursorBars::Underline),
                CursorShape::HollowBlock => Some(CursorBars::Hollow),
                _ => None,
            })
            .filter(|_| (0..rows as i32).contains(&cursor_row))
            .map(|bars| CursorCell {
                col: c.cursor.point.column.0 as u16,
                row: (cursor_row + over as i32) as u16,
                bars,
                color: rgba(pal.cursor(c.colors)),
            });
        // Only a block cursor recolors its cell.
        let cursor = (shape == Some(CursorShape::Block)).then_some(c.cursor.point);
        let (hits, hover) = {
            let mut sh = self.shell.lock();
            let hover = sh.hover.clone();
            let hits = match sh.search.as_mut() {
                Some(s) => {
                    if std::mem::take(&mut s.stale) {
                        s.matches = search::all_matches(&t, &mut s.re);
                        s.current = s.current.min(s.matches.len().saturating_sub(1));
                    }
                    // Only matches overlapping the viewport.
                    let (top, bottom) = (Line(-offset), Line(-offset + rows as i32 - 1));
                    let cur = s.current;
                    s.matches
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.end().line >= top && m.start().line <= bottom)
                        .map(|(i, m)| (m.clone(), i == cur))
                        .collect()
                }
                None => Vec::new(),
            };
            (hits, hover)
        };
        // The viewport, then (smooth scrolling) the line just above and just below it, where they exist.
        let outside = (over > 0)
            .then(|| {
                let (top, bottom) = (t.topmost_line(), t.bottommost_line());
                [Line(-offset - 1), Line(-offset + rows as i32)]
                    .into_iter()
                    .filter(move |l| *l >= top && *l <= bottom)
                    .flat_map(|l| {
                        let row = &t.grid()[l];
                        (0..cols).map(move |col| (l, col, row))
                    })
                    .map(|(l, col, row)| (Point::new(l, Column(col)), &row[Column(col)]))
            })
            .into_iter()
            .flatten();
        let inside = c.display_iter.map(|ic| (ic.point, ic.cell));
        for (point, cell) in inside.chain(outside) {
            let row = point.line.0 + offset;
            if row < -(over as i32) || row >= rows as i32 + over as i32 {
                continue;
            }
            let (row, col) = ((row + over as i32) as usize, point.column.0);
            let fl = cell.flags;
            if fl.contains(Flags::WIDE_CHAR_SPACER) && cursor != Some(point) {
                continue;
            }
            let mut fg = pal.resolve(cell.fg, c.colors);
            let mut bg = pal.resolve(cell.bg, c.colors);
            if fl.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut fg = rgba(fg);
            if fl.contains(Flags::DIM) {
                fg = [fg[0] / 3 * 2, fg[1] / 3 * 2, fg[2] / 3 * 2, 255];
            }
            let mut bg = rgba(bg);
            let mut keep_fg = false;
            if c.selection.is_some_and(|s| s.contains(point)) {
                match pal.selection_bg {
                    // Themed selection: opaque background (+ optional foreground).
                    Some(sel) => {
                        bg = rgba(sel);
                        if let Some(sfg) = pal.selection_fg {
                            fg = rgba(sfg);
                            keep_fg = true;
                        }
                    }
                    None => bg = blend(bg, f.selection_color),
                }
            }
            if let Some(&(_, cur)) = hits.iter().find(|(m, _)| m.contains(&point)) {
                bg = blend(
                    bg,
                    if cur {
                        f.search_current_color
                    } else {
                        f.search_color
                    },
                );
            }
            if cursor != Some(point) && !keep_fg {
                fg = pal.enforce_contrast(fg, bg);
            }
            if cursor == Some(point) {
                // Block cursor: cursor color behind, `cursor_text` (else the cell
                // background) as text.
                fg = pal.cursor_text.map_or(bg, rgba);
                bg = rgba(pal.cursor(c.colors));
            }
            let i = row * cols + col;
            f.bg[i] = bg;
            if fl.contains(Flags::WIDE_CHAR) && col + 1 < cols {
                f.bg[i + 1] = bg;
            }
            let extra = cell.zerowidth().filter(|z| !z.is_empty());
            if (cell.c != ' ' && cell.c != '\0' || extra.is_some()) && !fl.contains(Flags::HIDDEN) {
                let style = (fl.contains(Flags::BOLD) as u8 * BOLD)
                    | (fl.contains(Flags::ITALIC) as u8 * ITALIC);
                f.glyphs.push(GlyphCell {
                    col: col as u16,
                    row: row as u16,
                    ch: if cell.c == '\0' { ' ' } else { cell.c },
                    style,
                    fg,
                    extra: extra.map(|z| z.iter().collect::<String>().into_boxed_str()),
                    wide: fl.contains(Flags::WIDE_CHAR),
                });
            }
            if hover.as_ref().is_some_and(|h| in_span(h, point)) {
                f.decos.push(DecoCell {
                    col: col as u16,
                    row: row as u16,
                    kind: DecoKind::Underline,
                    color: fg,
                });
            }
            // Decorations: underline (and variants), strikethrough.
            if !fl.contains(Flags::HIDDEN) {
                let ul_color = cell
                    .underline_color()
                    .map(|uc| rgba(pal.resolve(uc, c.colors)))
                    .unwrap_or(fg);
                let push = |f: &mut Frame, kind| {
                    f.decos.push(DecoCell {
                        col: col as u16,
                        row: row as u16,
                        kind,
                        color: ul_color,
                    });
                };
                if fl.intersects(Flags::ALL_UNDERLINES) {
                    let kind = if fl.contains(Flags::UNDERCURL) {
                        DecoKind::Undercurl
                    } else if fl.contains(Flags::DOUBLE_UNDERLINE) {
                        DecoKind::DoubleUnderline
                    } else if fl.contains(Flags::DOTTED_UNDERLINE) {
                        DecoKind::DottedUnderline
                    } else if fl.contains(Flags::DASHED_UNDERLINE) {
                        DecoKind::DashedUnderline
                    } else {
                        DecoKind::Underline
                    };
                    push(f, kind);
                }
                if fl.contains(Flags::STRIKEOUT) {
                    push(f, DecoKind::Strikethrough);
                }
            }
            if let Some(h) = cell.hyperlink() {
                f.hyperlinks.push(HyperlinkCell {
                    col: col as u16,
                    row: row as u16,
                    uri: h.uri().to_string(),
                });
            }
        }
    }
}
