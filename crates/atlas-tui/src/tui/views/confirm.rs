use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::ClickMap;
use crate::tui::widgets::modal::Modal;

/// Generic yes/no modal. Same chrome as the quit modal; body wraps to the modal width.
pub fn render(frame: &mut Frame, title: &str, body: &str, clicks: &ClickMap) {
    let t = theme::current();
    let area = frame.area();

    let width = 56.min(area.width.saturating_sub(4));
    // Inner text width (borders eat 2); estimate wrapped rows to size the modal.
    let text_w = width.saturating_sub(4).max(1) as usize;
    let body_rows: u16 = body
        .lines()
        .map(|l| l.chars().count().div_ceil(text_w).max(1) as u16)
        .sum::<u16>()
        .max(1);
    // borders(2) + blank + body + blank + buttons + blank
    let height = (body_rows + 6).min(area.height.saturating_sub(2));
    let (_, inner) = Modal::new(&format!(" {title} "), Message::ConfirmCancel)
        .frame(frame, width, height, clicks);

    let mut lines = vec![Line::from("")];
    for l in body.lines() {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(t.text_muted),
        )));
    }
    lines.push(Line::from(""));
    let spans = [
        Span::styled(" Esc ", Style::default().fg(t.text).bg(t.bg_float).bold()),
        Span::styled(" cancel", Style::default().fg(t.text_dim)),
        Span::raw("    "),
        Span::styled(" Enter ", Style::default().fg(t.text).bg(t.error).bold()),
        Span::styled(" confirm", Style::default().fg(t.text_dim)),
    ];
    let w = |i: usize| spans[i].width() as u16;
    let (esc_w, enter_x_off, enter_w) = (w(0) + w(1), w(0) + w(1) + w(2), w(3) + w(4));
    let buttons = Line::from(spans.to_vec());
    // Rendered separately from the wrapped body so its row/columns are exact.
    let buttons_y = inner.y + inner.height.saturating_sub(2);
    let row_w = buttons.width() as u16;
    let bx = inner.x + inner.width.saturating_sub(row_w) / 2;
    clicks.click(
        Rect::new(bx, buttons_y, esc_w, 1).intersection(inner),
        Message::ConfirmCancel,
    );
    clicks.click(
        Rect::new(bx + enter_x_off, buttons_y, enter_w, 1).intersection(inner),
        Message::ConfirmAccept,
    );

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        Rect::new(inner.x, buttons_y, inner.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_confirm_modal_renders_long_body() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let body = "This wipes the local database and reseeds it. ".repeat(6);
        terminal
            .draw(|frame| render(frame, "Refresh DB", &body, &ClickMap::default()))
            .unwrap();
    }
}
