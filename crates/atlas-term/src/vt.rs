//! VT state: `alacritty_terminal` parser + grid, behind a fair lock shared by the
//! reader (feed), the render thread (snapshot) and input (mode queries).
//! Isolated here so the engine can be swapped (libghostty-vt) without touching render.
//! OSC 133 (prompt marks) and OSC 7 (cwd) are not handled by alacritty: `OscScan`
//! picks them out of the stream and `feed` applies them at their stream position.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Processor};
use parking_lot::Mutex;

use crate::colors::{Palette, Rgba, rgba};
use crate::graphics::{Placed, Placement, Store};
use crate::search::{self, UrlHit};

/// What the terminal asks of its host.
pub enum VtEvent {
    /// Reply bytes for the pty (DA, DSR, OSC color queries…).
    Write(String),
    Title(String),
    Bell,
    /// OSC 52 store.
    ClipboardWrite(String),
    /// OSC 7 working directory (decoded path).
    Cwd(String),
    /// OSC 133;D: the command after the last prompt finished (`None` = no code given).
    CommandFinished(Option<i32>),
}

type OnEvent = Arc<dyn Fn(VtEvent) + Send + Sync>;

/// `(cols, rows, cell_w, cell_h)` in physical px, for `CSI 14 t` (image tools size by it).
type SizeCell = Arc<Mutex<WindowSize>>;

pub struct Proxy(OnEvent, SizeCell);

impl EventListener for Proxy {
    fn send_event(&self, e: Event) {
        match e {
            // Alacritty answers `?2027$p` "not supported"; clusters are always kept whole here.
            Event::PtyWrite(s) if s == "\x1b[?2027;0$y" => {
                (self.0)(VtEvent::Write("\x1b[?2027;3$y".into()))
            }
            Event::PtyWrite(s) => (self.0)(VtEvent::Write(s)),
            Event::Title(t) => (self.0)(VtEvent::Title(t)),
            Event::Bell => (self.0)(VtEvent::Bell),
            Event::ClipboardStore(_, text) => (self.0)(VtEvent::ClipboardWrite(text)),
            // Reading the system clipboard from the program is denied.
            Event::ClipboardLoad(_, cb) => {
                let _ = cb("");
            }
            Event::TextAreaSizeRequest(f) => (self.0)(VtEvent::Write(f(*self.1.lock()))),
            _ => {}
        }
    }
}

/// A string sequence alacritty ignores, picked out of the stream.
#[derive(Debug, PartialEq)]
enum Seq {
    /// OSC 133 / 7 payload.
    Osc(Vec<u8>),
    /// APC `G…` (Kitty graphics), without the `G`.
    Kitty(Vec<u8>),
    /// DCS sixel (`P1;P2;P3 q …`).
    Sixel(Vec<u8>),
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Kind {
    #[default]
    Osc,
    Apc,
    Dcs,
}

/// Streaming finder for OSC (BEL/ST), APC and DCS (ST) strings, split-safe.
/// Whole APC/DCS bodies are consumed here, so an `ESC ]` inside one is not an OSC.
#[derive(Default)]
struct SeqScan {
    /// 0 text, 1 after ESC, 2 in a string, 3 ESC inside a string.
    st: u8,
    kind: Kind,
    buf: Vec<u8>,
    /// The string is not one we want (or overflowed): skip to its end.
    skip: bool,
}

impl SeqScan {
    const MAX_OSC: usize = 4096;
    const MAX_BODY: usize = 64 << 20;

    fn start(&mut self, kind: Kind) -> u8 {
        self.kind = kind;
        self.buf.clear();
        self.skip = false;
        2
    }

    /// `(offset just past the terminator, sequence)` for each wanted string in `bytes`.
    fn scan(&mut self, bytes: &[u8]) -> Vec<(usize, Seq)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            // Plain text: jump to the next ESC.
            if self.st == 0 {
                match bytes[i..].iter().position(|&b| b == 0x1b) {
                    Some(k) => i += k,
                    None => break,
                }
            }
            let b = bytes[i];
            self.st = match (self.st, b) {
                (0 | 1, 0x1b) => 1,
                (1, b']') => self.start(Kind::Osc),
                (1, b'_') => self.start(Kind::Apc),
                (1, b'P') => self.start(Kind::Dcs),
                (2, 0x07) | (3, b'\\') if self.st == 3 || self.kind == Kind::Osc => {
                    if !self.skip
                        && let Some(seq) = self.finish()
                    {
                        out.push((i + 1, seq));
                    }
                    0
                }
                (2 | 3, 0x1b) => 3,
                (2, _) => {
                    let max = match self.kind {
                        Kind::Osc => Self::MAX_OSC,
                        _ => Self::MAX_BODY,
                    };
                    if self.skip {
                    } else if self.buf.len() < max {
                        self.buf.push(b);
                    } else {
                        self.skip = true;
                        self.buf = Vec::new();
                    }
                    2
                }
                // CAN/SUB or an unrelated escape abort the string.
                _ => 0,
            };
            i += 1;
        }
        out
    }

    fn finish(&mut self) -> Option<Seq> {
        let buf = std::mem::take(&mut self.buf);
        match self.kind {
            Kind::Osc => {
                (buf.starts_with(b"133;") || buf.starts_with(b"7;")).then_some(Seq::Osc(buf))
            }
            Kind::Apc => buf.strip_prefix(b"G").map(|b| Seq::Kitty(b.to_vec())),
            // Sixel: numeric params, then `q` (no intermediates: `$q` is DECRQSS).
            Kind::Dcs => {
                let q = buf
                    .iter()
                    .position(|&c| !(c.is_ascii_digit() || c == b';'))?;
                (buf[q] == b'q').then_some(Seq::Sixel(buf))
            }
        }
    }
}

/// A prompt start (OSC 133;A). `line` counts from the top of the scrollback.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mark {
    line: i64,
    exit: Option<i32>,
    /// The command typed at this prompt is running (OSC 133;C seen, no 133;D yet).
    running: bool,
}

/// An active search: the compiled query and its matches (recomputed after output).
struct Search {
    re: RegexSearch,
    matches: Vec<Match>,
    /// Index into `matches` of the current one.
    current: usize,
    /// Output arrived since `matches` was computed.
    stale: bool,
}

/// Result of a search step: 1-based `index` of the current match (0 = none) of `total`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStat {
    pub index: usize,
    pub total: usize,
}

struct Shell {
    scan: SeqScan,
    marks: VecDeque<Mark>,
    images: Store,
    /// Cell size in physical px (image size ↔ cells), set by the renderer.
    cell: [f32; 2],
    alt: bool,
    search: Option<Search>,
    /// Compiled pattern of `search` (a different key restarts the search).
    search_key: Option<String>,
    /// URL under the pointer while ⌘ is held (drawn underlined).
    hover: Option<UrlHit>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            scan: SeqScan::default(),
            marks: VecDeque::new(),
            images: Store::default(),
            cell: [8.0, 16.0],
            alt: false,
            search: None,
            search_key: None,
            hover: None,
        }
    }
}

/// `file://host/path` → `/path` (percent-decoded).
fn file_url_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let b = path.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| b.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match hex {
            Some(v) => {
                out.push(v);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

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
    pub rows: u32,
    /// Row-major cell backgrounds.
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

pub struct Vt {
    term: FairMutex<Term<Proxy>>,
    parser: Mutex<Processor>,
    shell: Mutex<Shell>,
    on_event: OnEvent,
    win: SizeCell,
}

impl Vt {
    pub fn new(cols: u16, rows: u16, on_event: impl Fn(VtEvent) + Send + Sync + 'static) -> Self {
        let size = Size {
            cols: cols.max(1) as usize,
            rows: rows.max(1) as usize,
        };
        let config = Config {
            scrolling_history: 10_000,
            kitty_keyboard: true,
            ..Config::default()
        };
        let on_event: OnEvent = Arc::new(on_event);
        let win: SizeCell = Arc::new(Mutex::new(WindowSize {
            num_lines: size.rows as u16,
            num_cols: size.cols as u16,
            cell_width: 8,
            cell_height: 16,
        }));
        Self {
            term: FairMutex::new(Term::new(
                config,
                &size,
                Proxy(on_event.clone(), win.clone()),
            )),
            parser: Mutex::new(Processor::new()),
            shell: Mutex::new(Shell::default()),
            on_event,
            win,
        }
    }

    /// Feed pty output; the viewport follows new output to the bottom.
    /// Inside a synchronized update (mode 2026) the parser buffers until the update
    /// ends or `sync_deadline` passes (then call `flush_sync`).
    pub fn feed(&self, bytes: &[u8]) {
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        let mut sh = self.shell.lock();
        let mut at = 0;
        for (end, seq) in sh.scan.scan(bytes) {
            p.advance(&mut *t, &bytes[at..end]);
            at = end;
            // Marks and images need the grid as of their stream position: end a pending
            // synchronized update first. Other OSCs (7 on every prompt) keep it atomic.
            let positional = match &seq {
                Seq::Osc(o) => o.starts_with(b"133;A"),
                Seq::Kitty(_) | Seq::Sixel(_) => true,
            };
            if positional && p.sync_timeout().sync_timeout().is_some() {
                p.stop_sync(&mut *t);
            }
            self.track_alt(&t, &mut sh);
            match seq {
                Seq::Osc(o) => self.apply_osc(&t, &mut sh, &o),
                Seq::Kitty(k) => {
                    let out = sh.images.kitty(&k);
                    if let Some(r) = out.reply {
                        (self.on_event)(VtEvent::Write(r));
                    }
                    if let Some((pl, how)) = out.place {
                        self.place_image(&mut p, &mut t, &mut sh, pl, how, false);
                    }
                }
                Seq::Sixel(s) => {
                    if let Some((pl, how)) = sh.images.sixel(&s) {
                        self.place_image(&mut p, &mut t, &mut sh, pl, how, true);
                    }
                }
            }
        }
        p.advance(&mut *t, &bytes[at..]);
        self.track_alt(&t, &mut sh);
        if let Some(s) = sh.search.as_mut() {
            s.stale = true;
        }
        t.scroll_display(Scroll::Bottom);
    }

    /// Images on the alternate screen go away when it is entered or left.
    fn track_alt(&self, t: &Term<Proxy>, sh: &mut Shell) {
        let alt = t.mode().contains(TermMode::ALT_SCREEN);
        if alt != sh.alt {
            sh.alt = alt;
            sh.images.clear_alt();
        }
    }

    /// Anchor a placement at the cursor, then move the cursor past it:
    /// Kitty leaves it after the last column on the last row, sixel below the image.
    fn place_image(
        &self,
        p: &mut Processor,
        t: &mut Term<Proxy>,
        sh: &mut Shell,
        mut pl: Placement,
        how: Placed,
        sixel: bool,
    ) {
        let [cw, ch] = sh.cell;
        let [_, _, w, h] = pl.src;
        let natural = (w as f32 / cw, h as f32 / ch);
        let (cols, rows) = match (how.cols, how.rows) {
            (0, 0) => natural,
            (c, 0) => (c as f32, c as f32 * natural.1 / natural.0.max(f32::EPSILON)),
            (0, r) => (r as f32 * natural.0 / natural.1.max(f32::EPSILON), r as f32),
            (c, r) => (c as f32, r as f32),
        };
        let cursor = t.grid().cursor.point;
        pl.line = t.grid().history_size() as i64 + cursor.line.0 as i64;
        pl.col = cursor.column.0 as u16;
        pl.cols = cols.ceil().max(1.0) as u16;
        pl.rows = rows.ceil().max(1.0) as u16;
        pl.alt = t.mode().contains(TermMode::ALT_SCREEN);
        pl.size = (cols, rows);
        let (pc, pr) = (pl.cols, pl.rows);
        // A placement id replaces its earlier placement of the same image.
        if pl.id != 0 {
            sh.images
                .placements
                .retain(|q| !(q.image == pl.image && q.id == pl.id));
        }
        sh.images.placements.push(pl);
        if how.keep_cursor {
            return;
        }
        let mut mv = String::new();
        let downs = if sixel { pr } else { pr - 1 };
        for _ in 0..downs {
            mv.push_str("\x1bD"); // IND: down, scrolling at the bottom margin.
        }
        if !sixel {
            mv.push_str(&format!("\x1b[{pc}C"));
        }
        p.advance(t, mv.as_bytes());
    }

    /// User settings: scrollback lines and the cursor used until the app sets one
    /// (`shape`: `block` | `bar` | `underline` | `hollow`).
    pub fn set_options(&self, scrollback: usize, shape: &str, blinking: bool) {
        let shape = match shape {
            "bar" => CursorShape::Beam,
            "underline" => CursorShape::Underline,
            "hollow" => CursorShape::HollowBlock,
            _ => CursorShape::Block,
        };
        self.term.lock().set_options(Config {
            scrolling_history: scrollback.min(100_000),
            kitty_keyboard: true,
            default_cursor_style: CursorStyle { shape, blinking },
            ..Config::default()
        });
    }

    /// Cell size in physical px, for image ↔ cell conversion.
    pub fn set_cell_px(&self, w: f32, h: f32) {
        if w > 0.0 && h > 0.0 {
            self.shell.lock().cell = [w, h];
            let mut s = self.win.lock();
            (s.cell_width, s.cell_height) = (w.round() as u16, h.round() as u16);
        }
    }

    fn apply_osc(&self, t: &Term<Proxy>, sh: &mut Shell, payload: &[u8]) {
        let s = String::from_utf8_lossy(payload);
        let mut parts = s.splitn(3, ';');
        match (parts.next(), parts.next()) {
            (Some("133"), Some("A")) => {
                let line = t.grid().history_size() as i64 + t.grid().cursor.point.line.0 as i64;
                if sh.marks.back().is_none_or(|m| m.line != line) {
                    if sh.marks.len() == 1000 {
                        sh.marks.pop_front();
                    }
                    sh.marks.push_back(Mark {
                        line,
                        exit: None,
                        running: false,
                    });
                } else if let Some(m) = sh.marks.back_mut() {
                    m.running = false;
                }
            }
            (Some("133"), Some("C")) => {
                if let Some(m) = sh.marks.back_mut() {
                    m.running = true;
                }
            }
            (Some("133"), Some("D")) => {
                let code = parts.next().and_then(|c| c.trim().parse().ok());
                if let Some(m) = sh.marks.back_mut() {
                    m.exit = code;
                    m.running = false;
                }
                (self.on_event)(VtEvent::CommandFinished(code));
            }
            (Some("7"), Some(_)) => {
                let url = &s[2..];
                if let Some(p) = file_url_path(url) {
                    (self.on_event)(VtEvent::Cwd(p));
                }
            }
            _ => {}
        }
    }

    /// When a pending synchronized update must be force-ended.
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.parser.lock().sync_timeout().sync_timeout()
    }

    /// End an expired synchronized update (draws what was buffered).
    pub fn flush_sync(&self) {
        let mut p = self.parser.lock();
        if p.sync_timeout().sync_timeout().is_some() {
            p.stop_sync(&mut *self.term.lock());
        }
    }

    /// Scroll so the previous (`up`) / next prompt is the top line. `false` = none.
    /// Marks follow the scrollback until it is full (10k lines); then they drift.
    pub fn jump_to_prompt(&self, up: bool) -> bool {
        let mut t = self.term.lock();
        let sh = self.shell.lock();
        let hist = t.grid().history_size() as i64;
        let top = hist - t.grid().display_offset() as i64;
        let target = if up {
            sh.marks.iter().rev().map(|m| m.line).find(|&l| l < top)
        } else {
            sh.marks.iter().map(|m| m.line).find(|&l| l > top)
        };
        let Some(line) = target else {
            return false;
        };
        let offset = (hist - line).clamp(0, hist);
        let delta = offset - t.grid().display_offset() as i64;
        t.scroll_display(Scroll::Delta(delta as i32));
        true
    }

    /// Exit code of the last finished command (OSC 133;D), if reported.
    pub fn last_exit(&self) -> Option<i32> {
        self.shell.lock().marks.back().and_then(|m| m.exit)
    }

    pub fn size(&self) -> (u16, u16) {
        let t = self.term.lock();
        (t.columns() as u16, t.screen_lines() as u16)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        self.term.lock().resize(Size {
            cols: cols as usize,
            rows: rows as usize,
        });
        let mut s = self.win.lock();
        (s.num_cols, s.num_lines) = (cols, rows);
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

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
        f.bg.clear();
        f.bg.resize(cols * rows, clear);
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
                row: cursor_row as u16,
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
        for cell in c.display_iter {
            let row = cell.point.line.0 + offset;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let (row, col) = (row as usize, cell.point.column.0);
            let fl = cell.flags;
            if fl.contains(Flags::WIDE_CHAR_SPACER) && cursor != Some(cell.point) {
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
            if c.selection.is_some_and(|s| s.contains(cell.point)) {
                bg = blend(bg, f.selection_color);
            }
            if let Some(&(_, cur)) = hits.iter().find(|(m, _)| m.contains(&cell.point)) {
                bg = blend(
                    bg,
                    if cur {
                        f.search_current_color
                    } else {
                        f.search_color
                    },
                );
            }
            if cursor != Some(cell.point) {
                fg = pal.enforce_contrast(fg, bg);
            }
            if cursor == Some(cell.point) {
                // Block cursor: cursor color behind, cell background as text.
                fg = bg;
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
            if hover.as_ref().is_some_and(|h| in_span(h, cell.point)) {
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

    /// Visible screen text (tests, debugging).
    pub fn text(&self) -> String {
        let t = self.term.lock();
        let g = t.grid();
        (0..t.screen_lines() as i32)
            .map(|l| {
                let row = &g[Line(l)];
                let s: String = (0..t.columns()).map(|c| row[Column(c)].c).collect();
                s.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    }

    pub fn cursor(&self) -> Point {
        self.term.lock().grid().cursor.point
    }

    /// Scroll the viewport by `delta` lines (positive = up into history).
    pub fn scroll(&self, delta: i32) {
        self.term.lock().scroll_display(Scroll::Delta(delta));
    }

    pub fn scroll_page(&self, up: bool) {
        let s = if up { Scroll::PageUp } else { Scroll::PageDown };
        self.term.lock().scroll_display(s);
    }

    /// Scroll to the oldest line of the scrollback.
    pub fn scroll_to_top(&self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    /// Select the whole scrollback and screen (⌘A).
    pub fn select_all(&self) {
        let mut t = self.term.lock();
        let start = Point::new(t.topmost_line(), Column(0));
        let end = Point::new(t.bottommost_line(), Column(t.columns().saturating_sub(1)));
        let mut sel = Selection::new(SelectionType::Lines, start, Side::Left);
        sel.update(end, Side::Right);
        t.selection = Some(sel);
    }

    /// Ghostty `clear_screen`: erase the screen and the scrollback and home the cursor.
    /// The caller then sends `^L` so the shell redraws its prompt at the top. Prompt marks
    /// refer to erased lines, so they go too. Not on the alternate screen: a full-screen
    /// program owns it and redraws itself.
    pub fn clear_screen(&self) -> bool {
        if self.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        self.shell.lock().marks.clear();
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        p.advance(&mut *t, b"\x1b[H\x1b[2J\x1b[3J");
        true
    }

    /// Cursor movement that puts the cursor on viewport cell (`col`, `row`) while a prompt
    /// is being edited (Ghostty `cursor-click-to-move`). Needs OSC 133: the shell marks where
    /// the prompt starts (`A`) and when its command ends (`D`), so a running command (or a
    /// full-screen program) never gets arrow keys. Only clicks from the prompt's first line to
    /// the cursor's line count, with the viewport at the bottom. Returns `(dx, dy)`: columns
    /// right (negative = left) and rows down, to be sent as arrow keys.
    pub fn click_to_move(&self, col: usize, row: i32) -> Option<(i32, i32)> {
        let t = self.term.lock();
        let sh = self.shell.lock();
        if t.mode().contains(TermMode::ALT_SCREEN) || t.grid().display_offset() != 0 {
            return None;
        }
        let mark = sh.marks.back().filter(|m| m.exit.is_none() && !m.running)?;
        let prompt_row = mark.line - t.grid().history_size() as i64;
        let cur = t.grid().cursor.point;
        if (row as i64) < prompt_row || row > cur.line.0 {
            return None;
        }
        let col = col.min(t.columns().saturating_sub(1)) as i32;
        Some((col - cur.column.0 as i32, row - cur.line.0))
    }

    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// `(display_offset, max_offset)`.
    pub fn scrollbar(&self) -> (u32, u32) {
        let t = self.term.lock();
        let max = t.grid().total_lines().saturating_sub(t.screen_lines());
        (t.grid().display_offset() as u32, max as u32)
    }

    /// Viewport row -> grid point (rows scroll with the display offset).
    fn point(t: &Term<Proxy>, col: usize, row: i32) -> Point {
        Point {
            line: Line(row - t.grid().display_offset() as i32),
            column: Column(col.min(t.columns().saturating_sub(1))),
        }
    }

    /// Start a selection at a viewport cell.
    pub fn start_selection(&self, col: usize, row: i32, ty: SelectionType, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        t.selection = Some(Selection::new(ty, p, side));
    }

    pub fn update_selection(&self, col: usize, row: i32, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        if let Some(sel) = t.selection.as_mut() {
            sel.update(p, side);
        }
    }

    /// Selected text, if any (the selection stays highlighted).
    pub fn end_selection(&self) -> Option<String> {
        self.term
            .lock()
            .selection_to_string()
            .filter(|s| !s.is_empty())
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// OSC 8 link under a viewport cell.
    pub fn hyperlink_at(&self, col: usize, row: i32) -> Option<String> {
        let t = self.term.lock();
        let p = Self::point(&t, col, row);
        if p.line < t.topmost_line() || p.line > t.bottommost_line() {
            return None;
        }
        t.grid()[p].hyperlink().map(|h| h.uri().to_string())
    }

    /// URL under a viewport cell: OSC 8 first, else a plain http(s)/file URL in the text.
    pub fn url_at(&self, col: usize, row: i32) -> Option<String> {
        let t = self.term.lock();
        let p = Self::point(&t, col, row);
        if p.line < t.topmost_line() || p.line > t.bottommost_line() {
            return None;
        }
        if let Some(h) = t.grid()[p].hyperlink() {
            return Some(h.uri().to_string());
        }
        search::url_at(&t, p).map(|h| h.url)
    }

    /// Underline the plain URL under a viewport cell (⌘ held); `None` clears it.
    /// Returns whether the hover changed (the caller redraws).
    pub fn set_hover(&self, at: Option<(usize, i32)>) -> bool {
        let t = self.term.lock();
        let hit = at.and_then(|(col, row)| {
            let p = Self::point(&t, col, row);
            (p.line >= t.topmost_line() && p.line <= t.bottommost_line())
                .then(|| search::url_at(&t, p))
                .flatten()
        });
        let mut sh = self.shell.lock();
        let changed = sh.hover != hit;
        sh.hover = hit;
        changed
    }

    /// Search the scrollback + screen and move to the next (`forward`) / previous match,
    /// wrapping. A new query/flags restarts from the top of the viewport. The viewport
    /// scrolls to the current match.
    pub fn search(
        &self,
        query: &str,
        regex: bool,
        case: bool,
        forward: bool,
    ) -> Result<SearchStat, String> {
        if query.is_empty() {
            self.clear_search();
            return Ok(SearchStat::default());
        }
        let key = search::pattern(query, regex, case);
        let mut t = self.term.lock();
        let mut sh = self.shell.lock();
        let fresh = sh.search_key.as_deref() != Some(key.as_str());
        if fresh {
            let re = search::compile(query, regex, case)?;
            sh.search = Some(Search {
                re,
                matches: Vec::new(),
                current: 0,
                stale: true,
            });
            sh.search_key = Some(key);
        }
        let top = Line(-(t.grid().display_offset() as i32));
        let Some(s) = sh.search.as_mut() else {
            return Ok(SearchStat::default());
        };
        s.stale = false;
        s.matches = search::all_matches(&t, &mut s.re);
        let n = s.matches.len();
        if n == 0 {
            return Ok(SearchStat::default());
        }
        s.current = if fresh {
            // First match at/after the top of the viewport (last one when going back).
            let first = s.matches.iter().position(|m| m.start().line >= top);
            if forward {
                first.unwrap_or(0)
            } else {
                first.map_or(n - 1, |i| (i + n - 1) % n)
            }
        } else if forward {
            (s.current + 1) % n
        } else {
            (s.current + n - 1) % n
        };
        let line = s.matches[s.current].start().line;
        let (idx, total) = (s.current + 1, n);
        // Keep the match visible; recenter only when it is off-screen.
        let off = t.grid().display_offset() as i32;
        let rows = t.screen_lines() as i32;
        if line.0 + off < 0 || line.0 + off >= rows {
            let want = (rows / 2 - line.0).clamp(0, t.history_size() as i32);
            t.scroll_display(Scroll::Delta(want - off));
        }
        Ok(SearchStat { index: idx, total })
    }

    pub fn clear_search(&self) {
        let mut sh = self.shell.lock();
        sh.search = None;
        sh.search_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;
    use std::sync::Arc;

    fn pal() -> Palette {
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

    #[test]
    fn parses_sgr_and_snapshots_colors() {
        let vt = Vt::new(10, 2, |_| {});
        vt.feed(b"\x1b[31mA\x1b[0;44mB\x1b[0m");
        let mut f = Frame {
            focused: true,
            blink_on: true,
            ..Frame::default()
        };
        vt.snapshot(&pal(), &mut f);
        assert_eq!(vt.text(), "AB");
        assert_eq!(f.glyphs[0].fg, rgba(pal().base[1]));
        assert_eq!(f.bg[1], rgba(pal().base[4]));
        // Cursor sits on col 2 with the cursor color.
        assert_eq!(f.bg[2], rgba(pal().base[256]));
    }

    #[test]
    fn answers_device_attributes_through_the_proxy() {
        let out = Arc::new(Mutex::new(String::new()));
        let o = out.clone();
        let vt = Vt::new(10, 2, move |e| {
            if let VtEvent::Write(s) = e {
                o.lock().push_str(&s);
            }
        });
        vt.feed(b"\x1b[c");
        assert!(out.lock().starts_with("\x1b[?"), "{:?}", out.lock());
    }

    #[test]
    fn resize_reflows_the_grid() {
        let vt = Vt::new(4, 2, |_| {});
        vt.feed(b"abcdef");
        assert_eq!(vt.text(), "abcd\nef");
        vt.resize(8, 2);
        assert_eq!(vt.size(), (8, 2));
        assert_eq!(vt.text(), "abcdef");
    }

    fn events(vt_cols: u16, rows: u16) -> (Vt, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        let vt = Vt::new(vt_cols, rows, move |e| {
            let s = match e {
                VtEvent::Cwd(p) => format!("cwd {p}"),
                VtEvent::CommandFinished(c) => format!("exit {c:?}"),
                _ => return,
            };
            l.lock().push(s);
        });
        (vt, log)
    }

    #[test]
    fn osc_133_and_7_split_across_chunks() {
        let (vt, log) = events(20, 3);
        vt.feed(b"\x1b]7;file://mac/Users/me/My%20Dir\x1b");
        vt.feed(b"\\\x1b]133;A\x07$ ls\r\n\x1b]13");
        vt.feed(b"3;D;2\x07");
        assert_eq!(*log.lock(), ["cwd /Users/me/My Dir", "exit Some(2)"]);
        assert_eq!(vt.last_exit(), Some(2));
        // The OSCs are invisible; the text is untouched.
        assert_eq!(vt.text(), "$ ls");
    }

    #[test]
    fn jumps_between_prompts_in_scrollback() {
        let (vt, _) = events(10, 2);
        for i in 0..3 {
            vt.feed(format!("\x1b]133;A\x07p{i}\r\nout\r\nout\r\n").as_bytes());
        }
        // 10 lines on a 2-row screen: p0/p1/p2 sit at scrollback offsets 8/5/2
        // (`text()` reads the live screen, so the viewport is checked by offset).
        for want in [2, 5, 8] {
            assert!(vt.jump_to_prompt(true));
            assert_eq!(vt.scrollbar().0, want);
        }
        assert!(!vt.jump_to_prompt(true));
        assert!(vt.jump_to_prompt(false));
        assert_eq!(vt.scrollbar().0, 5);
    }

    #[test]
    fn mark_inside_synchronized_update_lands_on_its_line() {
        let (vt, _) = events(10, 2);
        // a, b, p, x, y: `p` is line 2 of 5, i.e. offset 1 from the bottom.
        vt.feed(b"\x1b[?2026ha\r\nb\r\n\x1b]133;A\x07p\r\nx\r\ny\x1b[?2026l");
        assert!(vt.jump_to_prompt(true));
        assert_eq!(vt.scrollbar().0, 1);
        let (vt, log) = events(10, 2);
        vt.feed(b"\x1b]7;file://h/a;b\x07");
        assert_eq!(*log.lock(), ["cwd /a;b"]);
        // OSC 7 inside an update does not release the buffered frame.
        vt.feed(b"\x1b[?2026hframe\x1b]7;file://h/c\x07");
        assert_eq!(vt.text(), "");
        vt.feed(b"\x1b[?2026l");
        assert_eq!(vt.text(), "frame");
    }

    #[test]
    fn cursor_shape_focus_and_blink() {
        let vt = Vt::new(10, 2, |_| {});
        let mut f = Frame {
            focused: true,
            blink_on: true,
            ..Frame::default()
        };
        // DECSCUSR 6: steady beam → bars, cell colors untouched.
        vt.feed(b"\x1b[6 q");
        vt.snapshot(&pal(), &mut f);
        assert_eq!(f.cursor.map(|c| c.bars), Some(CursorBars::Beam));
        assert!(!f.cursor_blinks);
        // Unfocused: hollow block regardless of shape.
        f.focused = false;
        vt.snapshot(&pal(), &mut f);
        assert_eq!(f.cursor.map(|c| c.bars), Some(CursorBars::Hollow));
        // DECSCUSR 1 blinking block: off phase hides it.
        vt.feed(b"\x1b[1 q");
        f.focused = true;
        f.blink_on = false;
        vt.snapshot(&pal(), &mut f);
        assert!(f.cursor_blinks);
        assert!(f.cursor.is_none());
        assert_ne!(f.bg[0], rgba(pal().base[256]));
    }

    #[test]
    fn synchronized_output_holds_until_end_or_flush() {
        let vt = Vt::new(10, 2, |_| {});
        vt.feed(b"\x1b[?2026hhidden");
        assert_eq!(vt.text(), "");
        assert!(vt.sync_deadline().is_some());
        vt.flush_sync();
        assert_eq!(vt.text(), "hidden");
        vt.feed(b"\x1b[?2026h!\x1b[?2026l");
        assert_eq!(vt.text(), "hidden!");
    }

    fn snap(vt: &Vt) -> Frame {
        let mut f = Frame::default();
        vt.snapshot(&pal(), &mut f);
        f
    }

    #[test]
    fn kitty_image_anchors_at_cursor_and_moves_it_past() {
        let vt = Vt::new(20, 5, |_| {});
        vt.set_cell_px(10.0, 20.0);
        let px = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            vec![0u8; 20 * 40 * 3],
        );
        // 20×40 px = 2×2 cells at 10×20 px per cell.
        vt.feed(format!("ab\x1b_Ga=T,f=24,s=20,v=40,i=1,q=2;{px}\x1b\\x").as_bytes());
        let f = snap(&vt);
        assert_eq!(f.images.len(), 1);
        let im = &f.images[0];
        assert_eq!((im.col, im.row, im.cols, im.rows), (2.0, 0.0, 2.0, 2.0));
        // Cursor went to the last image row, after its last column: `x` at (4, 1).
        assert_eq!(vt.text(), "ab\n    x");
    }

    #[test]
    fn sixel_lands_below_and_alt_screen_images_go_away() {
        let vt = Vt::new(20, 5, |_| {});
        vt.set_cell_px(10.0, 20.0);
        // 2×6 px red sixel: one cell row; the cursor ends on the next line.
        vt.feed(b"\x1bPq#0;2;100;0;0~~\x1b\\y");
        assert_eq!(snap(&vt).images.len(), 1);
        assert_eq!(vt.text(), "\ny");
        vt.feed(b"\x1b[?1049h\x1bPq~\x1b\\");
        assert_eq!(snap(&vt).images.len(), 1, "only the alt-screen image shows");
        vt.feed(b"\x1b[?1049l");
        let f = snap(&vt);
        assert_eq!(f.images.len(), 1, "primary image back, alt image dropped");
        assert_eq!(f.images[0].row, 0.0);
    }

    #[test]
    fn osc_inside_dcs_is_not_a_prompt_mark() {
        let (vt, _) = events(10, 2);
        vt.feed(b"\x1bPq\x1b]133;A\x07\x1b\\");
        vt.feed(b"\r\n\r\n\r\n");
        assert!(!vt.jump_to_prompt(true));
    }

    #[test]
    fn select_all_covers_scrollback_and_screen() {
        let vt = Vt::new(4, 2, |_| {});
        vt.feed(b"aa\r\nbb\r\ncc\r\ndd");
        vt.select_all();
        assert_eq!(vt.end_selection().unwrap().trim_end(), "aa\nbb\ncc\ndd");
    }

    #[test]
    fn scroll_to_top_reaches_the_oldest_line() {
        let vt = Vt::new(4, 2, |_| {});
        vt.feed(b"a\r\nb\r\nc\r\nd\r\ne");
        assert_eq!(vt.scrollbar().0, 0);
        vt.scroll_to_top();
        let (off, max) = vt.scrollbar();
        assert_eq!((off, max), (3, 3));
        vt.scroll_to_bottom();
        assert_eq!(vt.scrollbar().0, 0);
    }

    #[test]
    fn clear_screen_erases_scrollback_and_marks_but_not_the_alt_screen() {
        let vt = Vt::new(6, 2, |_| {});
        vt.feed(b"\x1b]133;A\x07one\r\ntwo\r\nthree");
        assert!(vt.scrollbar().1 > 0);
        assert!(vt.clear_screen());
        assert_eq!(vt.scrollbar().1, 0);
        assert_eq!(vt.text().trim(), "");
        assert!(!vt.jump_to_prompt(true), "marks point at erased lines");
        // A full-screen program owns the alternate screen: left alone.
        vt.feed(b"\x1b[?1049hvim");
        assert!(!vt.clear_screen());
        assert_eq!(vt.text().trim(), "vim");
    }

    #[test]
    fn click_to_move_only_while_a_prompt_is_being_edited() {
        let vt = Vt::new(20, 4, |_| {});
        // No OSC 133 at all: never move.
        vt.feed(b"$ echo hi");
        assert_eq!(vt.click_to_move(2, 0), None);
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(b"\x1b]133;A\x07$ echo hi");
        // The cursor is at column 9 of row 0: a click at column 4 is 5 cells left.
        assert_eq!(vt.click_to_move(4, 0), Some((-5, 0)));
        assert_eq!(vt.click_to_move(12, 0), Some((3, 0)));
        // Below the cursor's line is not the prompt.
        assert_eq!(vt.click_to_move(4, 2), None);
        // The command runs: arrows would reach the program, not the line editor.
        vt.feed(b"\x1b]133;C\x07\r\nout");
        assert_eq!(vt.click_to_move(4, 1), None);
        // Finished, and the next prompt is drawn at row 2.
        vt.feed(b"\x1b]133;D;0\x07\r\n\x1b]133;A\x07$ ");
        assert_eq!(vt.click_to_move(0, 2), Some((-2, 0)));
        // Rows above the prompt's first line are not editable.
        assert_eq!(vt.click_to_move(0, 0), None);
        // Full-screen programs never get click-to-move.
        vt.feed(b"\x1b[?1049h");
        assert_eq!(vt.click_to_move(0, 0), None);
    }

    #[test]
    fn reports_text_area_pixels() {
        let out = Arc::new(Mutex::new(String::new()));
        let o = out.clone();
        let vt = Vt::new(20, 5, move |e| {
            if let VtEvent::Write(s) = e {
                o.lock().push_str(&s);
            }
        });
        vt.set_cell_px(10.0, 20.0);
        vt.feed(b"\x1b[14t");
        assert_eq!(*out.lock(), "\x1b[4;100;200t");
    }

    fn lines(n: usize) -> String {
        (0..n).map(|i| format!("line{i}\r\n")).collect()
    }

    #[test]
    fn search_finds_matches_across_scrollback_and_wraps() {
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(lines(30).as_bytes());
        // `line1`, `line10`..`line19` all contain "line1": 11 matches, 8 in history.
        let s = vt.search("line1", false, false, true).unwrap();
        assert_eq!(s.total, 11);
        assert_eq!(s.index, 1);
        // The first match is far above the live screen: the viewport followed it.
        assert!(vt.scrollbar().0 > 0);
        let s = vt.search("line1", false, false, true).unwrap();
        assert_eq!(s.index, 2);
        // Backwards from the first wraps to the last.
        vt.search("line1", false, false, false).unwrap();
        let s = vt.search("line1", false, false, false).unwrap();
        assert_eq!((s.index, s.total), (11, 11));
        let s = vt.search("line1", false, false, true).unwrap();
        assert_eq!(s.index, 1, "forward from the last wraps to the first");
    }

    #[test]
    fn search_regex_case_and_literal_modes() {
        let vt = Vt::new(30, 4, |_| {});
        vt.feed(b"Foo foo FOO a.c abc\r\n");
        assert_eq!(vt.search("foo", false, false, true).unwrap().total, 3);
        assert_eq!(vt.search("foo", false, true, true).unwrap().total, 1);
        // Literal mode escapes the dot; regex mode does not.
        assert_eq!(vt.search("a.c", false, false, true).unwrap().total, 1);
        assert_eq!(vt.search("a.c", true, false, true).unwrap().total, 2);
        assert_eq!(vt.search(r"fo+ ", true, false, true).unwrap().total, 3);
        assert_eq!(vt.search(r"fo+ ", true, true, true).unwrap().total, 1);
        assert!(vt.search("(", true, false, true).is_err());
        assert_eq!(vt.search("nope", false, false, true).unwrap().total, 0);
        vt.clear_search();
        assert_eq!(
            vt.search("", false, false, true).unwrap(),
            SearchStat::default()
        );
    }

    #[test]
    fn search_highlights_visible_matches_and_follows_output() {
        let vt = Vt::new(20, 3, |_| {});
        vt.feed(b"ab ab\r\n");
        vt.search("ab", false, false, true).unwrap();
        let mut f = Frame {
            search_color: [255, 0, 0, 255],
            search_current_color: [0, 255, 0, 255],
            ..Frame::default()
        };
        vt.snapshot(&pal(), &mut f);
        assert_eq!(f.bg[0], [0, 255, 0, 255], "current match");
        assert_eq!(f.bg[3], [255, 0, 0, 255], "other match");
        assert_ne!(f.bg[2], [255, 0, 0, 255]);
        // New output re-runs the search on the next frame.
        vt.feed(b"ab\r\n");
        vt.snapshot(&pal(), &mut f);
        assert_eq!(f.bg[20 + 1], f.bg[20], "second line matched too");
        assert_ne!(f.bg[20], f.clear);
    }

    #[test]
    fn plain_urls_are_found_and_underlined_on_hover() {
        let vt = Vt::new(40, 3, |_| {});
        vt.feed(b"see (https://a.dev/x?q=1). ok http://b.io, end\r\n");
        assert_eq!(vt.url_at(8, 0).as_deref(), Some("https://a.dev/x?q=1"));
        assert_eq!(vt.url_at(0, 0), None);
        assert_eq!(vt.url_at(34, 0).as_deref(), Some("http://b.io"));
        assert!(vt.set_hover(Some((8, 0))));
        assert!(!vt.set_hover(Some((9, 0))), "same URL: no change");
        let f = snap(&vt);
        let ul: Vec<u16> = f.decos.iter().map(|d| d.col).collect();
        assert_eq!(ul.len(), "https://a.dev/x?q=1".len());
        assert_eq!(ul[0], 5);
        assert!(vt.set_hover(None));
        assert!(snap(&vt).decos.is_empty());
    }

    #[test]
    fn urls_follow_soft_wrapped_lines() {
        let vt = Vt::new(10, 4, |_| {});
        vt.feed(b"https://example.com/path");
        // Wrapped over three rows; every row resolves to the whole URL.
        for row in 0..3 {
            assert_eq!(
                vt.url_at(2, row).as_deref(),
                Some("https://example.com/path"),
                "row {row}"
            );
        }
    }

    #[test]
    fn osc8_link_wins_over_text() {
        let vt = Vt::new(40, 2, |_| {});
        vt.feed(b"\x1b]8;;https://real.dev\x1b\\click\x1b]8;;\x1b\\");
        assert_eq!(vt.url_at(1, 0).as_deref(), Some("https://real.dev"));
    }

    #[test]
    fn clusters_expose_base_plus_zerowidth_and_wide() {
        let vt = Vt::new(10, 2, |_| {});
        // e + combining acute; thumbs-up + skin tone (zerowidth modifier in alacritty is
        // its own wide cell, so check the ZWJ/VS16 forms): heart + VS16, a + ZWJ.
        vt.feed("e\u{301}x\u{2764}\u{fe0f}\u{1f600}".as_bytes());
        let f = snap(&vt);
        let g = |col| f.glyphs.iter().find(|g| g.col == col).unwrap();
        assert_eq!((g(0).ch, g(0).extra.as_deref()), ('e', Some("\u{301}")));
        assert!(!g(0).wide);
        assert_eq!(g(1).extra, None);
        assert_eq!(g(2).extra.as_deref(), Some("\u{fe0f}"));
        let emoji = f.glyphs.iter().find(|g| g.ch == '\u{1f600}').unwrap();
        assert!(emoji.wide);
    }

    #[test]
    fn answers_grapheme_cluster_mode_query() {
        let out = Arc::new(Mutex::new(String::new()));
        let o = out.clone();
        let vt = Vt::new(10, 2, move |e| {
            if let VtEvent::Write(s) = e {
                o.lock().push_str(&s);
            }
        });
        vt.feed(b"\x1b[?2027$p");
        assert_eq!(*out.lock(), "\x1b[?2027;3$y");
    }
}
