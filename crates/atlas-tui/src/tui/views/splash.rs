use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
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

/// Spans for one splash service row.
fn service_row(name: &str, state: &ServiceState, tick: usize) -> Line<'static> {
    let t = theme::current();
    let (icon, color) = match state {
        ServiceState::Running => ("✓", t.success),
        ServiceState::Failed => ("✗", t.error),
        ServiceState::Starting => (spinner::frame(tick), t.warning),
        ServiceState::Stopped => ("○", t.text_dim),
        _ => ("·", t.text_dim),
    };
    Line::from(vec![
        Span::styled(format!("{icon} "), Style::default().fg(color).bold()),
        Span::styled(name.to_string(), Style::default().fg(t.text_muted)),
    ])
}

pub fn render(frame: &mut Frame, app: &App) {
    let t = theme::current();
    let area = frame.area();

    frame.render_widget(Block::default().style(Style::default().bg(t.bg)), area);

    let tick = (app.splash_start.elapsed().as_millis() / 80) as usize;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Logo — centred by Paragraph alignment
    for i in 0..LOGO_LEFT.len() {
        lines.push(Line::from(vec![
            Span::styled(LOGO_LEFT[i], Style::default().fg(t.primary).bold()),
            Span::styled(LOGO_RIGHT[i], Style::default().fg(t.secondary).bold()),
        ]));
    }

    lines.push(Line::from(""));
    let sub = if app.config.name.is_empty() {
        vec![Span::styled(
            "development environment",
            Style::default().fg(t.text_dim),
        )]
    } else {
        let (name, rest) = subtitle_split(&app.config.name);
        vec![
            Span::styled(name, Style::default().fg(t.text).bold()),
            Span::styled(rest, Style::default().fg(t.text_dim)),
        ]
    };
    lines.push(Line::from(sub));
    lines.push(Line::from(""));

    let header_h = lines.len() as u16;
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);

    // Per-service live status: left-aligned inside a block that is itself centred,
    // so the icons line up in one column.
    let services = app.manager.services();
    let rows: Vec<Line> = if services.is_empty() {
        vec![Line::from(vec![
            Span::styled(spinner::frame(tick), Style::default().fg(t.primary)),
            Span::styled(" starting...", Style::default().fg(t.text_muted)),
        ])]
    } else {
        services
            .iter()
            .map(|s| service_row(&s.name, &s.state, tick))
            .collect()
    };
    let block_w = rows
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(0)
        .min(area.width);
    let x = area.x + area.width.saturating_sub(block_w) / 2;
    let y = area.y + header_h;
    if y < area.bottom() {
        let h = (rows.len() as u16).min(area.bottom() - y);
        frame.render_widget(Paragraph::new(rows), Rect::new(x, y, block_w, h));
    }
}

/// (`name`, ` · development environment`) so the project name can be emphasized.
fn subtitle_split(name: &str) -> (String, String) {
    (name.to_string(), " · development environment".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_left_aligned_with_icon_first() {
        let l = service_row("api", &ServiceState::Running, 0);
        assert_eq!(l.spans[0].content.as_ref(), "✓ ");
        assert_eq!(l.spans[1].content.as_ref(), "api");
    }

    #[test]
    fn subtitle_carries_project_name() {
        let (a, b) = subtitle_split("uxaudit");
        assert_eq!(a, "uxaudit");
        assert!(b.contains("development environment"));
    }
}
