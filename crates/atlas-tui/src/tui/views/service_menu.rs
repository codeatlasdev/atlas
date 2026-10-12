use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
use crate::theme;
use crate::tui::app::{Message, ServiceMenuAction};
use crate::tui::clicks::ClickMap;
use crate::tui::widgets::modal::Modal;

/// Menu actions that make sense for `state`, in display order.
/// Shared with the key handler so hidden entries can't be triggered.
pub fn visible_actions(state: &ServiceState) -> &'static [ServiceMenuAction] {
    match state {
        ServiceState::Running | ServiceState::Starting => {
            &[ServiceMenuAction::Restart, ServiceMenuAction::Stop]
        }
        // Start and Restart both dispatch RestartService, so one entry suffices.
        ServiceState::Failed => &[ServiceMenuAction::Restart],
        ServiceState::Stopped | ServiceState::Disabled | ServiceState::Suspended => {
            &[ServiceMenuAction::Start]
        }
    }
}

/// Render the service context menu overlay for `service_name`.
pub fn render(frame: &mut Frame, service_name: &str, state: &ServiceState, clicks: &ClickMap) {
    let t = theme::current();
    let area = frame.area();

    let width = 36_u16.min(area.width.saturating_sub(4));
    let height = 11_u16.min(area.height.saturating_sub(4));
    let (_, inner) = Modal::new(&format!(" {service_name} "), Message::HideServiceMenu)
        .frame(frame, width, height, clicks);

    let (icon, icon_color) = match state {
        ServiceState::Running => ("●", t.status_running),
        ServiceState::Starting => ("◎", t.status_starting),
        ServiceState::Failed => ("✗", t.status_failed),
        ServiceState::Stopped => ("○", t.status_stopped),
        ServiceState::Disabled => ("⊘", t.text_muted),
        ServiceState::Suspended => ("⏸", t.text_dim),
    };
    let state_label = format!("{state:?}");

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {icon} "), Style::default().fg(icon_color).bold()),
            Span::styled(state_label, Style::default().fg(t.text_muted)),
        ]),
        Line::from(""),
    ];
    for (i, action) in visible_actions(state).iter().enumerate() {
        // Items start on line 2 (state line + blank), inside the 1-col side padding.
        let row = Rect::new(
            inner.x + 1,
            inner.y + 2 + i as u16,
            inner.width.saturating_sub(2),
            1,
        )
        .intersection(inner);
        clicks.click(
            row,
            Message::ServiceMenuAction {
                name: service_name.to_string(),
                action: action.clone(),
            },
        );
        let (key, label) = match action {
            ServiceMenuAction::Restart => (" r  ", "Restart"),
            ServiceMenuAction::Stop => (" s  ", "Stop"),
            ServiceMenuAction::Start => (" t  ", "Start"),
        };
        lines.push(Line::from(vec![
            Span::styled(key, Style::default().fg(t.primary).bold()),
            Span::styled(label, Style::default().fg(t.text)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " esc to cancel",
        Style::default().fg(t.text_dim),
    )));

    let paragraph = Paragraph::new(lines).block(Block::default().padding(Padding::new(1, 1, 0, 0)));
    frame.render_widget(paragraph, inner);
}
