use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::PaletteCommand;

pub fn render(frame: &mut Frame, input: &str, commands: &[PaletteCommand], selected: usize) {
    let t = theme::current();
    let area = frame.area();

    let width = 50.min(area.width.saturating_sub(4));
    let height = (commands.len() as u16 + 4)
        .min(area.height.saturating_sub(4))
        .min(14);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = 3; // Top-aligned like VS Code
    let palette_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, palette_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.primary))
        .style(Style::default().bg(t.bg_panel));

    let inner = block.inner(palette_area);
    frame.render_widget(block, palette_area);

    let mut lines = Vec::new();

    // Input line
    let cursor = if input.is_empty() { "█" } else { "" };
    lines.push(Line::from(vec![
        Span::styled(": ", Style::default().fg(t.primary).bold()),
        Span::styled(input, Style::default().fg(t.text)),
        Span::styled(cursor, Style::default().fg(t.text_dim)),
    ]));

    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(width.saturating_sub(4) as usize),
        Style::default().fg(t.border_subtle),
    )));

    // Commands
    for (i, cmd) in commands.iter().enumerate() {
        let style = if i == selected {
            Style::default().fg(t.text).bg(t.bg_float)
        } else {
            Style::default().fg(t.text_muted)
        };

        let shortcut = cmd.shortcut.as_deref().unwrap_or("");
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<30}", cmd.name), style),
            Span::styled(format!("{:>4} ", shortcut), Style::default().fg(t.text_dim)),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Message;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_command_palette_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let commands = vec![
            PaletteCommand {
                name: "Restart".into(),
                description: "Restart all".into(),
                shortcut: Some("r".into()),
                action: Message::RestartAll,
            },
            PaletteCommand {
                name: "Quit".into(),
                description: "Exit".into(),
                shortcut: Some("q".into()),
                action: Message::ShowQuit,
            },
        ];
        terminal
            .draw(|frame| render(frame, "res", &commands, 0))
            .unwrap();
    }

    #[test]
    fn test_command_palette_empty_input() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, "", &[], 0)).unwrap();
    }
}
