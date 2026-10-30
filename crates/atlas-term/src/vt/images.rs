//! Inline images (Kitty graphics, sixel): anchoring a decoded placement at the cursor and
//! moving the cursor past it. Decoding and storage live in `crate::graphics`.

use super::Processor;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Term, TermMode};

use super::shell::Shell;
use super::{Proxy, Vt};
use crate::graphics::{Placed, Placement};

impl Vt {
    /// Images on the alternate screen go away when it is entered or left.
    pub(super) fn track_alt(&self, t: &Term<Proxy>, sh: &mut Shell) {
        let alt = t.mode().contains(TermMode::ALT_SCREEN);
        if alt != sh.alt {
            sh.alt = alt;
            sh.images.clear_alt();
        }
    }

    /// Anchor a placement at the cursor, then move the cursor past it:
    /// Kitty leaves it after the last column on the last row, sixel below the image.
    pub(super) fn place_image(
        &self,
        p: &mut Processor,
        t: &mut Term<Proxy>,
        sh: &mut Shell,
        mut pl: Placement,
        how: Placed,
        sixel: bool,
    ) {
        let [cw, ch] = sh.cell;
        let [_, _, w, h] = pl.src;
        let natural = (w as f32 / cw, h as f32 / ch);
        let (cols, rows) = match (how.cols, how.rows) {
            (0, 0) => natural,
            (c, 0) => (c as f32, c as f32 * natural.1 / natural.0.max(f32::EPSILON)),
            (0, r) => (r as f32 * natural.0 / natural.1.max(f32::EPSILON), r as f32),
            (c, r) => (c as f32, r as f32),
        };
        let cursor = t.grid().cursor.point;
        pl.line = t.grid().history_size() as i64 + cursor.line.0 as i64;
        pl.col = cursor.column.0 as u16;
        pl.cols = cols.ceil().max(1.0) as u16;
        pl.rows = rows.ceil().max(1.0) as u16;
        pl.alt = t.mode().contains(TermMode::ALT_SCREEN);
        pl.size = (cols, rows);
        let (pc, pr) = (pl.cols, pl.rows);
        // A placement id replaces its earlier placement of the same image.
        if pl.id != 0 {
            sh.images
                .placements
                .retain(|q| !(q.image == pl.image && q.id == pl.id));
        }
        sh.images.placements.push(pl);
        if how.keep_cursor {
            return;
        }
        let mut mv = String::new();
        let downs = if sixel { pr } else { pr - 1 };
        for _ in 0..downs {
            mv.push_str("\x1bD"); // IND: down, scrolling at the bottom margin.
        }
        if !sixel {
            mv.push_str(&format!("\x1b[{pc}C"));
        }
        p.advance(t, mv.as_bytes());
    }

    /// Cell size in physical px, for image ↔ cell conversion.
    pub fn set_cell_px(&self, w: f32, h: f32) {
        if w > 0.0 && h > 0.0 {
            self.shell.lock().cell = [w, h];
            let mut s = self.win.lock();
            (s.cell_width, s.cell_height) = (w.round() as u16, h.round() as u16);
        }
    }
}
