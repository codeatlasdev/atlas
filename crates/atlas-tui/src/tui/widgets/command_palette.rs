use std::cell::Cell;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::{Message, PaletteCommand};
use crate::tui::clicks::{ClickMap, Target, Wheel};

/// Maximum list rows the palette shows at once.
pub const MAX_ROWS: u16 = 12;

/// Rows available for the list given the terminal height (input + separator +
/// blank + description + 2 border lines are fixed; keep a 2-row margin).
pub fn list_rows(area_height: u16, commands: usize) -> usize {
    // border(2) + input + separator + blank + description = 6 fixed, 2-row screen margin
    let room = area_height.saturating_sub(4 + 6);
    (commands as u16).min(MAX_ROWS).min(room).max(1) as usize
}

/// Split `name` into spans, bold-accenting the chars at `matched`.
fn name_spans(name: &str, matched: &[usize], base: Style, width: usize) -> Vec<Span<'static>> {
    let hit = base.fg(theme::current().accent).bold();
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut buf_hit = false;
    let mut used = 0;
    for (i, c) in name.chars().enumerate() {
        if used >= width {
            break;
        }
        let is_hit = matched.contains(&i);
        if is_hit != buf_hit && !buf.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut buf),
                if buf_hit { hit } else { base },
            ));
        }
        buf_hit = is_hit;
        buf.push(c);
        used += 1;
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, if buf_hit { hit } else { base }));
    }
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), base));
    }
    spans
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    input: &str,
    commands: &[PaletteCommand],
    selected: usize,
    offset: usize,
    rows_out: &Cell<usize>,
    clicks: &ClickMap,
) {
    let t = theme::current();
    let area = frame.area();

    super::dim::backdrop(frame, area);

    let width = 58.min(area.width.saturating_sub(4));
    let rows = list_rows(area.height, commands.len());
    rows_out.set(rows);
    let has_desc = commands
        .get(selected)
        .is_some_and(|c| !c.description.is_empty());
    // border(2) + input + separator + list + (blank + description)
    let height = (rows as u16 + 4 + if has_desc { 2 } else { 0 }).min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let palette_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, palette_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.primary))
        .style(Style::default().bg(t.bg_panel));

    let inner = block.inner(palette_area);
    frame.render_widget(block, palette_area);

    // Backdrop closes (Esc); the body swallows clicks but takes the wheel; rows sit on top.
    clicks.click(area, Message::HidePalette);
    clicks.push(palette_area, Target::Wheel(Wheel::Palette));

    let total = commands.len();
    let offset = offset.min(total.saturating_sub(rows));
    let visible = &commands[offset..(offset + rows).min(total)];

    // Command rows start after the input and separator lines; a click selects + runs (Enter).
    // Indices are absolute (offset + row) so they stay in sync with the scroll window.
    for i in 0..visible.len() {
        let ry = inner.y + 2 + i as u16;
        if ry >= inner.bottom() {
            break;
        }
        clicks.click(
            Rect::new(inner.x, ry, inner.width, 1),
            Message::PaletteClick(offset + i),
        );
    }

    let mut lines = Vec::new();

    // Input line
    lines.push(Line::from(vec![
        Span::styled(": ", Style::default().fg(t.primary).bold()),
        Span::styled(input.to_string(), Style::default().fg(t.text)),
    ]));

    // Separator carries `↑ n/total ↓` so hidden rows are never a surprise.
    let up = if offset > 0 { "↑" } else { " " };
    let down = if offset + rows < total { "↓" } else { " " };
    let counter = if total == 0 {
        " no match ".to_string()
    } else {
        format!(" {up} {}/{total} {down} ", selected + 1)
    };
    let rule_w = (inner.width as usize).saturating_sub(counter.chars().count());
    lines.push(Line::from(vec![
        Span::styled("─".repeat(rule_w), Style::default().fg(t.border_subtle)),
        Span::styled(counter, Style::default().fg(t.text_dim)),
    ]));

    // Commands — highlight selected row; key hint right-aligned.
    let key_w = 6usize;
    let row_w = inner.width as usize;
    for (i, cmd) in visible.iter().enumerate() {
        let abs = offset + i;
        let is_selected = abs == selected;
        let base = if is_selected {
            Style::default().fg(t.text).bg(t.bg_float)
        } else {
            Style::default().fg(t.text_muted)
        };
        let prefix = match cmd.group.as_deref() {
            Some(g) => format!(" {g} › "),
            None => " ".to_string(),
        };
        let name_w = row_w.saturating_sub(prefix.chars().count() + key_w);
        let shortcut = cmd.shortcut.as_deref().unwrap_or("");
        let mut spans = vec![Span::styled(prefix, base.fg(t.text_dim))];
        spans.extend(name_spans(&cmd.name, &cmd.matched, base, name_w));
        spans.push(Span::styled(
            format!("{shortcut:>w$} ", w = key_w - 1),
            base.fg(t.text_dim),
        ));
        lines.push(Line::from(spans));
    }

    // Description bar: selected command's description at the bottom
    if let Some(cmd) = commands.get(selected)
        && !cmd.description.is_empty()
    {
        lines.push(Line::from(""));
        let max = row_w.saturating_sub(2);
        let d: String = cmd.description.chars().take(max).collect();
        lines.push(Line::from(Span::styled(
            format!("  {d}"),
            Style::default().fg(t.text_dim),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);

    // Real terminal cursor at input insertion point
    let cursor_x = inner.x + 2 + input.chars().count() as u16; // ": " prefix = 2 chars
    if cursor_x < inner.x + inner.width {
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Message;
    use ratatui::{Terminal, backend::TestBackend};

    fn cmd(name: &str, key: &str) -> PaletteCommand {
        PaletteCommand {
            id: name.into(),
            name: name.into(),
            description: format!("desc of {name}"),
            shortcut: (!key.is_empty()).then(|| key.to_string()),
            action: Message::ShowHelp,
            group: None,
            matched: Vec::new(),
        }
    }

    fn draw(
        w: u16,
        h: u16,
        input: &str,
        cmds: &[PaletteCommand],
        sel: usize,
        off: usize,
    ) -> (Vec<String>, usize, ClickMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let rows = Cell::new(0);
        let clicks = ClickMap::default();
        term.draw(|f| render(f, input, cmds, sel, off, &rows, &clicks))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let lines = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        (lines, rows.get(), clicks)
    }

    fn many(n: usize) -> Vec<PaletteCommand> {
        (0..n)
            .map(|i| cmd(&format!("Command {i:02}"), ""))
            .collect()
    }

    #[test]
    fn shows_counter_and_down_marker_when_clipped() {
        let cmds = many(40);
        let (lines, rows, _) = draw(100, 40, "", &cmds, 0, 0);
        let all = lines.join("\n");
        assert!(all.contains("1/40"), "{all}");
        assert!(all.contains('↓') && !all.contains('↑'), "{all}");
        assert_eq!(rows, MAX_ROWS as usize);
    }

    #[test]
    fn scrolled_window_shows_selected_and_up_marker() {
        let cmds = many(40);
        let (lines, rows, _) = draw(100, 40, "", &cmds, 25, 20);
        let all = lines.join("\n");
        assert!(all.contains("26/40"), "{all}");
        assert!(all.contains("Command 25"), "selected row must be visible");
        assert!(all.contains('↑'));
        assert!(!all.contains("Command 00"));
        assert!(rows >= 1);
    }

    #[test]
    fn click_indices_are_absolute_under_scroll() {
        let cmds = many(40);
        let (_, _, clicks) = draw(100, 40, "", &cmds, 25, 20);
        let found: Vec<usize> = clicks
            .targets
            .borrow()
            .iter()
            .filter_map(|(_, t)| match t {
                Target::Click(Message::PaletteClick(i)) => Some(*i),
                _ => None,
            })
            .collect();
        assert_eq!(found.first(), Some(&20));
        assert!(found.contains(&25));
    }

    #[test]
    fn shows_shortcut_right_aligned_and_highlights_match() {
        let mut c = cmd("Restart All", "r");
        c.matched = vec![0, 1];
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let rows = Cell::new(0);
        term.draw(|f| render(f, "re", &[c], 0, 0, &rows, &ClickMap::default()))
            .unwrap();
        let buf = term.backend().buffer();
        let mut found_hit = false;
        let mut row_text = String::new();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            if line.contains("Restart All") && !line.contains("desc of") {
                row_text = line;
                for x in 0..buf.area.width {
                    if buf[(x, y)].symbol() == "R" && buf[(x, y)].fg == theme::current().accent {
                        found_hit = true;
                    }
                }
            }
        }
        assert!(found_hit, "matched chars use the accent color");
        // Inside the modal: text up to the right border ends with the key hint.
        let inner: String = row_text
            .chars()
            .skip_while(|c| *c != '│')
            .skip(1)
            .take_while(|c| *c != '│')
            .collect();
        assert!(inner.trim_end().ends_with('r'), "{row_text:?}");
    }

    #[test]
    fn no_match_and_tiny_screens_do_not_panic() {
        let (lines, _, _) = draw(80, 24, "zzz", &[], 0, 0);
        assert!(lines.join("\n").contains("no match"));
        draw(60, 12, "", &many(5), 0, 0);
    }
}
