use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::ToastVariant;

use super::spinner;

pub fn render(frame: &mut Frame, msg: &str, variant: &ToastVariant, tick: usize) {
    let t = theme::current();
    let (prefix, color) = match variant {
        ToastVariant::Info => (spinner::frame(tick), t.primary),
        ToastVariant::Success => ("✓", t.success),
        ToastVariant::Error => ("✗", t.error),
    };

    let area = frame.area();
    let content = format!("{prefix} {msg}");
    let width = (content.len() + 4).min(44) as u16;
    let x = area.width.saturating_sub(width + 2);
    let toast_area = Rect::new(x, 1, width, 3);

    frame.render_widget(Clear, toast_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .style(Style::default().bg(t.bg_panel));

    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    let text = Line::from(vec![
        Span::styled(prefix, Style::default().fg(color)),
        Span::styled(format!(" {msg}"), Style::default().fg(t.text)),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}
