use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::logs::{LogEntry, LogLevel};
use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, logs: &[&LogEntry], scroll: usize) {
    let t = theme::current();

    let visible_height = area.height as usize;
    let start = scroll.saturating_sub(visible_height.saturating_sub(1));
    let end = (start + visible_height).min(logs.len());

    let lines: Vec<Line> = logs[start..end]
        .iter()
        .map(|entry| {
            let prefix: String = entry.service.chars().take(3).collect();
            let content = crate::runtime::logs::strip_ansi(&entry.content);
            let content_color = match entry.level {
                LogLevel::Error => t.error,
                LogLevel::Warn => t.warning,
                LogLevel::Debug => t.text_dim,
                LogLevel::Info => t.text_muted,
            };

            Line::from(vec![
                Span::styled(format!("[{prefix}] "), Style::default().fg(t.text_dim)),
                Span::styled(content, Style::default().fg(content_color)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(Block::default().padding(Padding::new(2, 1, 1, 0)));
    frame.render_widget(paragraph, area);
}
