use std::cell::Cell;
use std::ops::Range;

use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthChar;

use super::ansi;
use super::rowcache::RowCache;
use crate::runtime::logindex::ViewIndex;
use crate::runtime::logs::{LogBuffer, LogEntry, LogLevel, visible_chars};
use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::{ClickMap, ScrollbarGeometry, Target, Wheel};

const PAD_LEFT: u16 = 2;
const PAD_RIGHT: u16 = 1;
const TAB_WIDTH: usize = 4;

/// Geometry of the last rendered log view, in wrapped screen rows.
/// Written by `render`, read by scroll handlers so they clamp to real content.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogViewport {
    /// Columns available for text (wrap width).
    pub text_width: u16,
    /// Visible rows.
    pub height: usize,
    /// Largest valid `log_scroll` (rows above the bottom).
    pub max_scroll: usize,
    /// Widest line seen (columns) — bounds the horizontal scroll when wrap is off.
    pub max_hscroll: usize,
}

/// How entries are laid out. Row counting and rendering share it so the
/// scroll math can never drift from what is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewOpts {
    /// Show the `[svc]` prefix (off on single-service tabs).
    pub show_service: bool,
    pub timestamps: bool,
    /// Hard-wrap long lines (off = truncate, `hscroll` pans).
    pub wrap: bool,
    /// Compact JSON-object lines into `level msg key=val …`.
    pub json: bool,
    /// Columns panned when `wrap` is off.
    pub hscroll: usize,
}

impl Default for ViewOpts {
    fn default() -> Self {
        Self {
            show_service: true,
            timestamps: true,
            wrap: true,
            json: false,
            hscroll: 0,
        }
    }
}

/// A compiled find query (shared with MCP search and the index).
pub use crate::runtime::logindex::Matcher as Finder;

/// Find highlighting for a render pass.
#[derive(Clone, Copy)]
pub struct FindHighlight<'a> {
    pub finder: &'a Finder,
    /// `(entry seq, occurrence index within the entry)` of the current match.
    pub current: Option<(u64, usize)>,
}

fn char_width(c: char) -> usize {
    if c == '\t' {
        TAB_WIDTH
    } else {
        c.width().unwrap_or(0)
    }
}

/// What the message column of `entry` shows: the (possibly JSON-compacted)
/// text and the level it is colored by.
pub fn display_content(entry: &LogEntry, json: bool) -> (std::borrow::Cow<'_, str>, LogLevel) {
    if json && let Some((level, text)) = compact_json(&entry.content) {
        return (text.into(), level);
    }
    (entry.content.as_str().into(), entry.level)
}

/// `{"level":30,"msg":"x","a":1}` → `("INFO", "x a=1")`-style compaction.
/// Only lines that are exactly one JSON object qualify.
fn compact_json(content: &str) -> Option<(LogLevel, String)> {
    let trimmed = content.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return None;
    }
    let serde_json::Value::Object(map) = serde_json::from_str(trimmed).ok()? else {
        return None;
    };
    let level = map.get("level").or_else(|| map.get("severity"));
    let (lvl, label) = match level {
        Some(serde_json::Value::Number(n)) => match n.as_u64() {
            Some(0..=20) => (LogLevel::Debug, "TRACE"),
            Some(21..=30) => (LogLevel::Info, "INFO"),
            Some(31..=40) => (LogLevel::Warn, "WARN"),
            Some(_) => (LogLevel::Error, "ERROR"),
            None => (LogLevel::Info, "INFO"),
        },
        Some(serde_json::Value::String(s)) => match s.to_ascii_lowercase().as_str() {
            "trace" | "debug" | "verbose" => (LogLevel::Debug, "DEBUG"),
            "warn" | "warning" => (LogLevel::Warn, "WARN"),
            "error" | "err" | "fatal" | "critical" | "panic" => (LogLevel::Error, "ERROR"),
            _ => (LogLevel::Info, "INFO"),
        },
        _ => (LogLevel::detect(content), ""),
    };
    let msg = ["msg", "message"]
        .iter()
        .find_map(|k| map.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("");
    let mut out = String::new();
    if !label.is_empty() {
        out.push_str(label);
        out.push(' ');
    }
    out.push_str(msg);
    for (k, v) in &map {
        if matches!(
            k.as_str(),
            "level"
                | "severity"
                | "msg"
                | "message"
                | "time"
                | "timestamp"
                | "pid"
                | "hostname"
                | "v"
        ) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        match v {
            serde_json::Value::String(s) => out.push_str(&format!("{k}={s}")),
            other => out.push_str(&format!("{k}={other}")),
        }
    }
    Some((lvl, out))
}

/// The `time` / `[svc]` prefix of an entry, exactly as `entry_spans` draws it.
/// Its width is also the hanging indent for continuation rows.
struct Prefix {
    text: String,
}

impl Prefix {
    fn new(entry: &LogEntry, opts: &ViewOpts) -> Self {
        let mut text = String::new();
        if opts.timestamps {
            text.push_str(&entry.time_str);
            text.push(' ');
        }
        if opts.show_service {
            text.push('[');
            text.push_str(&entry.service);
            text.push_str("] ");
        }
        Self { text }
    }

    /// Columns taken by the prefix.
    fn width(&self) -> usize {
        self.text.chars().count()
    }

    fn chars(&self) -> std::str::Chars<'_> {
        self.text.chars()
    }
}

/// Hanging indent for continuation rows, capped so the message keeps room.
fn indent_for(prefix: usize, width: usize) -> usize {
    if prefix + 8 <= width { prefix } else { 0 }
}

/// Number of screen rows `entry` occupies at `width` columns.
/// Must stay in lockstep with `wrap_spans`.
pub fn entry_rows(entry: &LogEntry, width: u16, opts: &ViewOpts) -> usize {
    if !opts.wrap || entry.is_separator() {
        return 1;
    }
    let width = (width as usize).max(1);
    let prefix = Prefix::new(entry, opts);
    let indent = indent_for(prefix.width(), width);
    let (text, _) = display_content(entry, opts.json);
    let (mut rows, mut col) = (1, 0);
    for c in prefix.chars().chain(visible_chars(&text)) {
        let cw = char_width(c);
        if cw == 0 {
            continue;
        }
        if col + cw > width && col > indent {
            rows += 1;
            col = indent;
        }
        col += cw;
    }
    rows
}

/// Hard-wrap styled spans into rows of at most `width` columns (char-level,
/// nothing dropped). Zero-width/control chars are removed, tabs expanded.
/// `prefix` columns of the leading spans form the hanging indent: continuation
/// rows are indented by `prefix` (when it fits) and dimmed.
fn wrap_spans(spans: Vec<Span<'static>>, width: usize, prefix: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let indent = indent_for(prefix, width);
    let dim = Style::default().fg(theme::current().text_muted);
    let mut rows = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut col = 0;
    for span in spans {
        let style = span.style;
        for c in span.content.chars() {
            let cw = char_width(c);
            if cw == 0 {
                continue;
            }
            if col + cw > width && col > indent {
                if !buf.is_empty() {
                    cur.push(Span::styled(std::mem::take(&mut buf), style));
                }
                rows.push(Line::from(std::mem::take(&mut cur)));
                col = indent;
                if indent > 0 {
                    cur.push(Span::styled(" ".repeat(indent), dim));
                }
            }
            if c == '\t' {
                buf.push_str("    ");
            } else {
                buf.push(c);
            }
            col += cw;
        }
        if !buf.is_empty() {
            cur.push(Span::styled(std::mem::take(&mut buf), style));
        }
    }
    rows.push(Line::from(cur));
    rows
}

/// Single-row layout for `wrap = false`: skip `hscroll` columns, show at most
/// `width`, mark clipped edges with `←`/`→`. Returns the row and the full width.
fn truncate_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    hscroll: usize,
) -> (Line<'static>, usize) {
    let dim = Style::default().fg(theme::current().text_dim);
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let (mut col, mut total) = (0usize, 0usize);
    let end = hscroll + width;
    for span in spans {
        let style = span.style;
        for c in span.content.chars() {
            let cw = char_width(c);
            if cw == 0 {
                continue;
            }
            total += cw;
            // Glyphs straddling either edge are dropped.
            if col >= hscroll && col + cw <= end {
                if c == '\t' {
                    buf.push_str("    ");
                } else {
                    buf.push(c);
                }
            }
            col += cw;
        }
        if !buf.is_empty() {
            out.push(Span::styled(std::mem::take(&mut buf), style));
        }
    }
    if total > end
        && let Some(last) = out.last_mut()
    {
        // Replace the final glyph with the "more" marker.
        let mut s = last.content.to_string();
        s.pop();
        s.push('→');
        *last = Span::styled(s, dim);
    }
    (Line::from(out), total)
}

/// Stable per-service color for the prefix label. The palette avoids amber,
/// orange, yellow and red so a prefix is never mistaken for warn/error.
fn service_color(name: &str) -> Color {
    const PALETTE: &[Color] = &[
        Color::Rgb(0x60, 0xA5, 0xFA), // blue
        Color::Rgb(0x34, 0xD3, 0x99), // emerald
        Color::Rgb(0xA7, 0x8B, 0xFA), // violet
        Color::Rgb(0x38, 0xBD, 0xF8), // sky
        Color::Rgb(0x2D, 0xD4, 0xBF), // teal
        Color::Rgb(0x81, 0x8C, 0xF8), // indigo
        Color::Rgb(0xC4, 0xB5, 0xFD), // lavender
        Color::Rgb(0x6E, 0xE7, 0xB7), // mint
    ];
    let hash: u32 = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
    PALETTE[hash as usize % PALETTE.len()]
}

pub use crate::runtime::logindex::search_text;

/// Restyle byte `ranges` of the concatenated span text with `f(range index)`.
/// Ranges must be sorted and non-overlapping.
fn restyle_spans(
    spans: Vec<Span<'static>>,
    ranges: &[(Range<usize>, Style)],
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + ranges.len() * 2);
    let mut pos = 0usize;
    for span in spans {
        let text = span.content.as_ref();
        let (start, end) = (pos, pos + text.len());
        let mut at = 0usize; // byte offset inside `text`
        for (r, patch) in ranges {
            if r.end <= start || r.start >= end {
                continue;
            }
            let a = r.start.max(start) - start;
            let b = r.end.min(end) - start;
            if a < at {
                continue;
            }
            if a > at {
                out.push(Span::styled(text[at..a].to_string(), span.style));
            }
            out.push(Span::styled(
                text[a..b].to_string(),
                span.style.patch(*patch),
            ));
            at = b;
        }
        if at < text.len() {
            out.push(Span::styled(text[at..].to_string(), span.style));
        }
        pos = end;
    }
    out
}

/// A link under the view cursor: opened by `o`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Entry the cursor sits on.
    pub seq: u64,
    /// Which link of that entry (index into `entry_links`).
    pub link: usize,
}

/// URLs / `path:line` references / OSC 8 links of `entry`'s visible message,
/// left to right. Byte ranges are into `search_text(entry, json)`.
pub fn entry_links(entry: &LogEntry, json: bool) -> Vec<crate::runtime::loglinks::Link> {
    use crate::runtime::loglinks::{Link, Target, find};
    if entry.is_separator() {
        return Vec::new();
    }
    let (text, _) = display_content(entry, json);
    // Fast reject: no scheme and no `:<digit>` means nothing to find.
    let b = text.as_bytes();
    let maybe = text.contains("://")
        || text.contains('\x1b')
        || b.windows(2).any(|w| w[0] == b':' && w[1].is_ascii_digit());
    if !maybe {
        return Vec::new();
    }
    let parsed = ansi::parse(theme::current(), &text, Style::default());
    let plain: String = parsed.spans.iter().map(|s| s.content.as_ref()).collect();
    let mut links: Vec<Link> = parsed
        .links
        .iter()
        .map(|l| Link {
            range: l.start..l.end,
            target: Target::Url(l.url.clone()),
        })
        .collect();
    for l in find(&plain) {
        if !links
            .iter()
            .any(|o| o.range.start < l.range.end && l.range.start < o.range.end)
        {
            links.push(l);
        }
    }
    links.sort_by_key(|l| l.range.start);
    links
}

/// Everything one entry draws: prefix columns + (JSON-compacted) message.
fn entry_spans(
    entry: &LogEntry,
    opts: &ViewOpts,
    find: Option<&FindHighlight<'_>>,
    cursor: Option<&Cursor>,
) -> Vec<Span<'static>> {
    let t = theme::current();
    if entry.mark {
        return vec![Span::styled(
            entry.content.clone(),
            Style::default().fg(t.text_dim),
        )];
    }
    if entry.restart {
        return vec![Span::styled(
            entry.content.clone(),
            Style::default().fg(t.accent).add_modifier(Modifier::DIM),
        )];
    }
    let (text, level) = display_content(entry, opts.json);
    let base_style = match level {
        LogLevel::Error => Style::default().fg(t.error),
        LogLevel::Warn => Style::default().fg(t.warning),
        LogLevel::Debug => Style::default().fg(t.text_muted),
        LogLevel::Info => Style::default().fg(t.text),
    };
    let mut spans = Vec::new();
    if opts.timestamps {
        spans.push(Span::styled(
            format!("{} ", entry.time_str),
            Style::default().fg(t.text_dim),
        ));
    }
    if opts.show_service {
        spans.push(Span::styled(
            format!("[{}] ", entry.service),
            Style::default().fg(service_color(&entry.service)),
        ));
    }
    let parsed = ansi::parse(t, &text, base_style);
    let mut msg = parsed.spans;
    // Links (underlined; the cursor's one inverted) then find hits on top.
    let mut patches: Vec<(Range<usize>, Style)> = Vec::new();
    if !entry.truncated || !text.is_empty() {
        let links = entry_links(entry, opts.json);
        for (i, l) in links.iter().enumerate() {
            let mut style = Style::default()
                .fg(t.info)
                .add_modifier(Modifier::UNDERLINED);
            if cursor.is_some_and(|c| c.seq == entry.seq && c.link == i) {
                style = style.bg(t.bg_float).add_modifier(Modifier::BOLD);
            }
            patches.push((l.range.clone(), style));
        }
    }
    msg = restyle_spans(msg, &patches);
    if let Some(f) = find {
        let plain: String = msg.iter().map(|s| s.content.as_ref()).collect();
        let ranges = f.finder.ranges(&plain);
        let current = f
            .current
            .and_then(|(seq, idx)| (seq == entry.seq).then_some(idx));
        let hit = Style::default().bg(t.bg_float).fg(t.accent).bold();
        let cur = Style::default().bg(t.accent).fg(t.bg).bold();
        let patches: Vec<_> = ranges
            .into_iter()
            .enumerate()
            .map(|(i, r)| (r, if Some(i) == current { cur } else { hit }))
            .collect();
        msg = restyle_spans(msg, &patches);
    }
    spans.extend(msg);
    spans
}

/// The window of rows a render draws, over an index + cache pair.
pub struct Source<'a> {
    pub buf: &'a LogBuffer,
    pub index: &'a ViewIndex,
    pub rows: &'a RowCache,
    /// The view is one service's tab (entries are looked up in that ring).
    pub service: Option<&'a str>,
}

impl Source<'_> {
    fn entry(&self, i: usize) -> Option<&LogEntry> {
        let seq = self.index.seq_at(i)?;
        match self.service {
            Some(s) => self.buf.get(s, seq),
            None => self.buf.get_any(seq),
        }
    }
}

/// Everything `render` needs besides the frame.
pub struct LogPane<'a> {
    pub src: Source<'a>,
    /// Wrapped rows above the bottom (0 = newest visible).
    pub scroll: usize,
    pub opts: ViewOpts,
    pub find: Option<FindHighlight<'a>>,
    /// Link under the cursor (`o` opens it).
    pub cursor: Option<Cursor>,
    /// Tab name for the empty-state message.
    pub tab: &'a str,
    /// A find/filter query or level filter is narrowing the view.
    pub narrowed: bool,
    /// Lines that arrived since follow paused (chip shown when `scroll > 0`).
    pub new_lines: usize,
}

/// Columns available for text in a pane `area_width` wide (the wrap width).
pub fn text_width_for(area_width: u16) -> u16 {
    area_width.saturating_sub(PAD_LEFT + PAD_RIGHT + 1)
}

/// Render `pane` bottom-anchored. Long lines wrap (or truncate with
/// horizontal scroll when wrap is off); nothing else is clipped.
/// Cost is O(visible rows + log N): the height cache locates the window.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    pane: &LogPane<'_>,
    viewport: &Cell<LogViewport>,
    clicks: &ClickMap,
) {
    let t = theme::current();
    // The scrollbar column is always reserved so the wrap width never changes
    // when the bar appears — row counts stay stable while scrolling.
    let text_width = text_width_for(area.width);
    let height = area.height as usize;
    if text_width == 0 || height == 0 {
        viewport.set(LogViewport::default());
        return;
    }
    let opts = &pane.opts;
    let src = &pane.src;

    if src.index.is_empty() {
        viewport.set(LogViewport {
            text_width,
            height,
            ..Default::default()
        });
        clicks.push(area, Target::Wheel(Wheel::Logs));
        if pane.narrowed {
            let msg = format!("0 matches in {}", pane.tab);
            let w = (msg.chars().count() as u16).min(area.width);
            let x = area.x + area.width.saturating_sub(w) / 2;
            let y = area.y + area.height / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(msg, Style::default().fg(t.text_dim))),
                Rect::new(x, y, w, 1),
            );
        }
        return;
    }

    let total = src.rows.total();
    let max_scroll = total.saturating_sub(height);
    let scroll = pane.scroll.min(max_scroll);

    // Visible window [top, top + height) in wrapped-row space.
    let top = total - scroll - (total - scroll).min(height);
    let (first, skip) = src.rows.locate(top).unwrap_or((0, 0));

    let mut lines: Vec<Line> = Vec::with_capacity(skip + height);
    let mut widest = 0usize;
    let mut i = first;
    while i < src.rows.len() && lines.len() < skip + height {
        let Some(entry) = src.entry(i) else {
            i += 1;
            continue;
        };
        let spans = entry_spans(entry, opts, pane.find.as_ref(), pane.cursor.as_ref());
        if opts.wrap || entry.is_separator() {
            let prefix = if entry.is_separator() {
                0
            } else {
                Prefix::new(entry, opts).width()
            };
            lines.extend(wrap_spans(spans, text_width as usize, prefix));
        } else {
            let (line, w) = truncate_spans(spans, text_width as usize, opts.hscroll);
            widest = widest.max(w);
            lines.push(line);
        }
        i += 1;
    }
    let lines: Vec<Line> = lines.into_iter().skip(skip).take(height).collect();
    // Widest line in the rendered window bounds the pan (close enough, cheap).
    let max_hscroll = if opts.wrap {
        0
    } else {
        widest.saturating_sub(text_width as usize)
    };
    viewport.set(LogViewport {
        text_width,
        height,
        max_scroll,
        max_hscroll,
    });

    // Wheel anywhere over the log pane scrolls it (text clicks stay inert so ⌥-drag selects).
    clicks.push(area, Target::Wheel(Wheel::Logs));
    let inner = Rect {
        x: area.x + PAD_LEFT,
        y: area.y,
        width: text_width,
        height: height as u16,
    };
    frame.render_widget(Paragraph::new(lines), inner);

    // Paused chip on the last row: `↓ N new · G`, clickable → ScrollToBottom.
    if scroll > 0 {
        let label = if pane.new_lines > 0 {
            format!(" ↓ {} new · G ", pane.new_lines)
        } else {
            " ↓ G ".to_string()
        };
        let w = label.chars().count() as u16;
        if w <= inner.width {
            let rect = Rect::new(inner.x + inner.width - w, inner.y + inner.height - 1, w, 1);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    label,
                    Style::default().fg(t.bg).bg(t.accent).bold(),
                )),
                rect,
            );
            clicks.click(rect, Message::ScrollToBottom);
        }
    }

    // Scrollbar only when there is something to scroll (max_scroll > 0). The
    // track is blank: the sidebar separator is the only rule drawn, so there
    // is no `││` double line. The thumb sits in the reserved column.
    let track = area.height as usize;
    let thumb = (track * height)
        .checked_div(total)
        .unwrap_or(track)
        .clamp(1, track);
    if let Some(pos) = ((track - thumb) * top).checked_div(max_scroll) {
        let x = area.x + area.width - 1;
        clicks.push(
            Rect::new(x, area.y, 1, area.height),
            Target::Scrollbar(ScrollbarGeometry {
                track: track as u16,
                thumb: thumb as u16,
                max_scroll,
                y_origin: area.y,
            }),
        );
        let buf = frame.buffer_mut();
        for i in pos..(pos + thumb).min(track) {
            buf[(x, area.y + i as u16)]
                .set_symbol("┃")
                .set_fg(t.border_active);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::logs::LogStream;

    fn entry(content: &str) -> LogEntry {
        LogEntry {
            seq: 0,
            timestamp: 0,
            service: "api".into(),
            stream: LogStream::Stdout,
            content: content.into(),
            level: LogLevel::Info,
            time_str: "12:00:00".into(),
            mark: false,
            restart: false,
            truncated: false,
        }
    }

    /// A real buffer + index + height cache over `entries` (one service,
    /// seqs rewritten 0..n), so render tests go through the production path.
    struct Fixture {
        buf: LogBuffer,
        index: ViewIndex,
        rows: RowCache,
    }

    impl Fixture {
        fn new(entries: &[LogEntry]) -> Self {
            let mut buf = LogBuffer::new(100_000);
            for e in entries {
                if e.mark {
                    buf.push_mark("api".into());
                } else if e.restart {
                    buf.push_restart("api".into());
                } else {
                    buf.push("api".into(), e.content.clone(), e.stream);
                }
            }
            let key = crate::runtime::logindex::ViewKey {
                service: None,
                hidden: vec![],
                min_level: None,
                filter: None,
                find: None,
                json: false,
                window: Default::default(),
            };
            let mut index = ViewIndex::new();
            index.sync(&buf, &key);
            Self {
                buf,
                index,
                rows: RowCache::new(),
            }
        }

        fn pane<'a>(&'a mut self, width: u16, opts: ViewOpts) -> LogPane<'a> {
            self.rows.sync(
                &self.buf,
                None,
                self.index.seqs(0..self.index.len()),
                text_width_for(width),
                &opts,
            );
            LogPane {
                src: Source {
                    buf: &self.buf,
                    index: &self.index,
                    rows: &self.rows,
                    service: None,
                },
                scroll: 0,
                opts,
                find: None,
                cursor: None,
                tab: "All",
                narrowed: false,
                new_lines: 0,
            }
        }
    }

    fn draw(term: &mut Terminal<backend::TestBackend>, pane: &LogPane<'_>) -> ClickMap {
        let vp = Cell::new(LogViewport::default());
        let clicks = ClickMap::default();
        term.draw(|f| render(f, f.area(), pane, &vp, &clicks))
            .unwrap();
        clicks
    }

    fn rows_text(term: &Terminal<backend::TestBackend>) -> Vec<String> {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    /// Row count must equal the rows actually produced (hanging indent
    /// included), or the scroll math drifts and the newest lines get cut off.
    #[test]
    fn entry_rows_matches_wrap_spans() {
        let cases = [
            "short",
            "",
            "\x1b[31merror\x1b[0m with ansi and a fairly long tail of text to wrap around",
            "tabs\there\tand\tthere",
            "wide 日本語のテキストが折り返されるかどうか確認する",
            &"x".repeat(250),
            r#"{"level":30,"msg":"hello world","path":"/a/b/c","n":12345678}"#,
        ];
        let opt_sets = [
            ViewOpts::default(),
            ViewOpts {
                show_service: false,
                ..ViewOpts::default()
            },
            ViewOpts {
                timestamps: false,
                ..ViewOpts::default()
            },
            ViewOpts {
                json: true,
                ..ViewOpts::default()
            },
        ];
        for content in cases {
            let e = entry(content);
            for opts in &opt_sets {
                for w in [1u16, 7, 20, 33, 80] {
                    let prefix = Prefix::new(&e, opts).width();
                    let produced =
                        wrap_spans(entry_spans(&e, opts, None, None), w as usize, prefix);
                    assert_eq!(
                        entry_rows(&e, w, opts),
                        produced.len(),
                        "{content:?} @ {w} {opts:?}"
                    );
                    for line in &produced {
                        // A single glyph wider than the view (tab, CJK) may overflow alone.
                        assert!(
                            line.width() <= (w as usize).max(TAB_WIDTH),
                            "{content:?} @ {w}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn continuation_rows_hang_under_the_message_column() {
        let e = entry(&"x".repeat(60));
        let opts = ViewOpts::default();
        let prefix = Prefix::new(&e, &opts).width(); // "12:00:00 " + "[api] " = 15
        assert_eq!(prefix, 15);
        let lines = wrap_spans(entry_spans(&e, &opts, None, None), 30, prefix);
        assert!(lines.len() > 2);
        for line in &lines[1..] {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.starts_with(&" ".repeat(prefix)), "{text:?}");
            assert!(!text[prefix..].starts_with(' '));
        }
    }

    #[test]
    fn bottom_anchored_shows_newest_and_clamps_scroll() {
        let long = "y".repeat(100);
        let entries: Vec<LogEntry> = (0..20)
            .map(|i| entry(&format!("line{i} {long}")))
            .chain(std::iter::once(entry("NEWEST")))
            .collect();
        let mut fx = Fixture::new(&entries);
        let vp = Cell::new(LogViewport::default());
        let mut term = Terminal::new(backend::TestBackend::new(40, 10)).unwrap();

        let mut screen = |term: &mut Terminal<backend::TestBackend>, scroll| {
            let mut p = fx.pane(40, ViewOpts::default());
            p.scroll = scroll;
            term.draw(|f| render(f, f.area(), &p, &vp, &ClickMap::default()))
                .unwrap();
            format!("{:?}", term.backend().buffer())
        };

        assert!(screen(&mut term, 0).contains("NEWEST"));
        let max = vp.get().max_scroll;
        assert!(max > 0);
        // Over-scrolling clamps to the top instead of blanking the view.
        let top = screen(&mut term, usize::MAX);
        assert!(top.contains("line0"));
        assert!(!top.contains("NEWEST"));
        assert_eq!(top, screen(&mut term, max));
    }

    #[test]
    fn no_blank_top_row_and_single_service_drops_prefix() {
        let entries = [entry("first line"), entry("second line")];
        let mut term = Terminal::new(backend::TestBackend::new(40, 4)).unwrap();
        let opts = ViewOpts {
            show_service: false,
            ..ViewOpts::default()
        };
        draw(&mut term, &Fixture::new(&entries).pane(40, opts));
        let rows = rows_text(&term);
        // No blank top row: content starts on the very first row.
        assert!(rows[0].contains("first line"), "{rows:?}");
        assert!(rows[1].contains("second line"));
        assert!(!rows.iter().any(|r| r.contains("[api]")));

        // Full pane: first row carries content, not padding.
        let many: Vec<LogEntry> = (0..10).map(|i| entry(&format!("l{i}"))).collect();
        draw(&mut term, &Fixture::new(&many).pane(40, opts));
        assert!(rows_text(&term)[0].contains("l6"));
    }

    #[test]
    fn service_palette_avoids_warn_and_error_hues() {
        let t = theme::current();
        let close = |a: Color, b: Color| match (a, b) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
                d(r1, r2) + d(g1, g2) + d(b1, b2) < 90 * 90
            }
            _ => false,
        };
        for i in 0..200 {
            let c = service_color(&format!("svc-{i}"));
            assert!(!close(c, t.warning) && !close(c, t.error), "{c:?}");
            // Red-dominant colors read as errors; none allowed.
            if let Color::Rgb(r, g, b) = c {
                assert!(!(r > g && r > b), "{c:?} is warm");
            }
        }
    }

    #[test]
    fn smartcase_and_regex_finder() {
        let f = Finder::new("error", false).unwrap();
        assert!(f.is_match("Some ERROR here"), "lowercase ⇒ insensitive");
        let f = Finder::new("Error", false).unwrap();
        assert!(f.is_match("an Error"));
        assert!(!f.is_match("an error"), "uppercase ⇒ sensitive");
        // Literal mode escapes regex syntax.
        let f = Finder::new("a.c", false).unwrap();
        assert!(f.is_match("xa.cx") && !f.is_match("abc"));
        // Regex mode.
        let f = Finder::new(r"fail\w+ \d+", true).unwrap();
        assert_eq!(f.ranges("a failed 42 b failing 7"), vec![2..11, 14..23]);
        // Invalid regex / empty → no finder (caller shows an error state).
        assert!(Finder::new("(", true).is_none());
        assert!(Finder::new("", false).is_none());
    }

    #[test]
    fn find_highlights_all_matches_and_current_is_stronger() {
        let entries = [entry("foo bar foo"), entry("nothing"), entry("foo")];
        let finder = Finder::new("foo", false).unwrap();
        let mut term = Terminal::new(backend::TestBackend::new(60, 5)).unwrap();
        let mut fx = Fixture::new(&entries);
        let mut p = fx.pane(60, ViewOpts::default());
        p.find = Some(FindHighlight {
            finder: &finder,
            current: Some((0, 1)), // second "foo" of the first entry
        });
        draw(&mut term, &p);
        let t = theme::current();
        let buf = term.backend().buffer();
        let (mut hits, mut currents) = (0, 0);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let c = &buf[(x, y)];
                if c.bg == t.accent {
                    currents += 1;
                } else if c.bg == t.bg_float {
                    hits += 1;
                }
            }
        }
        // 3 non-current "foo"s ⇒ 2 occurrences × 3 cols; the current one is 3 cols.
        assert_eq!(currents, 3);
        assert_eq!(hits, 6);
    }

    #[test]
    fn empty_filtered_view_says_zero_matches() {
        let mut term = Terminal::new(backend::TestBackend::new(40, 5)).unwrap();
        let mut fx = Fixture::new(&[]);
        let mut p = fx.pane(40, ViewOpts::default());
        p.narrowed = true;
        p.tab = "web";
        draw(&mut term, &p);
        assert!(
            rows_text(&term)
                .iter()
                .any(|r| r.contains("0 matches in web"))
        );
        // Not narrowed: plain blank.
        p.narrowed = false;
        draw(&mut term, &p);
        assert!(!rows_text(&term).iter().any(|r| r.contains("matches")));
    }

    #[test]
    fn follow_chip_shows_new_count_and_is_clickable() {
        let long = "z".repeat(100);
        let entries: Vec<LogEntry> = (0..10).map(|_| entry(&long)).collect();
        let mut term = Terminal::new(backend::TestBackend::new(40, 8)).unwrap();
        let mut fx = Fixture::new(&entries);
        let mut p = fx.pane(40, ViewOpts::default());
        p.scroll = 3;
        p.new_lines = 7;
        let clicks = draw(&mut term, &p);
        let last = rows_text(&term).pop().unwrap();
        assert!(last.contains("↓ 7 new · G"), "{last:?}");
        let x = last.find('↓').map(|b| last[..b].chars().count()).unwrap() as u16;
        assert!(matches!(
            clicks.click_at(x, 7),
            Some(Message::ScrollToBottom)
        ));
        // Following: no chip.
        p.scroll = 0;
        draw(&mut term, &p);
        assert!(!rows_text(&term).iter().any(|r| r.contains("new · G")));
    }

    #[test]
    fn wrap_off_truncates_and_pans_horizontally() {
        let e = entry(&format!("{}{}", "a".repeat(30), "TAIL"));
        let entries = [e.clone()];
        let mut term = Terminal::new(backend::TestBackend::new(30, 3)).unwrap();
        let mut opts = ViewOpts {
            wrap: false,
            show_service: false,
            timestamps: false,
            ..ViewOpts::default()
        };
        assert_eq!(entry_rows(&e, 20, &opts), 1);
        draw(&mut term, &Fixture::new(&entries).pane(30, opts));
        let row = rows_text(&term)[0].clone();
        assert!(row.contains('→') && !row.contains("TAIL"), "{row:?}");
        // Pan right: tail appears, `← +N` chip appears, no more `→`.
        opts.hscroll = 12;
        let vp = Cell::new(LogViewport::default());
        let mut fx = Fixture::new(&entries);
        let p = fx.pane(30, opts);
        term.draw(|f| render(f, f.area(), &p, &vp, &ClickMap::default()))
            .unwrap();
        let rows = rows_text(&term);
        assert!(
            rows[0].contains("TAIL") && !rows[0].contains('→'),
            "{rows:?}"
        );
        assert!(
            !rows[0].contains('←'),
            "the offset lives in the status bar, not over text"
        );
        assert_eq!(vp.get().max_hscroll, 34 - vp.get().text_width as usize);
    }

    #[test]
    fn json_lines_compact_with_level() {
        let e = entry(r#"{"level":50,"time":1,"msg":"db down","host":"x","code":503}"#);
        let (text, level) = display_content(&e, true);
        assert_eq!(level, LogLevel::Error);
        assert!(text.starts_with("ERROR db down"), "{text}");
        assert!(text.contains("host=x") && text.contains("code=503"));
        assert!(!text.contains("time"), "noise keys dropped: {text}");
        // pino numeric levels and string levels.
        let lvl = |c: &str| display_content(&entry(c), true).1;
        assert_eq!(lvl(r#"{"level":30,"msg":"a"}"#), LogLevel::Info);
        assert_eq!(lvl(r#"{"level":40,"msg":"a"}"#), LogLevel::Warn);
        assert_eq!(lvl(r#"{"level":"warn","msg":"a"}"#), LogLevel::Warn);
        assert_eq!(lvl(r#"{"level":"fatal","msg":"a"}"#), LogLevel::Error);
        // Toggle off ⇒ raw; non-JSON untouched.
        assert_eq!(display_content(&e, false).0, e.content);
        let plain = entry("not { json }");
        assert_eq!(display_content(&plain, true).0, "not { json }");
    }

    #[test]
    fn restart_separator_is_one_row_and_never_searched() {
        let mut e = entry("──── 12:00:00 restarted ────");
        e.restart = true;
        assert_eq!(entry_rows(&e, 5, &ViewOpts::default()), 1);
        assert!(entry_links(&e, false).is_empty());
        let mut term = Terminal::new(backend::TestBackend::new(60, 4)).unwrap();
        let entries = [entry("before"), e, entry("after")];
        draw(
            &mut term,
            &Fixture::new(&entries).pane(60, ViewOpts::default()),
        );
        let rows = rows_text(&term).join("\n");
        assert!(rows.contains("before") && rows.contains("restarted") && rows.contains("after"));
    }

    #[test]
    fn links_cover_urls_paths_and_osc8_and_are_underlined() {
        let e = entry(
            "fail at src/a.rs:12:3 see https://x.test/p, \x1b]8;;http://osc.test\x1b\\here\x1b]8;;\x1b\\",
        );
        let links = entry_links(&e, false);
        let targets: Vec<_> = links.iter().map(|l| l.target.clone()).collect();
        use crate::runtime::loglinks::Target;
        assert_eq!(
            targets,
            vec![
                Target::Path {
                    path: "src/a.rs".into(),
                    line: 12,
                    col: Some(3)
                },
                Target::Url("https://x.test/p".into()),
                Target::Url("http://osc.test".into()),
            ]
        );
        // Drawn underlined; the cursor link gets the stronger background.
        let spans = entry_spans(&e, &ViewOpts::default(), None, None);
        assert!(spans.iter().any(|s| {
            s.content.contains("src/a.rs:12:3")
                && s.style.add_modifier.contains(Modifier::UNDERLINED)
        }));
        let cur = Cursor {
            seq: e.seq,
            link: 1,
        };
        let spans = entry_spans(&e, &ViewOpts::default(), None, Some(&cur));
        let t = theme::current();
        assert!(
            spans
                .iter()
                .any(|s| s.content.contains("https://x.test/p") && s.style.bg == Some(t.bg_float))
        );
        assert!(
            !spans
                .iter()
                .any(|s| s.content.contains("src/a.rs") && s.style.bg == Some(t.bg_float))
        );
    }

    #[test]
    fn plain_lines_without_links_skip_the_scan() {
        assert!(entry_links(&entry("nothing to see here, 12:30:45 ok"), false).is_empty());
    }

    #[test]
    fn render_cost_is_bounded_by_the_window_not_the_buffer() {
        // 200k lines; drawing a 10-row window must not touch them all.
        let mut buf = LogBuffer::new(1_000_000);
        for i in 0..200_000 {
            buf.push(
                "api".into(),
                format!("line {i} some text to wrap around"),
                LogStream::Stdout,
            );
        }
        let key = crate::runtime::logindex::ViewKey {
            service: None,
            hidden: vec![],
            min_level: None,
            filter: None,
            find: None,
            json: false,
            window: Default::default(),
        };
        let mut index = ViewIndex::new();
        index.sync(&buf, &key);
        let opts = ViewOpts::default();
        let mut rows = RowCache::new();
        rows.sync(
            &buf,
            None,
            index.seqs(0..index.len()),
            text_width_for(60),
            &opts,
        );
        let p = LogPane {
            src: Source {
                buf: &buf,
                index: &index,
                rows: &rows,
                service: None,
            },
            scroll: 100_000,
            opts,
            find: None,
            cursor: None,
            tab: "All",
            narrowed: false,
            new_lines: 0,
        };
        let mut term = Terminal::new(backend::TestBackend::new(60, 10)).unwrap();
        let t0 = std::time::Instant::now();
        for _ in 0..50 {
            draw(&mut term, &p);
        }
        let per_frame = t0.elapsed() / 50;
        assert!(
            per_frame < std::time::Duration::from_millis(20),
            "{per_frame:?}"
        );
        assert!(rows_text(&term).iter().any(|r| r.contains("line ")));
    }

    #[test]
    fn mark_renders_as_one_dim_row() {
        let mut e = entry("──── 12:00:00 mark ────");
        e.mark = true;
        assert_eq!(entry_rows(&e, 5, &ViewOpts::default()), 1);
    }
}
