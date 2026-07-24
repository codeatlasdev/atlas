use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
use crate::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tabs: &[String],
    active: usize,
    states: &[(String, ServiceState)],
) {
    let t = theme::current();
    let state_map: HashMap<&str, &ServiceState> =
        states.iter().map(|(n, s)| (n.as_str(), s)).collect();

    let mut spans = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        let style = if i == active {
            Style::default().fg(t.text).bold()
        } else {
            Style::default().fg(t.text_dim)
        };
        spans.push(Span::styled(format!(" {tab} "), style));

        // Status dot (skip "All" tab)
        if i > 0 {
            if let Some(state) = state_map.get(tab.as_str()) {
                let (dot, color) = match state {
                    ServiceState::Running => ("●", t.status_running),
                    ServiceState::Starting => ("◌", t.status_starting),
                    ServiceState::Failed => ("✗", t.status_failed),
                    ServiceState::Stopped => ("○", t.status_stopped),
                };
                spans.push(Span::styled(dot, Style::default().fg(color)));
            }
        }
        spans.push(Span::raw(" "));
    }

    let line = Line::from(spans);
    let underline = Line::from(Span::styled(
        "━".repeat(area.width as usize),
        Style::default().fg(if active < tabs.len() {
            t.primary
        } else {
            t.border
        }),
    ));

    let paragraph =
        Paragraph::new(vec![line, underline]).block(Block::default().padding(Padding::left(1)));
    frame.render_widget(paragraph, area);
}
