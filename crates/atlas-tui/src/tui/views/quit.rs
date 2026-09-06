use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;

pub fn render(frame: &mut Frame, selected: usize) {
    let t = theme::current();
    let area = frame.area();

    let width = 44.min(area.width.saturating_sub(4));
    let height = 7.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    let block = Block::default()
        .title(" Quit? ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg_panel));

    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    let cancel_style = if selected == 0 {
        Style::default().fg(t.text).bg(t.bg_float).bold()
    } else {
        Style::default().fg(t.text_dim)
    };
    let quit_style = if selected == 1 {
        Style::default().fg(t.text).bg(t.error).bold()
    } else {
        Style::default().fg(t.text_dim)
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "All services will be stopped.",
            Style::default().fg(t.text_muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("      "),
            Span::styled(" Cancel ", cancel_style),
            Span::raw("   "),
            Span::styled(" Quit ", quit_style),
        ]),
    ];

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_quit_modal_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, 0)).unwrap();
    }

    #[test]
    fn test_quit_modal_selected_quit() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, 1)).unwrap();
    }
}
