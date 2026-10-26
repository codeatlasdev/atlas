//! Per-terminal state alacritty does not keep: prompt marks (OSC 133), the cwd (OSC 7),
//! inline images, the active search and the URL hover.

use std::collections::VecDeque;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};

use super::pixel::PixelScan;
use super::scan::{ClearScan, SeqScan};
use super::{Proxy, Vt, VtEvent};
use crate::graphics::Store;
use crate::search::UrlHit;

/// A prompt start (OSC 133;A). `line` counts from the top of the scrollback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Mark {
    pub(super) line: i64,
    pub(super) exit: Option<i32>,
    /// The command typed at this prompt is running (OSC 133;C seen, no 133;D yet).
    pub(super) running: bool,
    /// Output lines of the command (OSC 133;C to 133;D): `(first, end_exclusive)` counted like
    /// `line`. `end` is `None` while the command runs.
    pub(super) out: Option<(i64, Option<i64>)>,
}

/// An active search: the compiled query and its matches (recomputed after output).
pub(super) struct Search {
    pub(super) re: RegexSearch,
    pub(super) matches: Vec<Match>,
    /// Index into `matches` of the current one.
    pub(super) current: usize,
    /// Output arrived since `matches` was computed.
    pub(super) stale: bool,
}

pub(super) struct Shell {
    pub(super) scan: SeqScan,
    pub(super) clear: ClearScan,
    /// DECSET 1016 (SGR-Pixels), which alacritty ignores: the scanner and the state it keeps.
    pub(super) pixel_scan: PixelScan,
    pub(super) pixel: bool,
    pub(super) marks: VecDeque<Mark>,
    pub(super) images: Store,
    /// Cell size in physical px (image size ↔ cells), set by the renderer.
    pub(super) cell: [f32; 2],
    pub(super) alt: bool,
    pub(super) search: Option<Search>,
    /// Compiled pattern of `search` (a different key restarts the search).
    pub(super) search_key: Option<String>,
    /// URL under the pointer while ⌘ is held (drawn underlined).
    pub(super) hover: Option<UrlHit>,
    /// Start line of the output last selected by `select_last_output` (the next call goes
    /// further back); reset by a new prompt.
    pub(super) out_walk: Option<i64>,
    /// First and last cell of the word a double-click selected (a URL or path is one word): the
    /// anchor a drag from it grows from. Any other selection start clears it.
    pub(super) word: Option<(Point, Point)>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            scan: SeqScan::default(),
            clear: ClearScan::default(),
            pixel_scan: PixelScan::default(),
            pixel: false,
            marks: VecDeque::new(),
            images: Store::default(),
            cell: [8.0, 16.0],
            alt: false,
            search: None,
            search_key: None,
            hover: None,
            out_walk: None,
            word: None,
        }
    }
}

/// Hand a notification to the host. Control characters are dropped (they would reach the OS
/// banner), an empty body is nothing to show, an empty title is none.
fn notify(vt: &Vt, title: Option<&str>, body: &str) {
    let clean = |s: &str| s.chars().filter(|c| !c.is_control()).collect::<String>();
    let body = clean(body);
    if body.is_empty() {
        return;
    }
    let title = title.map(clean).filter(|t| !t.is_empty());
    (vt.on_event)(VtEvent::Notify { title, body });
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

impl Vt {
    pub(super) fn apply_osc(&self, t: &Term<Proxy>, sh: &mut Shell, payload: &[u8]) {
        let cursor_line =
            |t: &Term<Proxy>| t.grid().history_size() as i64 + t.grid().cursor.point.line.0 as i64;
        let s = String::from_utf8_lossy(payload);
        let mut parts = s.splitn(3, ';');
        match (parts.next(), parts.next()) {
            (Some("133"), Some("A")) => {
                let line = cursor_line(t);
                sh.out_walk = None;
                if sh.marks.back().is_none_or(|m| m.line != line) {
                    if sh.marks.len() == 1000 {
                        sh.marks.pop_front();
                    }
                    sh.marks.push_back(Mark {
                        line,
                        exit: None,
                        running: false,
                        out: None,
                    });
                } else if let Some(m) = sh.marks.back_mut() {
                    m.running = false;
                }
            }
            (Some("133"), Some("C")) => {
                let line = cursor_line(t);
                if let Some(m) = sh.marks.back_mut() {
                    m.running = true;
                    m.out = Some((line, None));
                }
            }
            (Some("133"), Some("D")) => {
                let code = parts.next().and_then(|c| c.trim().parse().ok());
                // Output ends before the cursor's line, or on it when text is left there.
                let c = t.grid().cursor.point;
                let end = cursor_line(t) + i64::from(c.column.0 > 0);
                if let Some(m) = sh.marks.back_mut() {
                    m.exit = code;
                    m.running = false;
                    if let Some((start, None)) = m.out {
                        m.out = Some((start, Some(end)));
                    }
                }
                (self.on_event)(VtEvent::CommandFinished(code));
            }
            (Some("7"), Some(_)) => {
                let url = &s[2..];
                if let Some(p) = file_url_path(url) {
                    (self.on_event)(VtEvent::Cwd(p));
                }
            }
            // iTerm2 growl `OSC 9;text`. `9;4;…` is ConEmu's progress report, not a message.
            (Some("9"), Some(_)) if !s.starts_with("9;4;") => {
                notify(self, None, &s[2..]);
            }
            // rxvt `OSC 777;notify;title;body` (the body keeps its own `;`).
            (Some("777"), Some("notify")) => {
                let rest = s.splitn(4, ';').skip(2).collect::<Vec<_>>();
                if let [title, body] = rest[..] {
                    notify(self, Some(title), body);
                }
            }
            _ => {}
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

    /// Select the output of the newest finished command that printed something (OSC 133
    /// C..D). Asking again walks back to the one before. `false` = none (no marks, only
    /// running or empty commands, or the alternate screen).
    pub fn select_last_output(&self) -> bool {
        let mut t = self.term.lock();
        let mut sh = self.shell.lock();
        if t.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        let before = sh.out_walk.unwrap_or(i64::MAX);
        let found = sh
            .marks
            .iter()
            .rev()
            .filter_map(|m| m.out.and_then(|(s, e)| Some((s, e?))))
            .find(|&(s, e)| s < before && e > s);
        let Some((start, end)) = found else {
            sh.out_walk = None;
            return false;
        };
        sh.out_walk = Some(start);
        let hist = t.grid().history_size() as i64;
        let line = |l: i64| Line((l - hist) as i32);
        let cols = t.columns();
        let mut sel = Selection::new(
            SelectionType::Lines,
            Point::new(line(start), Column(0)),
            Side::Left,
        );
        sel.update(
            Point::new(line(end - 1), Column(cols.saturating_sub(1))),
            Side::Right,
        );
        t.selection = Some(sel);
        true
    }

    /// Ghostty `clear_screen`: erase the screen and the scrollback and home the cursor.
    /// The caller then sends `^L` so the shell redraws its prompt at the top. Prompt marks
    /// refer to erased lines, so they go too. Not on the alternate screen: a full-screen
    /// program owns it and redraws itself.
    pub fn clear_screen(&self) -> bool {
        if self.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        {
            let mut sh = self.shell.lock();
            sh.marks.clear();
            sh.out_walk = None;
        }
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
}
