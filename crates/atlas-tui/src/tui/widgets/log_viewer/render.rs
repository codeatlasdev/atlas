//! Drawing the pane: bottom-anchored window over the index + height cache,
//! scrollbar, follow chip and click targets.

use super::*;
use atlas_i18n::t;

/// The window of rows a render draws, over an index + cache pair.
pub struct Source<'a> {
    pub buf: &'a LogBuffer,
    pub index: &'a ViewIndex,
    pub rows: &'a RowCache,
    /// The view is one service's tab (entries are looked up in that ring).
    pub service: Option<&'a str>,
}

impl Source<'_> {
    pub(super) fn entry(&self, i: usize) -> Option<&LogEntry> {
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
            let msg = t!("tui_runtime_widgets_log_no_matches", tab = pane.tab);
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
            format!(
                " {} ",
                t!(
                    "tui_runtime_widgets_log_new_lines",
                    count = pane.new_lines,
                    key = "G"
                )
            )
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
