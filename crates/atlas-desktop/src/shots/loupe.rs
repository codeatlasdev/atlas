//! The overlay's magnifier: a 15×15 px block of the frozen frame around the pointer, as a PNG data URL.
//!
//! The frame is taken once per display with the overlay already up and excluded from it (`runner::show_overlay`),
//! so the loupe shows what the screen looked like, not the dimmed overlay. Until the freeze is published there is
//! no loupe (the page hides it).

use super::{Loupe, Shots, geom::loupe_source, pixels};

/// Block size in source pixels; the page draws it 8× larger.
const GRID: u32 = 15;

/// The block around the global point `(x, y)`, from the frozen frame of the display that contains it.
pub fn sample(state: &Shots, x: f64, y: f64) -> Option<Loupe> {
    let session = state.frozen.peek()?;
    let d = session.displays.iter().find(|d| d.frame.contains(x, y))?;
    let f = session.frames.get(&d.id)?;
    let (bx, by, bw, bh) = loupe_source((x, y), d.frame, d.scale, GRID);
    // The frame is the display's pixel grid. If it is smaller (the mode changed since it was taken) clamp instead of
    // failing: a slightly stale loupe is better than none.
    let (bw, bh) = (bw.min(f.size.0), bh.min(f.size.1));
    let bx = bx.min(f.size.0 - bw);
    let by = by.min(f.size.1 - bh);
    let block = pixels::block(&f.rgba, f.size, bx, by, bw, bh).ok()?;
    Some(Loupe {
        hex: pixels::centre_hex(&block, bw, bh)?,
        url: pixels::data_url(&block, bw, bh).ok()?,
    })
}
