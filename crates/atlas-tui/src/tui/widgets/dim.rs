//! Modal backdrop: keep what is on screen, just push it back.

use ratatui::prelude::*;

use crate::theme;

/// Recolor every cell of `area` to the dim foreground on the base background,
/// so logs and sidebar stay readable-as-context behind a modal.
pub fn backdrop(frame: &mut Frame, area: Rect) {
    let t = theme::current();
    let buf = frame.buffer_mut();
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.set_style(
                Style::default()
                    .fg(t.text_dim)
                    .bg(t.bg)
                    .remove_modifier(Modifier::BOLD),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn backdrop_keeps_text_and_dims_fg() {
        let mut term = Terminal::new(TestBackend::new(10, 2)).unwrap();
        term.draw(|f| {
            f.render_widget(
                Line::styled("hello", Style::default().fg(Color::Red)),
                f.area(),
            );
            backdrop(f, f.area());
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), "h");
        assert_eq!(buf[(0, 0)].fg, theme::current().text_dim);
    }
}
