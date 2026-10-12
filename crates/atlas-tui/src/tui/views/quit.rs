use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::ClickMap;
use crate::tui::widgets::modal::Modal;

/// Which quit dialog: the owner stops everything; an attached client chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Owner,
    Attached,
}

pub fn render(frame: &mut Frame, selected: usize, mode: Mode, clicks: &ClickMap) {
    let t = theme::current();
    let area = frame.area();

    let attached = mode == Mode::Attached;
    let width = if attached { 54 } else { 62 }.min(area.width.saturating_sub(4));
    let height = 8.min(area.height.saturating_sub(4));
    let (_, inner) = Modal::new(
        if attached { " Leave? " } else { " Quit? " },
        Message::HideQuit,
    )
    .frame(frame, width, height, clicks);

    // Left button is the safe one (Cancel / Detach); right is the destructive one.
    let (left, right, left_msg, right_msg) = if attached {
        (
            " Detach ",
            " Stop everything ",
            Message::Detach,
            Message::StopRunner,
        )
    } else {
        (" Cancel ", " Quit ", Message::HideQuit, Message::Quit)
    };
    let left_style = if selected == 0 {
        Style::default().fg(t.text).bg(t.bg_float).bold()
    } else {
        Style::default().fg(t.text_dim)
    };
    let right_style = if selected == 1 {
        Style::default().fg(t.text).bg(t.error).bold()
    } else {
        Style::default().fg(t.text_dim)
    };

    let (headline, hint) = if attached {
        (
            "Detach leaves the services running in the runner.",
            "reattach with atlas dev · stop with atlas down",
        )
    } else {
        (
            "All services will be stopped.",
            "keep them running: atlas dev --detach",
        )
    };
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(headline, Style::default().fg(t.text_muted))),
        Line::from(Span::styled(hint, Style::default().fg(t.text_dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled(left, left_style),
            Span::raw("   "),
            Span::styled(right, right_style),
        ]),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(Block::default());
    frame.render_widget(paragraph, inner);

    // Buttons row is line 4 of the centered paragraph: left + 3 gap + right.
    let (left_w, gap, right_w) = (
        left.chars().count() as u16,
        3u16,
        right.chars().count() as u16,
    );
    let row_w = left_w + gap + right_w;
    let bx = inner.x + inner.width.saturating_sub(row_w) / 2;
    let by = inner.y + 4;
    let bounds = inner;
    clicks.click(Rect::new(bx, by, left_w, 1).intersection(bounds), left_msg);
    clicks.click(
        Rect::new(bx + left_w + gap, by, right_w, 1).intersection(bounds),
        right_msg,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_quit_modal_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, 0, Mode::Owner, &ClickMap::default()))
            .unwrap();
    }

    #[test]
    fn test_quit_modal_selected_quit() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, 1, Mode::Owner, &ClickMap::default()))
            .unwrap();
    }

    #[test]
    fn test_quit_modal_attached_offers_detach_and_stop() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, 0, Mode::Attached, &ClickMap::default()))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol().to_string())
            .collect();
        assert!(text.contains("Detach") && text.contains("Stop everything"));
    }
}
