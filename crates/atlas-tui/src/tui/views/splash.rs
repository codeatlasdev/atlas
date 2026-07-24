use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::App;
use crate::tui::widgets::spinner;

const LOGO_LEFT: &[&str] = &[
    " ██████╗ ██████╗ ██████╗ ███████╗",
    "██╔════╝██╔═══██╗██╔══██╗██╔════╝",
    "██║     ██║   ██║██║  ██║█████╗  ",
    "██║     ██║   ██║██║  ██║██╔══╝  ",
    "╚██████╗╚██████╔╝██████╔╝███████╗",
    " ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
];

const LOGO_RIGHT: &[&str] = &[
    " █████╗ ████████╗██╗      █████╗ ███████╗",
    "██╔══██╗╚══██╔══╝██║     ██╔══██╗██╔════╝",
    "███████║   ██║   ██║     ███████║███████╗",
    "██╔══██║   ██║   ██║     ██╔══██║╚════██║",
    "██║  ██║   ██║   ███████╗██║  ██║███████║",
    "╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚══════╝",
];

pub fn render(frame: &mut Frame, app: &App) {
    let t = theme::current();
    let area = frame.area();

    // Clear background
    frame.render_widget(Block::default().style(Style::default().bg(t.bg)), area);

    let tick = (app.splash_start.elapsed().as_millis() / 80) as usize;

    let mut lines = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Logo
    for i in 0..LOGO_LEFT.len() {
        lines.push(Line::from(vec![
            Span::styled(LOGO_LEFT[i], Style::default().fg(t.primary).bold()),
            Span::styled(LOGO_RIGHT[i], Style::default().fg(t.secondary).bold()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "development environment",
        Style::default().fg(t.text_dim),
    )));
    lines.push(Line::from(""));

    // Loading indicator
    let spin = spinner::frame(tick);
    lines.push(Line::from(vec![
        Span::styled(spin, Style::default().fg(t.primary)),
        Span::styled(" starting services...", Style::default().fg(t.text_muted)),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}
