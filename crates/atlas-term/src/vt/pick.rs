//! What the pointer picks out of the grid: the link under a cell (OSC 8, plain URL, file
//! path), the ⌘-hover underline for it, smart double-click words, shift-click extension and
//! the selection state the debug bridge reads. Pure over `Term`, driven by `Vt`.

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};

use super::Vt;
use crate::search::{self, UrlHit};

/// What a link under the pointer is. The app opens a `Url` in a browser; a `Path` it resolves
/// against the worktree (only it knows whether the file exists there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Url,
    Path,
}

/// The link under a viewport cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    /// The uri (OSC 8 or plain URL) or the path token (`src/a.ts:12:3`).
    pub target: String,
}

/// The link underlined by the ⌘-hover: target and first/last cell as `(grid line, column)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverTarget {
    pub target: String,
    pub start: (i32, usize),
    pub end: (i32, usize),
}

/// The selection as the debug bridge reads it: grid lines (negative = scrollback) and columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionState {
    /// `simple`, `block`, `semantic` or `lines`.
    pub kind: &'static str,
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
}

/// Does this token read as a path or a `path:line` reference rather than a word: a `/` in it, or
/// a trailing `:line` / `:line:col`. Double-clicking one takes all of it (alacritty's separators
/// would stop at `:`).
fn pathlike(tok: &str) -> bool {
    if tok.contains('/') {
        return true;
    }
    let mut parts = tok.rsplitn(3, ':');
    let num =
        |s: Option<&str>| s.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    let (a, b, rest) = (parts.next(), parts.next(), parts.next());
    // `a.ts:12` or `a.ts:12:3`
    (num(a) && b.is_some_and(|b| !b.is_empty()) && rest.is_none())
        || (num(a) && num(b) && rest.is_some_and(|r| !r.is_empty()))
}

/// `name.ext` (optionally `:line[:col]` / `(line,col)`): a dot, then a letter-led extension of up
/// to 8 alphanumerics at the end, so `a.ts` and `Cargo.toml:3` qualify and `plain`, `1.5` and a
/// trailing `.` do not. The app still decides whether the file exists in the worktree.
fn has_extension(tok: &str) -> bool {
    let base = tok
        .split_once('(')
        .map_or(tok, |(b, _)| b)
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == ':');
    let Some((stem, ext)) = base.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && ext.len() <= 8
        && ext.starts_with(|c: char| c.is_ascii_alphabetic())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl Vt {
    /// The link under a viewport cell: an OSC 8 link, else a plain URL, else a file-path-shaped
    /// token (for the app to vouch for).
    pub fn link_at(&self, col: usize, row: i32) -> Option<Link> {
        let t = self.term.lock();
        let p = Self::point_in_grid(&t, col, row)?;
        let (kind, hit) = Self::link_hit(&t, p)?;
        Some(Link {
            kind,
            target: hit.url,
        })
    }

    fn link_hit(
        t: &alacritty_terminal::term::Term<super::Proxy>,
        p: Point,
    ) -> Option<(LinkKind, UrlHit)> {
        if let Some(h) = search::osc8_at(t, p) {
            return Some((LinkKind::Url, h));
        }
        if let Some(h) = search::url_at(t, p) {
            return Some((LinkKind::Url, h));
        }
        search::token_hit(t, p)
            .filter(|h| pathlike(&h.url) || has_extension(&h.url))
            .map(|h| (LinkKind::Path, h))
    }

    /// Underline the link under a viewport cell (⌘ held); `None` clears it. A path-shaped token
    /// is underlined only with `paths` (the app checked it names a file). Returns whether the
    /// hover changed (the caller redraws).
    pub fn hover(&self, at: Option<(usize, i32)>, paths: bool) -> bool {
        let t = self.term.lock();
        let hit = at
            .and_then(|(col, row)| Self::point_in_grid(&t, col, row))
            .and_then(|p| Self::link_hit(&t, p))
            .filter(|(kind, _)| paths || *kind == LinkKind::Url)
            .map(|(_, h)| h);
        let mut sh = self.shell.lock();
        let changed = sh.hover != hit;
        sh.hover = hit;
        changed
    }

    /// The link now underlined by `hover` and the cells it covers, for the debug bridge.
    pub fn hover_target(&self) -> Option<HoverTarget> {
        let sh = self.shell.lock();
        let h = sh.hover.as_ref()?;
        Some(HoverTarget {
            target: h.url.clone(),
            start: (h.start.line.0, h.start.column.0),
            end: (h.end.line.0, h.end.column.0),
        })
    }

    /// Start a selection at a viewport cell.
    pub fn start_selection(&self, col: usize, row: i32, ty: SelectionType, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        t.selection = Some(Selection::new(ty, p, side));
        self.shell.lock().word = None;
    }

    /// Move the end of the selection to a viewport cell (a drag). A selection that came from a
    /// double-click is anchored on its word: a release inside the word keeps it whole, a drag
    /// past either end grows it from the word's far end.
    pub fn update_selection(&self, col: usize, row: i32, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        let word = self.shell.lock().word;
        let Some(sel) = t.selection.as_mut() else {
            return;
        };
        match word {
            Some((first, last)) if p >= first && p <= last => {
                // Back inside the word: the word itself.
                *sel = Selection::new(SelectionType::Simple, first, Side::Left);
                sel.update(last, Side::Right);
            }
            Some((first, last)) if p < first => {
                *sel = Selection::new(SelectionType::Simple, last, Side::Right);
                sel.update(p, side);
            }
            Some((first, _)) => {
                *sel = Selection::new(SelectionType::Simple, first, Side::Left);
                sel.update(p, side);
            }
            None => sel.update(p, side),
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
        self.shell.lock().word = None;
    }

    /// Double-click at a viewport cell: a URL or a path reference is one word; anything else is
    /// selected by alacritty's semantic boundaries (and a drag from here extends by word).
    pub fn select_word(&self, col: usize, row: i32, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        let whole = Self::point_in_grid(&t, col, row).and_then(|p| {
            search::url_at(&t, p).or_else(|| search::token_hit(&t, p).filter(|h| pathlike(&h.url)))
        });
        self.shell.lock().word = whole.as_ref().map(|h| (h.start, h.end));
        t.selection = Some(match whole {
            Some(h) => {
                let mut sel = Selection::new(SelectionType::Simple, h.start, Side::Left);
                sel.update(h.end, Side::Right);
                sel
            }
            None => Selection::new(SelectionType::Semantic, p, side),
        });
    }

    /// Shift-click: move the end of the selection to a viewport cell, keeping its anchor. With no
    /// selection yet, one starts at that cell (so a ⇧-drag selects from the press).
    pub fn extend_selection(&self, col: usize, row: i32, side: Side) {
        let mut t = self.term.lock();
        let p = Self::point(&t, col, row);
        if t.selection.is_none() {
            t.selection = Some(Selection::new(SelectionType::Simple, p, side));
            self.shell.lock().word = None;
        }
        if let Some(sel) = t.selection.as_mut() {
            sel.update(p, side);
        }
    }

    /// A drag past the top (`lines` > 0, into history) or bottom (`lines` < 0) edge: scroll the
    /// viewport and put the selection's end on the edge row, at `col`, so it grows with the text
    /// that comes into view. A no-op for the selection when none is in progress.
    pub fn select_scroll(&self, lines: i32, col: usize, side: Side) {
        let mut t = self.term.lock();
        t.scroll_display(Scroll::Delta(lines));
        let row = if lines > 0 {
            0
        } else {
            t.screen_lines() as i32 - 1
        };
        let p = Self::point(&t, col, row);
        if let Some(sel) = t.selection.as_mut() {
            sel.update(p, side);
        }
    }

    /// Selected text, if any (the selection stays highlighted). A whole-line selection (triple-click,
    /// ⌘A) drops its last line break: pasting `ls\n` into a shell would run it.
    pub fn end_selection(&self) -> Option<String> {
        let t = self.term.lock();
        let lines = t
            .selection
            .as_ref()
            .is_some_and(|s| s.ty == SelectionType::Lines);
        let text = t.selection_to_string().filter(|s| !s.is_empty())?;
        Some(match text.strip_suffix('\n') {
            Some(rest) if lines => rest.to_string(),
            _ => text,
        })
    }

    /// The selection's kind and cells, or `None` (debug bridge).
    pub fn selection_state(&self) -> Option<SelectionState> {
        let t = self.term.lock();
        let sel = t.selection.as_ref()?;
        let r = sel.to_range(&t)?;
        let kind = match sel.ty {
            SelectionType::Simple => "simple",
            SelectionType::Block => "block",
            SelectionType::Semantic => "semantic",
            SelectionType::Lines => "lines",
        };
        Some(SelectionState {
            kind,
            start_line: r.start.line.0,
            start_col: r.start.column.0,
            end_line: r.end.line.0,
            end_col: r.end.column.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use alacritty_terminal::selection::SelectionType;

    fn decos(vt: &Vt) -> Vec<(u16, u16)> {
        let mut f = Frame::default();
        let pal = crate::colors::Palette::new(
            alacritty_terminal::vte::ansi::Rgb {
                r: 200,
                g: 200,
                b: 200,
            },
            alacritty_terminal::vte::ansi::Rgb {
                r: 10,
                g: 10,
                b: 10,
            },
            None,
        );
        vt.snapshot(&pal, &mut f);
        f.decos.iter().map(|d| (d.row, d.col)).collect()
    }

    fn feed_links(vt: &Vt) {
        vt.feed(b"plain https://example.com/docs?q=1 end\r\n");
        vt.feed(b"path src/a.ts:2:5 here\r\n");
        vt.feed(b"osc8 \x1b]8;;https://osc8.example/x\x1b\\click-me\x1b]8;;\x1b\\ done\r\n");
    }

    // A link under the pointer says what it is: the app opens a URL in a browser and a path in
    // the editor, and only the app knows whether a path is a file of the worktree.
    #[test]
    fn link_at_tells_osc8_urls_and_paths_apart() {
        let vt = Vt::new(60, 5, |_| {});
        feed_links(&vt);
        let url = vt.link_at(10, 0).unwrap();
        assert_eq!(
            (url.kind, url.target.as_str()),
            (LinkKind::Url, "https://example.com/docs?q=1")
        );
        let path = vt.link_at(8, 1).unwrap();
        assert_eq!(
            (path.kind, path.target.as_str()),
            (LinkKind::Path, "src/a.ts:2:5")
        );
        let osc = vt.link_at(8, 2).unwrap();
        assert_eq!(
            (osc.kind, osc.target.as_str()),
            (LinkKind::Url, "https://osc8.example/x")
        );
        assert_eq!(vt.link_at(1, 0), None, "plain word is no link");
        assert_eq!(vt.link_at(50, 0), None, "empty cell is no link");
    }

    // Incident: ⌘-hover underlined plain URLs only; an OSC 8 link (the one link a program
    // really asked for) and a path showed nothing, so nothing said they were clickable.
    #[test]
    fn hover_underlines_the_whole_osc8_link_not_just_urls() {
        let vt = Vt::new(60, 5, |_| {});
        feed_links(&vt);
        assert!(vt.hover(Some((8, 2)), false));
        let ul = decos(&vt);
        // "click-me" = 8 cells starting at col 5 of row 2.
        assert_eq!(ul, (5..13).map(|c| (2, c)).collect::<Vec<_>>());
        assert!(
            !vt.hover(Some((9, 2)), false),
            "same link: nothing to redraw"
        );
        assert!(vt.hover(None, false));
        assert!(decos(&vt).is_empty());
    }

    #[test]
    fn hover_underlines_a_path_only_when_the_app_accepts_paths() {
        let vt = Vt::new(60, 5, |_| {});
        feed_links(&vt);
        assert!(
            !vt.hover(Some((8, 1)), false),
            "a path the app did not vouch for stays plain"
        );
        assert!(decos(&vt).is_empty());
        assert!(vt.hover(Some((8, 1)), true));
        let ul = decos(&vt);
        assert_eq!(ul.len(), "src/a.ts:2:5".len());
        assert_eq!(ul[0], (1, 5));
    }

    #[test]
    fn an_osc8_link_that_wraps_is_one_link_and_one_underline() {
        let vt = Vt::new(10, 4, |_| {});
        vt.feed(b"\x1b]8;;https://w.example/\x1b\\0123456789ABCDE\x1b]8;;\x1b\\");
        let a = vt.link_at(2, 0).unwrap();
        let b = vt.link_at(3, 1).unwrap();
        assert_eq!(a, b);
        assert!(vt.hover(Some((2, 0)), false));
        assert_eq!(decos(&vt).len(), 15);
    }

    // Two adjacent links with different targets must not merge into one underline.
    #[test]
    fn adjacent_osc8_links_stay_separate() {
        let vt = Vt::new(40, 3, |_| {});
        vt.feed(b"\x1b]8;;https://a.example/\x1b\\aaa\x1b]8;;\x1b\\\x1b]8;;https://b.example/\x1b\\bbb\x1b]8;;\x1b\\");
        assert_eq!(vt.link_at(1, 0).unwrap().target, "https://a.example/");
        assert_eq!(vt.link_at(4, 0).unwrap().target, "https://b.example/");
        assert!(vt.hover(Some((1, 0)), false));
        assert_eq!(decos(&vt), vec![(0, 0), (0, 1), (0, 2)]);
    }

    fn word(vt: &Vt, col: usize, row: i32) -> Option<String> {
        vt.select_word(col, row, Side::Left);
        vt.end_selection()
    }

    // Incident: a double-click inside a URL stopped at the `:` (alacritty's semantic
    // separators), copying `//a.dev/x?k=v` — useless to paste. A URL or a path is one word.
    #[test]
    fn double_click_selects_a_whole_url_or_path_reference() {
        let vt = Vt::new(80, 6, |_| {});
        vt.feed(b"go https://a.dev/x_y-z?k=v#frag now\r\n");
        vt.feed(b"at src/a.ts:12:3 (ok)\r\n");
        vt.feed(b"and /usr/local/bin/foo-bar.txt.\r\n");
        vt.feed(b"plain words here\r\n");
        assert_eq!(
            word(&vt, 20, 0).as_deref(),
            Some("https://a.dev/x_y-z?k=v#frag")
        );
        assert_eq!(
            word(&vt, 4, 0).as_deref(),
            Some("https://a.dev/x_y-z?k=v#frag"),
            "on the scheme too"
        );
        assert_eq!(
            word(&vt, 6, 1).as_deref(),
            Some("src/a.ts:12:3"),
            "line and column belong to the path"
        );
        assert_eq!(
            word(&vt, 10, 2).as_deref(),
            Some("/usr/local/bin/foo-bar.txt")
        );
        assert_eq!(
            word(&vt, 8, 3).as_deref(),
            Some("words"),
            "ordinary text keeps word boundaries"
        );
    }

    #[test]
    fn double_click_on_a_url_that_wraps_selects_all_of_it() {
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(b"https://example.com/a/very/long/path?x=1");
        assert_eq!(
            word(&vt, 3, 1).as_deref().map(|s| s.replace('\n', "")),
            Some("https://example.com/a/very/long/path?x=1".to_string())
        );
    }

    #[test]
    fn double_click_on_blank_space_selects_nothing_odd() {
        let vt = Vt::new(40, 3, |_| {});
        vt.feed(b"a   b\r\n");
        let w = word(&vt, 2, 0);
        assert!(
            w.as_deref().is_none_or(|s| s.trim().is_empty()),
            "got {w:?}"
        );
    }

    // Incident: shift-click started a new selection, so there was no way to extend one by
    // keyboard-free means (Ghostty, iTerm2, Terminal.app all extend).
    #[test]
    fn shift_click_extends_the_selection_from_its_anchor() {
        let vt = Vt::new(20, 6, |_| {});
        vt.feed(b"line one\r\nline two\r\nline three\r\n");
        vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
        vt.update_selection(3, 0, Side::Right);
        assert_eq!(vt.end_selection().as_deref(), Some("line"));
        vt.extend_selection(3, 2, Side::Right);
        assert_eq!(
            vt.end_selection().as_deref(),
            Some("line one\nline two\nline")
        );
        // Shrinking back works too (the anchor stays at the start).
        vt.extend_selection(3, 0, Side::Right);
        assert_eq!(vt.end_selection().as_deref(), Some("line"));
    }

    // Incident (seen live in a program that owns the mouse): ⇧-drag there started its selection at the
    // text cursor instead of under the pointer, so nothing could be selected where the press was.
    #[test]
    fn shift_click_without_a_selection_starts_at_the_pressed_cell() {
        let vt = Vt::new(20, 3, |_| {});
        vt.feed(b"hello world");
        vt.extend_selection(6, 0, Side::Left);
        vt.update_selection(10, 0, Side::Right);
        assert_eq!(vt.end_selection().as_deref(), Some("world"));
    }

    // Incident: triple-click copied "line five\n"; pasting that into a shell runs the line.
    #[test]
    fn a_line_selection_copies_without_its_trailing_newline() {
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(b"line five\r\nline six\r\n");
        vt.start_selection(2, 0, SelectionType::Lines, Side::Left);
        assert_eq!(vt.end_selection().as_deref(), Some("line five"));
        vt.update_selection(2, 1, Side::Left);
        assert_eq!(vt.end_selection().as_deref(), Some("line five\nline six"));
    }

    // The selection is anchored to content, not to the screen: output that scrolls the text up
    // moves it with the text, and the copy is still the same words.
    #[test]
    fn selection_scrolls_with_streaming_output() {
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(b"keep me\r\nb\r\nc\r\n");
        vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
        vt.update_selection(6, 0, Side::Right);
        for i in 0..6 {
            vt.feed(format!("out {i}\r\n").as_bytes());
        }
        assert_eq!(vt.end_selection().as_deref(), Some("keep me"));
        let s = vt.selection_state().unwrap();
        assert!(
            s.start_line < 0,
            "the selected text is now in the scrollback: {s:?}"
        );
    }

    // The mouse may leave the slot while dragging: selecting a row that is not on screen (the
    // autoscroll target) must not panic or clamp to nonsense.
    #[test]
    fn selecting_past_the_viewport_stays_inside_the_grid() {
        let vt = Vt::new(10, 3, |_| {});
        for i in 0..30 {
            vt.feed(format!("{i}\r\n").as_bytes());
        }
        vt.scroll(5);
        vt.start_selection(0, 1, SelectionType::Simple, Side::Left);
        vt.update_selection(3, 400, Side::Right);
        vt.update_selection(3, -400, Side::Right);
        assert!(vt.end_selection().is_some());
    }

    #[test]
    fn selection_state_reports_kind_and_cells_for_the_debug_bridge() {
        let vt = Vt::new(20, 4, |_| {});
        vt.feed(b"abcdef\r\n");
        assert_eq!(vt.selection_state(), None);
        vt.start_selection(1, 0, SelectionType::Block, Side::Left);
        vt.update_selection(3, 1, Side::Right);
        let s = vt.selection_state().unwrap();
        assert_eq!(s.kind, "block");
        assert_eq!((s.start_col, s.end_col), (1, 3));
    }

    // Dragging past the top or bottom edge scrolls the viewport and keeps the selection's end on
    // the edge row, so it grows with the text that comes into view (Ghostty, every terminal).
    #[test]
    fn dragging_past_the_edge_scrolls_and_extends_the_selection() {
        let vt = Vt::new(12, 4, |_| {});
        for i in 0..40 {
            vt.feed(format!("row {i:02}\r\n").as_bytes());
        }
        // Scrolled up 10 lines; press on the first visible row, then drag below the bottom edge.
        vt.scroll(10);
        let (off, _) = vt.scrollbar();
        vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
        vt.select_scroll(-3, 5, Side::Right);
        assert_eq!(
            vt.scrollbar().0,
            off - 3,
            "the viewport followed the drag down"
        );
        let text = vt.end_selection().unwrap();
        // From the first visible row down to the new bottom row: more than the 4 rows of a screen.
        assert!(
            text.lines().count() > 4,
            "selection grew with the scroll: {text:?}"
        );
        // And upward past the top edge.
        vt.select_scroll(8, 0, Side::Left);
        assert_eq!(vt.scrollbar().0, off - 3 + 8);
        // Never past the ends of the scrollback.
        vt.select_scroll(10_000, 0, Side::Left);
        let (o, max) = vt.scrollbar();
        assert_eq!(o, max);
        vt.select_scroll(-10_000, 0, Side::Left);
        assert_eq!(vt.scrollbar().0, 0);
        assert!(vt.end_selection().is_some());
    }

    #[test]
    fn hover_target_names_the_underlined_link_for_the_debug_bridge() {
        let vt = Vt::new(60, 5, |_| {});
        feed_links(&vt);
        assert_eq!(vt.hover_target(), None);
        vt.hover(Some((8, 2)), false);
        let t = vt.hover_target().unwrap();
        assert_eq!(t.target, "https://osc8.example/x");
        assert_eq!((t.start, t.end), ((2, 5), (2, 12)));
    }

    // Incident (seen in the live app, not by the unit tests above): the page ends a click with
    // `update_selection` at the release cell. That moved the end of a double-click's selection to the
    // cell under the pointer and cut `https://a.dev/x` to `https://a.d`. A word selection is anchored
    // on its word: a release inside it changes nothing, a drag past it grows it.
    #[test]
    fn releasing_inside_a_double_clicked_word_keeps_the_whole_word() {
        let vt = Vt::new(80, 3, |_| {});
        vt.feed(b"go https://a.dev/x_y-z?k=v#frag now\r\n");
        vt.select_word(20, 0, Side::Left);
        vt.update_selection(20, 0, Side::Left);
        assert_eq!(
            vt.end_selection().as_deref(),
            Some("https://a.dev/x_y-z?k=v#frag")
        );
        vt.update_selection(4, 0, Side::Left);
        assert_eq!(
            vt.end_selection().as_deref(),
            Some("https://a.dev/x_y-z?k=v#frag")
        );
        // Dragging past the word's end grows it by cells from the word's start.
        vt.update_selection(34, 0, Side::Right);
        assert_eq!(
            vt.end_selection().as_deref(),
            Some("https://a.dev/x_y-z?k=v#frag now")
        );
        // Dragging back before its start grows it to the left, keeping the word's far end.
        vt.update_selection(0, 0, Side::Left);
        assert_eq!(
            vt.end_selection().as_deref(),
            Some("go https://a.dev/x_y-z?k=v#frag")
        );
        // A new selection forgets the word.
        vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
        vt.update_selection(1, 0, Side::Right);
        assert_eq!(vt.end_selection().as_deref(), Some("go"));
    }
}
