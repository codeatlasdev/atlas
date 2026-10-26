//! VT state: `alacritty_terminal` parser + grid, behind a fair lock shared by the
//! reader (feed), the render thread (snapshot) and input (mode queries).
//! Isolated here so the engine can be swapped (libghostty-vt) without touching render.
//! OSC 133 (prompt marks), OSC 7 (cwd), Kitty graphics and sixel are not handled by
//! alacritty: `scan::SeqScan` picks them out of the stream and `feed` applies them at
//! their stream position.
//!
//! `Vt` is one type spread over cohesive files: this one (lifecycle, feed, scrolling,
//! selection), `shell` (prompt marks, cwd), `images`, `find` (links, URLs, search) and
//! `frame` (the render snapshot).

mod find;
mod frame;
mod history;
mod images;
mod pick;
mod pixel;
mod scan;
mod shell;
mod sync;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Handler};
use parking_lot::Mutex;

use crate::colors::Palette;
use scan::Seq;
use shell::Shell;
use sync::SyncHold;

/// The VT parser with Atlas's synchronized-update hold (`sync`).
pub(crate) type Processor = alacritty_terminal::vte::ansi::Processor<SyncHold>;

pub use find::SearchStat;
pub use frame::{
    BOLD, CursorBars, CursorCell, DecoCell, DecoKind, Frame, GlyphCell, HyperlinkCell, ITALIC,
    ImageDraw,
};
pub use pick::{HoverTarget, Link, LinkKind, SelectionState};

/// The viewport and the modes the wheel rules read (`Vt::wheel_state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelState {
    pub display_offset: u32,
    pub max_offset: u32,
    /// Any mouse-reporting mode (1000/1002/1003) is on.
    pub mouse: bool,
    pub alt_screen: bool,
    /// DECSET 1007.
    pub alt_scroll: bool,
}

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
    /// OSC 9 / OSC 777;notify: a desktop notification the program asks for.
    Notify {
        title: Option<String>,
        body: String,
    },
}

type OnEvent = Arc<dyn Fn(VtEvent) + Send + Sync>;

/// `(cols, rows, cell_w, cell_h)` in physical px, for `CSI 14 t` (image tools size by it).
type SizeCell = Arc<Mutex<WindowSize>>;

/// The theme palette, for answering OSC 10/11/4 queries (`Vt::set_palette`).
type PalCell = Arc<Mutex<Option<Palette>>>;

pub struct Proxy(OnEvent, SizeCell, PalCell);

impl EventListener for Proxy {
    fn send_event(&self, e: Event) {
        match e {
            // Alacritty answers `?2027$p` "not supported"; clusters are always kept whole here.
            Event::PtyWrite(s) if s == "\x1b[?2027;0$y" => {
                (self.0)(VtEvent::Write("\x1b[?2027;3$y".into()))
            }
            // Alacritty answers `?1016$p` "not recognized": `track_pixel` answers it from its own state.
            Event::PtyWrite(s) if s == "\x1b[?1016;0$y" => {}
            Event::PtyWrite(s) => (self.0)(VtEvent::Write(s)),
            Event::Title(t) => (self.0)(VtEvent::Title(t)),
            Event::Bell => (self.0)(VtEvent::Bell),
            Event::ClipboardStore(_, text) => (self.0)(VtEvent::ClipboardWrite(text)),
            // Reading the system clipboard from the program is denied.
            Event::ClipboardLoad(_, cb) => {
                let _ = cb("");
            }
            Event::TextAreaSizeRequest(f) => (self.0)(VtEvent::Write(f(*self.1.lock()))),
            // Answered from the theme palette only: a color the program set with OSC 4/10/11
            // lives in alacritty's `Colors`, which the proxy cannot reach.
            Event::ColorRequest(i, f) => {
                let rgb = self.2.lock().as_ref().and_then(|p| p.base.get(i).copied());
                if let Some(rgb) = rgb {
                    (self.0)(VtEvent::Write(f(rgb)));
                }
            }
            _ => {}
        }
    }
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

pub struct Vt {
    term: FairMutex<Term<Proxy>>,
    parser: Mutex<Processor>,
    shell: Mutex<Shell>,
    /// The user's scrollback line cap, before the byte budget tightens it (`history`).
    scrollback: AtomicUsize,
    /// The byte budget that tightens it (`history::BYTE_BUDGET` until the user sets one).
    budget: AtomicUsize,
    on_event: OnEvent,
    win: SizeCell,
    pal: PalCell,
}

/// Default scrollback line cap.
const SCROLLBACK: usize = 10_000;

impl Vt {
    /// `on_event` is called with the VT locks held: it must never call back into this `Vt`.
    pub fn new(cols: u16, rows: u16, on_event: impl Fn(VtEvent) + Send + Sync + 'static) -> Self {
        let size = Size {
            cols: cols.max(1) as usize,
            rows: rows.max(1) as usize,
        };
        let config = Config {
            scrolling_history: history::history_lines(SCROLLBACK, size.cols, history::BYTE_BUDGET),
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
        let pal: PalCell = Arc::default();
        let scrollback = AtomicUsize::new(SCROLLBACK);
        Self {
            term: FairMutex::new(Term::new(
                config,
                &size,
                Proxy(on_event.clone(), win.clone(), pal.clone()),
            )),
            parser: Mutex::new(Processor::new()),
            shell: Mutex::new(Shell::default()),
            scrollback,
            budget: AtomicUsize::new(history::BYTE_BUDGET),
            on_event,
            win,
            pal,
        }
    }

    /// Feed pty output. The viewport stays where the user left it (alacritty shifts the
    /// display offset as lines enter history, #97); the desktop snaps to the bottom on
    /// keystroke/paste (`Shared::input`).
    /// Inside a synchronized update (mode 2026) the parser buffers until the update
    /// ends or `sync_deadline` passes (then call `flush_sync`).
    pub fn feed(&self, bytes: &[u8]) {
        // `clear` (ED3 + home + ED2) must end with an empty history: cut the chunk after each one.
        let cuts = self.shell.lock().clear.scan(bytes);
        let mut from = 0;
        for end in cuts {
            self.feed_part(&bytes[from..end]);
            self.purge_history();
            from = end;
        }
        self.feed_part(&bytes[from..]);
    }

    /// ED 3 again, after the whole `clear` ran: its ED 2 pushed the old screen into history.
    fn purge_history(&self) {
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        p.advance(&mut *t, b"\x1b[3J");
        Self::reclaim(&mut t);
    }

    fn feed_part(&self, bytes: &[u8]) {
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        let mut sh = self.shell.lock();
        let mut at = 0;
        let had_history = t.grid().history_size() > 0;
        self.track_pixel(&mut sh, bytes);
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
        // ED3 / RIS emptied the history: free the rows alacritty keeps as spare.
        if had_history && t.grid().history_size() == 0 {
            Self::reclaim(&mut t);
        }
        if let Some(s) = sh.search.as_mut() {
            s.stale = true;
        }
    }

    /// Follow DECSET 1016 (alacritty ignores it) and answer its DECRQM.
    fn track_pixel(&self, sh: &mut Shell, bytes: &[u8]) {
        for c in sh.pixel_scan.scan(bytes) {
            match c {
                pixel::Change::Set(on) => sh.pixel = on,
                pixel::Change::Reset => sh.pixel = false,
                // xterm: 1 set, 2 reset.
                pixel::Change::Query => (self.on_event)(VtEvent::Write(format!(
                    "\x1b[?1016;{}$y",
                    if sh.pixel { 1 } else { 2 }
                ))),
            }
        }
    }

    /// The program turned on SGR-Pixels (DECSET 1016): pointer reports carry pixel coordinates
    /// (`mouse::report`), taking precedence over 1006 like xterm and Ghostty.
    pub fn pixel_mouse(&self) -> bool {
        self.shell.lock().pixel
    }

    /// Theme colors used to answer OSC 10/11/4 color queries.
    pub fn set_palette(&self, p: &Palette) {
        *self.pal.lock() = Some(p.clone());
    }

    /// Back to a fresh terminal (screen, scrollback, modes, marks), keeping size and
    /// options: a reattach replays the pty's output from scratch.
    pub fn reset(&self) {
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        t.reset_state();
        Self::reclaim(&mut t);
        *p = Processor::new();
        *self.shell.lock() = Shell::default();
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
        let scrollback = scrollback.min(100_000);
        self.scrollback.store(scrollback, Ordering::Relaxed);
        let mut t = self.term.lock();
        let cap = history::history_lines(scrollback, t.columns(), self.scrollback_bytes());
        t.set_options(Config {
            scrolling_history: cap,
            kitty_keyboard: true,
            default_cursor_style: CursorStyle { shape, blinking },
            ..Config::default()
        });
        Self::reclaim(&mut t);
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

    pub fn size(&self) -> (u16, u16) {
        let t = self.term.lock();
        (t.columns() as u16, t.screen_lines() as u16)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        // Ghostty: a resize always ends a synchronized update (the half frame was laid out for the old grid).
        // Same lock order as `feed`: parser, then term.
        let mut p = self.parser.lock();
        let mut t = self.term.lock();
        if p.sync_timeout().sync_timeout().is_some() {
            p.stop_sync(&mut *t);
        }
        drop(p);
        t.resize(Size {
            cols: cols as usize,
            rows: rows as usize,
        });
        // The byte budget buys fewer lines at a wider grid.
        self.apply_history(&mut t);
        let mut s = self.win.lock();
        (s.num_cols, s.num_lines) = (cols, rows);
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
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

    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// `(display_offset, max_offset)`.
    pub fn scrollbar(&self) -> (u32, u32) {
        let t = self.term.lock();
        let max = t.grid().total_lines().saturating_sub(t.screen_lines());
        (t.grid().display_offset() as u32, max as u32)
    }

    /// What the wheel rules read, at this instant (`term_stats.vt`).
    pub fn wheel_state(&self) -> WheelState {
        let t = self.term.lock();
        let m = *t.mode();
        let max = t.grid().total_lines().saturating_sub(t.screen_lines());
        WheelState {
            display_offset: t.grid().display_offset() as u32,
            max_offset: max as u32,
            mouse: m.intersects(TermMode::MOUSE_MODE),
            alt_screen: m.contains(TermMode::ALT_SCREEN),
            alt_scroll: m.contains(TermMode::ALTERNATE_SCROLL),
        }
    }

    /// Viewport row -> grid point (rows scroll with the display offset).
    fn point(t: &Term<Proxy>, col: usize, row: i32) -> Point {
        Point {
            line: Line(row - t.grid().display_offset() as i32),
            column: Column(col.min(t.columns().saturating_sub(1))),
        }
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
}
