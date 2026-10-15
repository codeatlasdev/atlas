//! The per-terminal render thread: sleeps until new bytes, a resize or a blink/bell
//! deadline, then snapshots the VT and draws it.

use std::sync::Arc;
use std::time::Instant;

use atlas_term::{FontResolution, FontSpec, Frame, Renderer};

use super::shared::{BLINK, Shared};

/// Selection highlight: #264F78 at ~67% opacity.
const SELECTION: [u8; 4] = [38, 79, 120, 170];
/// Search matches (dim amber) and the current match (strong orange), alpha-blended.
const SEARCH: [u8; 4] = [255, 200, 0, 90];
const SEARCH_CURRENT: [u8; 4] = [255, 140, 0, 200];

pub(super) fn render_loop(
    mut r: Renderer,
    sh: Arc<Shared>,
    on_resize: impl Fn(u16, u16),
    on_font: impl Fn(&FontSpec, &FontResolution),
    on_first_frame: impl FnOnce(),
) {
    let mut on_first_frame = Some(on_first_frame);
    let mut pal = sh.state.lock().pal.clone();
    let mut frame = Frame::default();
    let mut blink_at = Instant::now() + BLINK;
    let mut blink_on = true;
    loop {
        let (size, font_pt, font_new, ligatures, focused, bell) = {
            let mut s = sh.state.lock();
            loop {
                if s.closed || s.size.is_some() || s.dirty {
                    // New output or input: restart the blink cycle visible.
                    blink_on = true;
                    blink_at = Instant::now() + BLINK;
                    break;
                }
                let blink = frame.cursor_blinks.then_some(blink_at);
                let next = match (blink, s.bell_until) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                let Some(next) = next else {
                    sh.wake.wait(&mut s);
                    continue;
                };
                if !sh.wake.wait_until(&mut s, next).timed_out() {
                    continue;
                }
                let now = Instant::now();
                if s.bell_until.is_some_and(|b| b <= now) {
                    s.bell_until = None;
                }
                if frame.cursor_blinks && blink_at <= now {
                    blink_on = !blink_on;
                    blink_at = now + BLINK;
                }
                break;
            }
            if s.closed {
                return;
            }
            if std::mem::take(&mut s.reset) {
                r.reset_stats();
            }
            s.dirty = false;
            if std::mem::take(&mut s.pal_new) {
                pal = s.pal.clone();
            }
            (
                s.size.take(),
                s.font_pt.take(),
                s.font_new.take(),
                s.ligatures.take(),
                s.focused,
                s.bell_until.is_some(),
            )
        };
        if let Some(on) = ligatures {
            r.set_ligatures(on);
        }
        // A new font size or family changes the cell size, so the grid is re-fitted (and
        // the pty told) exactly like a resize.
        let font_change = match (font_new, font_pt) {
            (None, None) => None,
            (spec, pt) => {
                let spec = spec.unwrap_or_else(|| r.font_spec().clone());
                let pt = pt.unwrap_or_else(|| sh.state.lock().font_cur);
                let (grid, res) = r.set_font(pt, &spec);
                on_font(&spec, &res);
                Some(grid)
            }
        };
        let fitted = match (font_change, size) {
            (Some(grid), size) => Some(size.map_or(grid, |(w, h)| r.resize(w, h))),
            (None, Some((w, h))) => Some(r.resize(w, h)),
            (None, None) => None,
        };
        if let Some((cols, rows)) = fitted {
            let [cw, ch] = r.cell_px();
            sh.vt.set_cell_px(cw, ch);
            if sh.vt.size() != (cols, rows) {
                sh.vt.resize(cols, rows);
                on_resize(cols, rows);
            }
        }
        frame.selection_color = SELECTION;
        frame.search_color = SEARCH;
        frame.search_current_color = SEARCH_CURRENT;
        frame.focused = focused;
        frame.blink_on = blink_on;
        sh.vt.snapshot(&pal, &mut frame);
        if bell {
            let tint = |c: [u8; 4]| {
                [
                    c[0].saturating_add(40),
                    c[1].saturating_add(40),
                    c[2].saturating_add(40),
                    c[3],
                ]
            };
            frame.bg.iter_mut().for_each(|c| *c = tint(*c));
            frame.clear = tint(frame.clear);
        }
        match r.render(&frame) {
            // The first presented frame: the native view may be shown now (see `Placement`).
            Ok(true) => {
                if let Some(f) = on_first_frame.take() {
                    f();
                }
            }
            Ok(false) => {}
            Err(e) => eprintln!("atlas-term render: {e}"),
        }
        {
            let mut s = sh.state.lock();
            s.stats = r.stats();
            s.cell = r.cell_px();
        }
    }
}
