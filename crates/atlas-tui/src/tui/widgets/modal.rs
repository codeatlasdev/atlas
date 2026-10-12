//! Shared modal chrome: dimmed backdrop, centered rounded box, click wiring.

use ratatui::prelude::*;
use ratatui::widgets::*;

use super::dim;
use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::{ClickMap, Target};

/// Look of one modal: its title, border color and what its body does with clicks.
pub struct Modal<'a> {
    pub title: &'a str,
    pub border: Color,
    /// Target recorded over the box itself (`Inert` swallows stray clicks).
    pub body: Target,
    /// Sent when the backdrop is clicked.
    pub cancel: Message,
}

impl<'a> Modal<'a> {
    /// Standard modal: neutral border, body swallows clicks.
    pub fn new(title: &'a str, cancel: Message) -> Self {
        Self {
            title,
            border: theme::current().border,
            body: Target::Inert,
            cancel,
        }
    }

    /// Dim the screen, draw the box of `width` × `height` (already clamped to the
    /// terminal by the caller) centered, and record its click targets.
    /// Returns `(modal_area, inner)`.
    pub fn frame(
        self,
        frame: &mut Frame,
        width: u16,
        height: u16,
        clicks: &ClickMap,
    ) -> (Rect, Rect) {
        let t = theme::current();
        let area = frame.area();

        // Keep the dashboard visible, just pushed back.
        dim::backdrop(frame, area);

        let x = area.width.saturating_sub(width) / 2;
        let y = area.height.saturating_sub(height) / 2;
        let modal_area = Rect::new(x, y, width, height);

        let block = Block::default()
            .title(self.title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .style(Style::default().bg(t.bg_panel));
        let inner = block.inner(modal_area);
        frame.render_widget(Clear, modal_area);
        frame.render_widget(block, modal_area);

        // Backdrop cancels; the box takes its own target; content sits on top.
        clicks.click(area, self.cancel);
        clicks.push(modal_area, self.body);
        (modal_area, inner)
    }
}
