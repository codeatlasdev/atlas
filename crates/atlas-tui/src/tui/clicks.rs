//! Per-frame hit map. Widgets record the exact `Rect` they drew together with
//! what a click or wheel turn there means; `App` reads it back for mouse events.
//! Rendering is the single source of truth: no layout math is replicated.

use std::cell::RefCell;

use ratatui::layout::Rect;

use super::app::Message;

/// What a position means to the mouse.
#[derive(Debug, Clone)]
pub enum Target {
    /// Left click sends this message (same one the equivalent key sends).
    Click(Message),
    /// Swallows clicks (modal body) so they don't fall through to the backdrop.
    Inert,
    /// Wheel turns over this region scroll/move the given list.
    Wheel(Wheel),
    /// Log scrollbar track: click jumps, drag scrolls. `top`/`track` are the geometry
    /// of the rendered thumb math; see `log_viewer::render`.
    Scrollbar(ScrollbarGeometry),
}

/// A scrollable region the wheel can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wheel {
    Logs,
    /// Command palette selection.
    Palette,
    /// Device picker selection.
    Picker,
}

/// Geometry of the log scrollbar needed to invert a mouse row into a scroll offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarGeometry {
    /// Rows in the track (same as the log area height).
    pub track: u16,
    /// Thumb length in rows.
    pub thumb: u16,
    /// Largest valid `log_scroll`.
    pub max_scroll: usize,
    /// Screen row of the first row of the scrollbar track.
    pub y_origin: u16,
}

impl ScrollbarGeometry {
    /// `log_scroll` for the pointer at absolute screen row `abs_y`, centering the thumb.
    /// Inverse of the thumb placement in `log_viewer::render`.
    pub fn scroll_for_row(&self, abs_y: u16) -> usize {
        let travel = self.track.saturating_sub(self.thumb) as usize;
        if travel == 0 {
            return 0;
        }
        // Convert to track-relative row, then center the thumb on the pointer.
        let rel = (abs_y as usize).saturating_sub(self.y_origin as usize);
        let pos = rel.saturating_sub(self.thumb as usize / 2).min(travel);
        // Invert: pos = travel * top / max_scroll  =>  top = ceil(pos * max_scroll / travel)
        let top = (pos * self.max_scroll)
            .div_ceil(travel.max(1))
            .min(self.max_scroll);
        self.max_scroll - top
    }
}

/// Topmost-wins list of hit targets for the current frame.
#[derive(Debug, Default)]
pub struct ClickMap {
    pub(crate) targets: RefCell<Vec<(Rect, Target)>>,
}

impl ClickMap {
    pub fn clear(&self) {
        self.targets.borrow_mut().clear();
    }

    pub fn push(&self, rect: Rect, target: Target) {
        if !rect.is_empty() {
            self.targets.borrow_mut().push((rect, target));
        }
    }

    /// Shorthand for the common `Click` case.
    pub fn click(&self, rect: Rect, msg: Message) {
        self.push(rect, Target::Click(msg));
    }

    /// Topmost target containing `(x, y)`; later pushes are drawn on top.
    pub fn hit(&self, x: u16, y: u16) -> Option<(Rect, Target)> {
        let pos = ratatui::layout::Position::new(x, y);
        self.targets
            .borrow()
            .iter()
            .rev()
            .find(|(r, _)| r.contains(pos))
            .cloned()
    }

    /// Topmost *click-capable* target at `(x, y)`, or None if an inert/other target is on top.
    pub fn click_at(&self, x: u16, y: u16) -> Option<Message> {
        match self.hit(x, y)?.1 {
            Target::Click(m) => Some(m),
            _ => None,
        }
    }
}

/// `Rect` of `width` columns at `(x, y)` clipped to `bounds`.
pub fn clip(x: u16, y: u16, width: u16, bounds: Rect) -> Rect {
    Rect::new(x, y, width, 1).intersection(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topmost_wins_and_edges_are_inclusive_exclusive() {
        let m = ClickMap::default();
        m.click(Rect::new(0, 0, 10, 10), Message::HideQuit);
        m.push(Rect::new(2, 2, 4, 1), Target::Inert);
        m.click(Rect::new(3, 2, 2, 1), Message::ShowHelp);
        assert!(matches!(m.click_at(3, 2), Some(Message::ShowHelp)));
        assert!(m.click_at(2, 2).is_none(), "inert body swallows the click");
        assert!(matches!(m.click_at(0, 0), Some(Message::HideQuit)));
        assert!(m.click_at(10, 0).is_none());
        assert!(m.click_at(5, 2).is_none());
    }

    #[test]
    fn scrollbar_geometry_round_trips() {
        // Same formula as log_viewer::render: pos = (track - thumb) * top / max_scroll.
        let g = ScrollbarGeometry {
            track: 20,
            thumb: 4,
            max_scroll: 100,
            y_origin: 0,
        };
        // Pointer at the top of the track → oldest (max_scroll); bottom → newest (0).
        assert_eq!(g.scroll_for_row(0), 100);
        assert_eq!(g.scroll_for_row(19), 0);
        let mid = g.scroll_for_row(10);
        assert!(mid > 0 && mid < 100);
    }
}
