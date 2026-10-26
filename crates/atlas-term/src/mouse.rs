//! Mouse reports for programs that ask for them (DECSET 1000/1002/1003, encodings 1005/1006).
//! Pure: a pointer event and the terminal modes in, the bytes for the pty out.

use alacritty_terminal::term::TermMode;

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    /// Moved with `button` held (`None`: no button, mode 1003 only).
    Motion,
    /// Wheel notch: up = away from the user (button 64), down (65).
    WheelUp,
    WheelDown,
}

/// Buttons 0 left, 1 middle, 2 right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub action: Action,
    pub button: Option<u8>,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    /// 0-based cell.
    pub col: u16,
    pub row: u16,
    /// Terminal-space pixel position, 0 = the grid's top-left corner, rounded, not clamped (negative when a
    /// drag left the terminal): what SGR-Pixels (1016) reports. `None` when the caller only knows the cell.
    pub px: Option<(i32, i32)>,
}

/// Does the program own the mouse (any reporting mode on)? Then clicks and drags belong to it,
/// except with ⇧ held (Ghostty `mouse-shift-capture = false`): ⇧ selects text anyway.
pub fn captures(mode: TermMode, shift: bool) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !shift
}

/// Bytes for `e`, or `None` when the active mode does not report it. `pixel` is DECSET 1016
/// (`Vt::pixel_mouse`): the SGR shape with pixel coordinates, ahead of 1006/1005 as in xterm and Ghostty.
pub fn report(e: &Event, mode: TermMode, pixel: bool) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    // 1016 is its own format, so it also needs a position in pixels; without one the cell is the best answer.
    let pixels = e.px.filter(|_| pixel);
    let sgr = mode.contains(TermMode::SGR_MOUSE) || pixel;
    let base = match e.action {
        Action::Press => e.button?.min(2),
        // Legacy encodings cannot say which button went up.
        Action::Release if sgr => e.button?.min(2),
        Action::Release => 3,
        Action::Motion => {
            let reported = match e.button {
                Some(_) => mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION),
                None => mode.contains(TermMode::MOUSE_MOTION),
            };
            if !reported {
                return None;
            }
            32 + e.button.map_or(3, |b| b.min(2))
        }
        Action::WheelUp => 64,
        Action::WheelDown => 65,
    };
    let code = base + 4 * u8::from(e.shift) + 8 * u8::from(e.alt) + 16 * u8::from(e.ctrl);
    let end = if e.action == Action::Release {
        'm'
    } else {
        'M'
    };
    if let Some((px, py)) = pixels {
        return Some(format!("\x1b[<{code};{px};{py}{end}").into_bytes());
    }
    let (x, y) = (u32::from(e.col) + 1, u32::from(e.row) + 1);
    if sgr {
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    let mut out = b"\x1b[M".to_vec();
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    for v in [u32::from(code) + 32, x + 32, y + 32] {
        match char::from_u32(v) {
            // UTF-8 (1005) reaches 2015 and encodes past 127 in two bytes.
            Some(c) if utf8 && v <= 2015 => out.extend_from_slice(c.to_string().as_bytes()),
            _ if !utf8 && v <= 255 => out.push(v as u8),
            // Out of range: dropping the report beats a corrupt one that confuses the program.
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells only (no 1016): what every test below the 1016 block means by `report`.
    fn report(e: &Event, mode: TermMode) -> Option<Vec<u8>> {
        super::report(e, mode, false)
    }

    fn report3(e: &Event, mode: TermMode, pixel: bool) -> Option<Vec<u8>> {
        super::report(e, mode, pixel)
    }

    const CLICK: TermMode = TermMode::MOUSE_REPORT_CLICK;

    fn ev(action: Action, button: Option<u8>, col: u16, row: u16) -> Event {
        Event {
            action,
            button,
            shift: false,
            alt: false,
            ctrl: false,
            col,
            row,
            px: None,
        }
    }

    /// An event with a pixel position (terminal-space, 0-based: what the page measures).
    fn evp(action: Action, button: Option<u8>, col: u16, row: u16, px: (i32, i32)) -> Event {
        Event {
            px: Some(px),
            ..ev(action, button, col, row)
        }
    }

    // vim/tmux/omp TUI under `?1000h?1006h`: the encoding every modern program asks for.
    #[test]
    fn sgr_reports_press_and_release_with_one_based_cells() {
        let m = CLICK | TermMode::SGR_MOUSE;
        assert_eq!(
            report(&ev(Action::Press, Some(0), 0, 0), m).unwrap(),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            report(&ev(Action::Release, Some(0), 9, 4), m).unwrap(),
            b"\x1b[<0;10;5m"
        );
        assert_eq!(
            report(&ev(Action::Press, Some(2), 300, 70), m).unwrap(),
            b"\x1b[<2;301;71M"
        );
    }

    #[test]
    fn modifiers_and_wheel_set_the_button_bits() {
        let m = CLICK | TermMode::SGR_MOUSE;
        let mut e = ev(Action::Press, Some(0), 1, 1);
        e.shift = true;
        e.alt = true;
        e.ctrl = true;
        // shift 4 + alt 8 + ctrl 16.
        assert_eq!(report(&e, m).unwrap(), b"\x1b[<28;2;2M");
        assert_eq!(
            report(&ev(Action::WheelUp, None, 1, 1), m).unwrap(),
            b"\x1b[<64;2;2M"
        );
        assert_eq!(
            report(&ev(Action::WheelDown, None, 1, 1), m).unwrap(),
            b"\x1b[<65;2;2M"
        );
    }

    // `?1000` reports clicks only; `?1002` adds drags; `?1003` adds bare motion.
    #[test]
    fn modes_decide_which_motion_is_reported() {
        let drag = ev(Action::Motion, Some(0), 3, 3);
        let bare = ev(Action::Motion, None, 3, 3);
        let sgr = TermMode::SGR_MOUSE;
        assert_eq!(report(&drag, CLICK | sgr), None);
        assert_eq!(
            report(&drag, TermMode::MOUSE_DRAG | sgr).unwrap(),
            b"\x1b[<32;4;4M"
        );
        assert_eq!(report(&bare, TermMode::MOUSE_DRAG | sgr), None);
        assert_eq!(
            report(&bare, TermMode::MOUSE_MOTION | sgr).unwrap(),
            b"\x1b[<35;4;4M"
        );
    }

    // The legacy X10 encoding: `CSI M` + three bytes offset by 32; a release has no button.
    #[test]
    fn legacy_encoding_offsets_and_releases_as_button_three() {
        assert_eq!(
            report(&ev(Action::Press, Some(0), 0, 0), CLICK).unwrap(),
            b"\x1b[M !!"
        );
        assert_eq!(
            report(&ev(Action::Release, Some(0), 4, 2), CLICK).unwrap(),
            b"\x1b[M#%#"
        );
        // Past column 222 the single byte overflows: the report is dropped, not corrupted.
        assert_eq!(report(&ev(Action::Press, Some(0), 223, 0), CLICK), None);
        assert_eq!(report(&ev(Action::Press, Some(0), 0, 223), CLICK), None);
    }

    #[test]
    fn utf8_encoding_extends_the_range() {
        let m = CLICK | TermMode::UTF8_MOUSE;
        // col 223 → 223 + 33 = 256 → U+0100, two bytes; button ` ` and row `!` stay one byte.
        assert_eq!(
            report(&ev(Action::Press, Some(0), 223, 0), m).unwrap(),
            b"\x1b[M \xc4\x80!"
        );
    }

    #[test]
    fn nothing_is_reported_without_a_mouse_mode() {
        let m = TermMode::SGR_MOUSE;
        assert_eq!(report(&ev(Action::Press, Some(0), 1, 1), m), None);
        assert!(!captures(m, false));
        assert!(captures(CLICK, false));
        // ⇧ gives the selection back to the terminal.
        assert!(!captures(CLICK, true));
    }

    // xterm: wheel up is button 64 and down 65, both +32 on the wire (`` ` `` and `a`).
    #[test]
    fn wheel_in_legacy_mode_is_button_64_plus() {
        assert_eq!(
            report(&ev(Action::WheelUp, None, 0, 0), CLICK).unwrap(),
            b"\x1b[M`!!"
        );
        assert_eq!(
            report(&ev(Action::WheelDown, None, 0, 0), CLICK).unwrap(),
            b"\x1b[Ma!!"
        );
    }

    // The whole path a TUI relies on: it switches the modes on over the pty (`?1000h?1006h`),
    // the terminal parses them into `TermMode`, and a click then yields the SGR bytes. Switching
    // them off (`?1000l`) hands the mouse back.
    #[test]
    fn modes_switched_on_by_the_program_drive_the_reports() {
        let vt = crate::vt::Vt::new(20, 5, |_| {});
        let click = ev(Action::Press, Some(0), 4, 2);
        assert!(!captures(vt.mode(), false));
        assert_eq!(report(&click, vt.mode()), None);
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        assert!(captures(vt.mode(), false));
        assert_eq!(report(&click, vt.mode()).unwrap(), b"\x1b[<0;5;3M");
        vt.feed(b"\x1b[?1000l");
        assert!(!captures(vt.mode(), false));
    }

    const PIX: TermMode = TermMode::SGR_MOUSE;

    // DECSET 1016 (Ghostty `mouse_encode.zig` sgr_pixels): the SGR format with terminal-space pixels, 0-based
    // (no +1 as cells have), rounded to whole pixels by the caller.
    #[test]
    fn pixel_mode_reports_zero_based_pixels_in_the_sgr_shape() {
        let m = CLICK | PIX;
        assert_eq!(
            report3(&evp(Action::Press, Some(0), 4, 2, (83, 47)), m, true).unwrap(),
            b"\x1b[<0;83;47M"
        );
        assert_eq!(
            report3(&evp(Action::Release, Some(2), 4, 2, (83, 47)), m, true).unwrap(),
            b"\x1b[<2;83;47m"
        );
        assert_eq!(
            report3(&evp(Action::Press, Some(0), 0, 0, (0, 0)), m, true).unwrap(),
            b"\x1b[<0;0;0M"
        );
    }

    // xterm and Ghostty: 1016 wins over 1006 (and over 1005 or no encoding at all: it is its own format).
    #[test]
    fn pixel_mode_takes_precedence_over_the_other_encodings() {
        let e = evp(Action::Press, Some(0), 4, 2, (83, 47));
        assert_eq!(
            report3(&e, CLICK | PIX, false).unwrap(),
            b"\x1b[<0;5;3M",
            "off: cells"
        );
        for m in [CLICK, CLICK | PIX, CLICK | TermMode::UTF8_MOUSE] {
            assert_eq!(report3(&e, m, true).unwrap(), b"\x1b[<0;83;47M", "{m:?}");
        }
    }

    // Pixels need a mouse mode like every other encoding: 1016 alone reports nothing.
    #[test]
    fn pixel_mode_without_a_mouse_mode_reports_nothing() {
        let e = evp(Action::Press, Some(0), 4, 2, (83, 47));
        assert_eq!(report3(&e, PIX, true), None);
        assert_eq!(report3(&e, TermMode::empty(), true), None);
    }

    #[test]
    fn pixel_mode_keeps_modifiers_wheel_and_motion_rules() {
        let mut e = evp(Action::Press, Some(0), 1, 1, (12, 20));
        e.shift = true;
        e.ctrl = true;
        assert_eq!(report3(&e, CLICK | PIX, true).unwrap(), b"\x1b[<20;12;20M");
        assert_eq!(
            report3(&evp(Action::WheelUp, None, 1, 1, (12, 20)), CLICK, true).unwrap(),
            b"\x1b[<64;12;20M"
        );
        assert_eq!(
            report3(&evp(Action::WheelDown, None, 1, 1, (12, 20)), CLICK, true).unwrap(),
            b"\x1b[<65;12;20M"
        );
        let drag = evp(Action::Motion, Some(0), 1, 1, (12, 20));
        assert_eq!(
            report3(&drag, CLICK, true),
            None,
            "1000 does not report drags"
        );
        assert_eq!(
            report3(&drag, TermMode::MOUSE_DRAG, true).unwrap(),
            b"\x1b[<32;12;20M"
        );
        let bare = evp(Action::Motion, None, 1, 1, (12, 20));
        assert_eq!(report3(&bare, TermMode::MOUSE_DRAG, true), None);
        assert_eq!(
            report3(&bare, TermMode::MOUSE_MOTION, true).unwrap(),
            b"\x1b[<35;12;20M"
        );
    }

    // No legacy limit in pixels: a 4K terminal reports x > 3000 fine (the cell encodings drop past 222), and
    // Ghostty does not clamp, so a drag that left the terminal reports a negative position.
    #[test]
    fn pixel_mode_has_no_range_limit_and_does_not_clamp() {
        let m = CLICK | PIX;
        assert_eq!(
            report3(&evp(Action::Press, Some(0), 400, 90, (3790, 2100)), m, true).unwrap(),
            b"\x1b[<0;3790;2100M"
        );
        assert_eq!(
            report3(&evp(Action::Release, Some(0), 0, 0, (-4, -1)), m, true).unwrap(),
            b"\x1b[<0;-4;-1m"
        );
    }

    // No pixel position known (a caller that only has cells, e.g. a replayed trace): the cell is the best
    // answer, in the cell encoding the program would get without 1016 rather than nothing.
    #[test]
    fn pixel_mode_without_a_pixel_position_falls_back_to_the_cell_in_sgr() {
        let e = ev(Action::Press, Some(0), 4, 2);
        assert_eq!(report3(&e, CLICK, true).unwrap(), b"\x1b[<0;5;3M");
    }

    // The path a program takes: set the modes over the pty, then click; DECRST hands cells back.
    #[test]
    fn modes_switched_on_by_the_program_choose_pixels() {
        let vt = crate::vt::Vt::new(20, 5, |_| {});
        let click = evp(Action::Press, Some(0), 4, 2, (45, 40));
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(
            report3(&click, vt.mode(), vt.pixel_mouse()).unwrap(),
            b"\x1b[<0;5;3M"
        );
        vt.feed(b"\x1b[?1016h");
        assert_eq!(
            report3(&click, vt.mode(), vt.pixel_mouse()).unwrap(),
            b"\x1b[<0;45;40M"
        );
        vt.feed(b"\x1b[?1016l");
        assert_eq!(
            report3(&click, vt.mode(), vt.pixel_mouse()).unwrap(),
            b"\x1b[<0;5;3M"
        );
    }
}
