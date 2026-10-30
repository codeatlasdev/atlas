//! Things found in the grid: OSC 8 links, plain URLs, paths and the incremental search (the
//! ⌘-hover underline and word picking are in `pick`). The matching itself is in `crate::search`.

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Line, Point};
use alacritty_terminal::term::Term;

use super::shell::Search;
use super::{Proxy, Vt};
use crate::search;

/// Result of a search step: 1-based `index` of the current match (0 = none) of `total`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStat {
    pub index: usize,
    pub total: usize,
}

impl Vt {
    /// Grid point under a viewport cell, or `None` when it is outside the scrollback.
    pub(super) fn point_in_grid(t: &Term<Proxy>, col: usize, row: i32) -> Option<Point> {
        let p = Self::point(t, col, row);
        (p.line >= t.topmost_line() && p.line <= t.bottommost_line()).then_some(p)
    }

    /// URL under a viewport cell: OSC 8 first, else a plain http(s)/file URL in the text.
    pub fn url_at(&self, col: usize, row: i32) -> Option<String> {
        let t = self.term.lock();
        let p = Self::point_in_grid(&t, col, row)?;
        if let Some(h) = t.grid()[p].hyperlink() {
            return Some(h.uri().to_string());
        }
        search::url_at(&t, p).map(|h| h.url)
    }

    /// The token under a viewport cell (a path such as `src/a.ts:12:3`), for ⌘-click when no
    /// URL is there; the app decides whether it names a file.
    pub fn path_at(&self, col: usize, row: i32) -> Option<String> {
        let t = self.term.lock();
        let p = Self::point_in_grid(&t, col, row)?;
        search::token_at(&t, p)
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
