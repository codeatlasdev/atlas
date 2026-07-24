use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::{ServiceState, ServiceStatus};
use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, services: &[&ServiceStatus]) {
    let t = theme::current();
    let mut text = Vec::new();

    // Header
    let running = services
        .iter()
        .filter(|s| s.state == ServiceState::Running)
        .count();
    let total = services.len();

    if running == total {
        text.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(t.status_running).bold()),
            Span::styled(
                "All systems operational",
                Style::default().fg(t.text),
            ),
        ]));
    } else {
        text.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(t.warning).bold()),
            Span::styled(
                format!("{running}/{total} services running"),
                Style::default().fg(t.text),
            ),
        ]));
    }
    text.push(Line::from(""));

    // Service rows
    for svc in services {
        let (icon, color) = match svc.state {
            ServiceState::Running => ("●", t.status_running),
            ServiceState::Starting => ("◌", t.status_starting),
            ServiceState::Failed => ("✗", t.status_failed),
            ServiceState::Stopped => ("○", t.status_stopped),
        };
        let port_str = svc
            .port
            .map(|p| format!(":{p}"))
            .unwrap_or_else(|| "—".to_string());

        text.push(Line::from(vec![
            Span::styled(format!("  {icon} "), Style::default().fg(color)),
            Span::styled(format!("{:<12}", svc.name), Style::default().fg(t.text)),
            Span::styled(port_str, Style::default().fg(t.text_dim)),
        ]));
    }

    let paragraph = Paragraph::new(text).block(Block::default());
    frame.render_widget(paragraph, area);
}
