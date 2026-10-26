//! Regex search over the whole grid (scrollback + screen) and plain-URL detection.
//! Pure functions over `Term`, driven by `Vt`.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// Most matches kept per search (a runaway regex on a 100k-line scrollback).
pub const MAX_MATCHES: usize = 20_000;

/// Pattern for the user's query: literal text is escaped; the inline flag forces
/// case (in/)sensitivity over alacritty's smart-case default.
pub fn pattern(query: &str, regex: bool, case: bool) -> String {
    let body = if regex {
        query.to_string()
    } else {
        let mut s = String::with_capacity(query.len() * 2);
        for c in query.chars() {
            if "\\.+*?()|[]{}^$#&-~".contains(c) {
                s.push('\\');
            }
            s.push(c);
        }
        s
    };
    format!("(?{}:{body})", if case { "-i" } else { "i" })
}

pub fn compile(query: &str, regex: bool, case: bool) -> Result<RegexSearch, String> {
    RegexSearch::new(&pattern(query, regex, case)).map_err(|e| format!("invalid pattern: {e}"))
}

/// Every match from the top of the scrollback to the bottom of the screen, in order.
pub fn all_matches<T>(t: &Term<T>, re: &mut RegexSearch) -> Vec<Match> {
    let start = Point::new(t.topmost_line(), Column(0));
    let end = Point::new(t.bottommost_line(), t.last_column());
    RegexIter::new(start, end, Direction::Right, t, re)
        .take(MAX_MATCHES)
        .collect()
}

/// Characters that end a URL (whitespace, quotes, brackets that do not close one).
fn url_char(c: char) -> bool {
    !(c.is_whitespace() || c == '\0' || "<>\"'`{}|\\^".contains(c))
}

/// Trailing punctuation that belongs to the sentence, not the URL; a `)` stays when
/// the URL has its `(`.
fn trim_url(s: &str) -> &str {
    let mut s = s;
    loop {
        let Some(last) = s.chars().next_back() else {
            return s;
        };
        let strip = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '*' | ']' => true,
            ')' => s.matches('(').count() < s.matches(')').count(),
            _ => false,
        };
        if !strip {
            return s;
        }
        s = &s[..s.len() - last.len_utf8()];
    }
}

/// The `http(s)://` / `file://` URL covering char index `at` of `line`, as
/// `(start, end_exclusive)` char indices (the line is one char per cell).
pub fn url_span(line: &[char], at: usize) -> Option<(usize, usize)> {
    if at >= line.len() || !url_char(line[at]) {
        return None;
    }
    let mut s = at;
    while s > 0 && url_char(line[s - 1]) {
        s -= 1;
    }
    let mut e = at + 1;
    while e < line.len() && url_char(line[e]) {
        e += 1;
    }
    let text: String = line[s..e].iter().collect();
    // A token may carry a prefix (`(see https://…`, `[https://…`): find the scheme.
    let scheme = ["https://", "http://", "file://"]
        .iter()
        .filter_map(|p| text.find(p))
        .min()?;
    let url = trim_url(&text[scheme..]);
    let host_start = url.find("://")? + 3;
    if url.len() <= host_start {
        return None;
    }
    let from = s + text[..scheme].chars().count();
    let to = from + url.chars().count();
    (from..to).contains(&at).then_some((from, to))
}

/// A URL found on the grid: its text and the flat cell range it covers.
#[derive(Clone, Debug, PartialEq)]
pub struct UrlHit {
    pub url: String,
    /// First/last cell (inclusive), grid coordinates.
    pub start: Point,
    pub end: Point,
}

/// The soft-wrapped paragraph holding grid point `p` flattened to one char per cell (a wide
/// char's spacer cell is `'\0'`), plus the first line, so a flat index maps back to a cell.
fn paragraph<T>(t: &Term<T>, p: Point) -> (Vec<char>, Line) {
    let cols = t.columns();
    let grid = t.grid();
    let wraps = |l: Line| grid[l][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    // Walk up to the first line of the wrapped paragraph and down to its last.
    let mut first = p.line;
    while first > t.topmost_line() && wraps(first - 1) {
        first -= 1;
    }
    let mut last = p.line;
    while last < t.bottommost_line() && wraps(last) {
        last += 1;
    }
    let mut chars: Vec<char> = Vec::new();
    let mut line = first;
    while line <= last {
        for c in 0..cols {
            let cell = &grid[line][Column(c)];
            chars.push(match cell.c {
                _ if cell.flags.contains(Flags::WIDE_CHAR_SPACER) => '\0',
                '\0' => ' ',
                c => c,
            });
        }
        line += 1;
    }
    (chars, first)
}

/// The URL under grid point `p`, following soft-wrapped lines (a URL longer than
/// the width wraps).
pub fn url_at<T>(t: &Term<T>, p: Point) -> Option<UrlHit> {
    let cols = t.columns();
    let (chars, first) = paragraph(t, p);
    let at = (p.line.0 - first.0) as usize * cols + p.column.0;
    let (s, e) = url_span(&chars, at)?;
    let cell = |i: usize| Point::new(first + i / cols, Column(i % cols));
    Some(UrlHit {
        url: chars[s..e].iter().filter(|&&c| c != '\0').collect(),
        start: cell(s),
        end: cell(e - 1),
    })
}

/// The whitespace-delimited token covering char index `at`, minus the brackets, quotes and
/// sentence punctuation around it (`(./b.rs:7).` → `./b.rs:7`), as `(start, end_exclusive)`.
/// A file reference (`src/a.ts:12:3`) is such a token; whether it names a real file is for the
/// caller to decide.
pub fn token_span(line: &[char], at: usize) -> Option<(usize, usize)> {
    // A wide char's spacer cell (`'\0'`) is part of the word its char is in.
    let word = |c: char| !c.is_whitespace();
    if at >= line.len() || !word(line[at]) {
        return None;
    }
    let mut s = at;
    while s > 0 && word(line[s - 1]) {
        s -= 1;
    }
    let mut e = at + 1;
    while e < line.len() && word(line[e]) {
        e += 1;
    }
    while s < e && "([{<\"'`".contains(line[s]) {
        s += 1;
    }
    let text: String = line[s..e].iter().collect();
    let kept = trim_url(text.trim_end_matches(['"', '\'', '`', '>', '}']));
    let to = s + kept.chars().count();
    (s < to && (s..to).contains(&at)).then_some((s, to))
}

/// The token under grid point `p`, following soft-wrapped lines.
pub fn token_at<T>(t: &Term<T>, p: Point) -> Option<String> {
    let (chars, first) = paragraph(t, p);
    let at = (p.line.0 - first.0) as usize * t.columns() + p.column.0;
    let (s, e) = token_span(&chars, at)?;
    Some(chars[s..e].iter().filter(|&&c| c != '\0').collect())
}

/// The text token under grid point `p` with the cells it covers (the hover underline of a path).
pub fn token_hit<T>(t: &Term<T>, p: Point) -> Option<UrlHit> {
    let cols = t.columns();
    let (chars, first) = paragraph(t, p);
    let at = (p.line.0 - first.0) as usize * cols + p.column.0;
    let (s, e) = token_span(&chars, at)?;
    let cell = |i: usize| Point::new(first + i / cols, Column(i % cols));
    Some(UrlHit {
        url: chars[s..e].iter().filter(|&&c| c != '\0').collect(),
        start: cell(s),
        end: cell(e - 1),
    })
}

/// The OSC 8 hyperlink covering grid point `p`: its uri and the run of cells that carry the same
/// link (same uri and id), following soft-wrapped lines. Two links side by side stay two.
pub fn osc8_at<T>(t: &Term<T>, p: Point) -> Option<UrlHit> {
    let link = t.grid()[p].hyperlink()?;
    let cols = t.columns();
    let grid = t.grid();
    let same = |q: Point| grid[q].hyperlink().is_some_and(|h| h == link);
    let wraps = |l: Line| grid[l][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    // Walk back to the first cell of the run, then forward to the last, across wrapped rows only.
    let mut start = p;
    loop {
        let prev = if start.column.0 > 0 {
            Point::new(start.line, start.column - 1)
        } else if start.line > t.topmost_line() && wraps(start.line - 1) {
            Point::new(start.line - 1, Column(cols - 1))
        } else {
            break;
        };
        if !same(prev) {
            break;
        }
        start = prev;
    }
    let mut end = p;
    loop {
        let next = if end.column.0 + 1 < cols {
            Point::new(end.line, end.column + 1)
        } else if end.line < t.bottommost_line() && wraps(end.line) {
            Point::new(end.line + 1, Column(0))
        } else {
            break;
        };
        if !same(next) {
            break;
        }
        end = next;
    }
    Some(UrlHit {
        url: link.uri().to_string(),
        start,
        end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(s: &str, at: usize) -> Option<String> {
        let line: Vec<char> = s.chars().collect();
        url_span(&line, at).map(|(a, b)| line[a..b].iter().collect())
    }

    #[test]
    fn trims_sentence_punctuation_but_keeps_balanced_parens() {
        assert_eq!(
            span("go to https://a.dev/x.", 8).as_deref(),
            Some("https://a.dev/x")
        );
        assert_eq!(
            span("(https://a.dev/x)", 3).as_deref(),
            Some("https://a.dev/x")
        );
        assert_eq!(
            span("https://en.wikipedia.org/wiki/Foo_(bar)", 3).as_deref(),
            Some("https://en.wikipedia.org/wiki/Foo_(bar)")
        );
        assert_eq!(
            span("file:///tmp/a%20b.txt,", 0).as_deref(),
            Some("file:///tmp/a%20b.txt")
        );
    }

    #[test]
    fn ignores_non_urls_and_positions_outside_them() {
        assert_eq!(span("just text", 2), None);
        assert_eq!(span("https://", 2), None);
        assert_eq!(span("ftp://x.y", 2), None);
        assert_eq!(span("a https://x.io b", 0), None);
        assert_eq!(span("a https://x.io b", 15), None);
    }

    #[test]
    fn literal_queries_are_escaped() {
        assert_eq!(pattern("a.b", false, false), r"(?i:a\.b)");
        assert_eq!(pattern("a.b", true, true), "(?-i:a.b)");
    }
}
