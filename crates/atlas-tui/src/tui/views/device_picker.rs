use ratatui::prelude::*;
use ratatui::widgets::*;

use super::super::widgets::spinner;
use crate::config::SessionPlatform;
use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::{ClickMap, Target, Wheel};
use crate::tui::widgets::modal::Modal;
use atlas_i18n::t;

/// Render the device picker modal overlay.
/// `devices` is a slice of `(id, display_name)` pairs.
/// `selected` is the currently highlighted index.
/// `detecting` is true while adb/xcrun is still running.
pub fn render(
    frame: &mut Frame,
    platform: &SessionPlatform,
    devices: &[(String, String)],
    selected: usize,
    detecting: bool,
    tick: usize,
    clicks: &ClickMap,
) {
    let t = theme::current();
    let area = frame.area();

    let width = 56_u16.min(area.width.saturating_sub(4));
    let list_rows = devices.len().max(1) as u16;
    // header(1) + items + footer(2) + padding(2) + border(2)
    let height = (7 + list_rows).min(area.height.saturating_sub(4));
    let (platform_label, platform_icon) = match platform {
        SessionPlatform::Ios => (t!("tui_runtime_views_devices_title_ios"), ""),
        SessionPlatform::Android => (t!("tui_runtime_views_devices_title_android"), ""),
    };
    let title = format!(" {platform_icon} {platform_label} ");

    // The body swallows clicks but takes the wheel; rows sit on top.
    let (_, inner) = Modal {
        border: t.border_active,
        body: Target::Wheel(Wheel::Picker),
        ..Modal::new(&title, Message::DevicePickerCancel)
    }
    .frame(frame, width, height, clicks);

    // Split inner: header(1) | list(min) | footer(2)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    // ── Header row: status line ─────────────────────────────────────────
    let header = if detecting {
        let spin = spinner::frame(tick);
        Line::from(vec![
            Span::raw("  "),
            Span::styled(spin, Style::default().fg(t.primary).bold()),
            Span::raw(" "),
            Span::styled(
                t!("tui_runtime_views_devices_detecting"),
                Style::default().fg(t.text_muted),
            ),
        ])
    } else {
        let count = devices.len();
        let label = t!("tui_runtime_views_devices_found", count = count);
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                label,
                Style::default().fg(if count == 0 { t.error } else { t.text_muted }),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(header), layout[0]);

    // ── Device list ─────────────────────────────────────────────────────
    let items: Vec<Line> = if detecting {
        vec![Line::from("")]
    } else if devices.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", t!("tui_runtime_views_devices_none")),
                Style::default().fg(t.text_muted),
            )),
            Line::from(Span::styled(
                format!("  {}", t!("tui_runtime_views_devices_none_hint")),
                Style::default().fg(t.text_dim),
            )),
        ]
    } else {
        devices
            .iter()
            .enumerate()
            .map(|(i, (id, name))| {
                // Show device ID in dim if it differs from the display name
                let show_id = id != name;
                if i == selected {
                    let mut spans = vec![
                        Span::styled(" ▶ ", Style::default().fg(t.primary).bold()),
                        Span::styled(name.as_str(), Style::default().fg(t.text).bold()),
                    ];
                    if show_id {
                        spans.push(Span::styled(
                            format!("  {id}"),
                            Style::default().fg(t.text_dim),
                        ));
                    }
                    Line::from(spans)
                } else {
                    let mut spans = vec![
                        Span::raw("   "),
                        Span::styled(name.as_str(), Style::default().fg(t.text_muted)),
                    ];
                    if show_id {
                        spans.push(Span::styled(
                            format!("  {id}"),
                            Style::default().fg(t.text_dim),
                        ));
                    }
                    Line::from(spans)
                }
            })
            .collect()
    };

    let list_paragraph =
        Paragraph::new(items).block(Block::default().padding(Padding::new(1, 1, 0, 0)));
    frame.render_widget(list_paragraph, layout[1]);

    // A row click highlights it and confirms, like moving there and pressing Enter.
    if !detecting {
        for i in 0..devices.len() {
            let row = Rect::new(
                layout[1].x + 1,
                layout[1].y + i as u16,
                layout[1].width.saturating_sub(2),
                1,
            )
            .intersection(layout[1]);
            clicks.click(row, Message::DevicePickerPick(i));
        }
    }

    // ── Footer hints ────────────────────────────────────────────────────
    let footer = if detecting || devices.is_empty() {
        Line::from(vec![
            Span::styled(
                format!(" {} ", t!("common_keys_enter")),
                Style::default().fg(t.primary).bold(),
            ),
            Span::styled(
                t!("tui_runtime_views_devices_spawn_without"),
                Style::default().fg(t.text_dim),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" {} ", t!("common_keys_escape")),
                Style::default().fg(t.text_dim),
            ),
            Span::styled(t!("common_action_cancel"), Style::default().fg(t.text_dim)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(t.primary).bold()),
            Span::styled(
                t!("tui_runtime_views_devices_navigate"),
                Style::default().fg(t.text_dim),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" {} ", t!("common_keys_enter")),
                Style::default().fg(t.primary).bold(),
            ),
            Span::styled(
                t!("tui_runtime_views_devices_select"),
                Style::default().fg(t.text_dim),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" {} ", t!("common_keys_escape")),
                Style::default().fg(t.text_dim),
            ),
            Span::styled(t!("common_action_cancel"), Style::default().fg(t.text_dim)),
        ])
    };

    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::default().bg(t.bg_panel)),
        layout[2],
    );
}
