use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;

pub fn render(frame: &mut Frame) {
    let t = theme::current();
    let area = frame.area();

    // Centered modal
    let width = 48.min(area.width.saturating_sub(4));
    let height = 18.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    let block = Block::default()
        .title(" Shortcuts ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg_panel));

    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("GENERAL", Style::default().fg(t.text_dim))),
        Line::from(vec![
            Span::styled(" q    ", Style::default().fg(t.primary).bold()),
            Span::styled("Quit", Style::default().fg(t.text)),
            Span::raw("          "),
            Span::styled("?    ", Style::default().fg(t.primary).bold()),
            Span::styled("Help", Style::default().fg(t.text)),
        ]),
        Line::from(""),
        Line::from(Span::styled("SERVICES", Style::default().fg(t.text_dim))),
        Line::from(vec![
            Span::styled(" r    ", Style::default().fg(t.primary).bold()),
            Span::styled("Restart all", Style::default().fg(t.text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "NAVIGATION",
            Style::default().fg(t.text_dim),
        )),
        Line::from(vec![
            Span::styled(" tab  ", Style::default().fg(t.primary).bold()),
            Span::styled("Next tab", Style::default().fg(t.text)),
            Span::raw("     "),
            Span::styled("0-6  ", Style::default().fg(t.primary).bold()),
            Span::styled("Jump", Style::default().fg(t.text)),
        ]),
        Line::from(vec![
            Span::styled(" j/k  ", Style::default().fg(t.primary).bold()),
            Span::styled("Scroll", Style::default().fg(t.text)),
            Span::raw("       "),
            Span::styled("G/g  ", Style::default().fg(t.primary).bold()),
            Span::styled("Bottom/Top", Style::default().fg(t.text)),
        ]),
        Line::from(""),
        Line::from(Span::styled("LOGS", Style::default().fg(t.text_dim))),
        Line::from(vec![
            Span::styled(" L    ", Style::default().fg(t.primary).bold()),
            Span::styled("Copy logs", Style::default().fg(t.text)),
            Span::raw("      "),
            Span::styled("P    ", Style::default().fg(t.primary).bold()),
            Span::styled("AI prompt", Style::default().fg(t.text)),
        ]),
        Line::from(vec![
            Span::styled(" E    ", Style::default().fg(t.primary).bold()),
            Span::styled("Copy errors", Style::default().fg(t.text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " esc / ? to close",
            Style::default().fg(t.text_dim),
        )),
    ];

    let paragraph =
        Paragraph::new(lines).block(Block::default().padding(Padding::new(1, 1, 0, 0)));
    frame.render_widget(paragraph, inner);
}
