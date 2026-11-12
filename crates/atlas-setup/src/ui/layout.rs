//! Layout of the setup window in logical pixels, and what a click on a point means. Pure: the
//! window scales these rectangles by the monitor's DPI, the renderer paints them, and the tests
//! check them without any window.

use crate::state::{Phase, Snapshot};

pub const WIDTH: f32 = 460.0;
pub const HEIGHT: f32 = 320.0;
pub const RADIUS: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

pub const CLOSE: Rect = Rect::new(WIDTH - 40.0, 8.0, 32.0, 28.0);
pub const LOGO: Rect = Rect::new(WIDTH / 2.0 - 48.0, 40.0, 96.0, 96.0);
pub const BAR: Rect = Rect::new(60.0, 252.0, WIDTH - 120.0, 4.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Close,
    CopyDetails,
    /// Failed: dismiss. Done: nothing (the window closes itself).
    Dismiss,
    KeepData,
    RemoveData,
    /// The second step: really delete.
    ConfirmRemove,
    /// The second step: back to the question.
    GoBack,
}

#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub button: Button,
    pub rect: Rect,
    pub label: &'static str,
    pub primary: bool,
}

/// Buttons sit low so the text above them has room for four lines.
const BTN_Y: f32 = 262.0;
const BTN_H: f32 = 34.0;

/// The buttons the window shows in this state (the close box is always there, drawn apart).
pub fn buttons(s: &Snapshot) -> Vec<Placed> {
    let two = |a: (Button, &'static str, bool), b: (Button, &'static str, bool)| {
        let w = 170.0;
        let gap = 12.0;
        let x0 = (WIDTH - (2.0 * w + gap)) / 2.0;
        vec![
            Placed {
                button: a.0,
                rect: Rect::new(x0, BTN_Y, w, BTN_H),
                label: a.1,
                primary: a.2,
            },
            Placed {
                button: b.0,
                rect: Rect::new(x0 + w + gap, BTN_Y, w, BTN_H),
                label: b.1,
                primary: b.2,
            },
        ]
    };
    match s.phase {
        Phase::Failed => two(
            (Button::CopyDetails, "Copy details", false),
            (Button::Dismiss, "Close", true),
        ),
        // The default (primary) answer is KEEP.
        Phase::AskData => two(
            (Button::RemoveData, "Delete my data", false),
            (Button::KeepData, "Keep my data", true),
        ),
        // The second step: the way back is the primary one.
        Phase::ConfirmRemove => two(
            (Button::ConfirmRemove, "Yes, delete it", false),
            (Button::GoBack, "No, go back", true),
        ),
        _ => Vec::new(),
    }
}

/// What is under the point `(x, y)` (logical px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Button(Button),
    Drag,
    Client,
}

pub fn hit(s: &Snapshot, x: f32, y: f32) -> Hit {
    if CLOSE.contains(x, y) {
        return Hit::Button(Button::Close);
    }
    if let Some(b) = buttons(s).into_iter().find(|b| b.rect.contains(x, y)) {
        return Hit::Button(b.button);
    }
    // The whole window drags except the buttons: there is nothing else to click.
    if (0.0..WIDTH).contains(&x) && (0.0..HEIGHT).contains(&y) {
        Hit::Drag
    } else {
        Hit::Client
    }
}

/// Wrap `text` to lines no wider than `max` according to `measure`; words longer than a line are
/// split. At most `max_lines` come back, the last ending in `…` when text was cut.
pub fn wrap(text: &str, max: f32, max_lines: usize, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.lines() {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let try_ = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if measure(&try_) <= max {
                cur = try_;
                continue;
            }
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            // A single word wider than the line: break it by characters.
            let mut piece = String::new();
            for ch in word.chars() {
                piece.push(ch);
                if measure(&piece) > max && piece.chars().count() > 1 {
                    let last = piece.pop().unwrap();
                    lines.push(std::mem::take(&mut piece));
                    piece.push(last);
                }
            }
            cur = piece;
        }
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(l) = lines.last_mut() {
            l.push('…');
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Mode;

    fn snap(phase: Phase) -> Snapshot {
        Snapshot {
            mode: Mode::Install,
            phase,
            status: String::new(),
            note: String::new(),
            progress: 0.5,
            details: String::new(),
        }
    }

    fn chars(s: &str) -> f32 {
        s.chars().count() as f32 * 7.0
    }

    #[test]
    fn a_working_window_has_no_buttons_only_the_close_box() {
        assert!(buttons(&snap(Phase::Working)).is_empty());
        assert!(buttons(&snap(Phase::Done)).is_empty());
        assert_eq!(
            hit(&snap(Phase::Working), WIDTH - 20.0, 20.0),
            Hit::Button(Button::Close)
        );
    }

    #[test]
    fn the_data_question_defaults_to_keep() {
        let b = buttons(&snap(Phase::AskData));
        let primary: Vec<_> = b.iter().filter(|p| p.primary).collect();
        assert_eq!(primary.len(), 1);
        assert_eq!(
            primary[0].button,
            Button::KeepData,
            "keep is the default answer"
        );
        assert!(b.iter().any(|p| p.button == Button::RemoveData));
    }

    #[test]
    fn the_second_step_defaults_to_going_back() {
        let b = buttons(&snap(Phase::ConfirmRemove));
        let primary: Vec<_> = b.iter().filter(|p| p.primary).collect();
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].button, Button::GoBack);
        assert!(
            b.iter()
                .any(|p| p.button == Button::ConfirmRemove && !p.primary)
        );
    }

    #[test]
    fn a_failure_offers_copy_details_and_close() {
        let b = buttons(&snap(Phase::Failed));
        assert!(b.iter().any(|p| p.button == Button::CopyDetails));
        assert!(b.iter().any(|p| p.button == Button::Dismiss && p.primary));
    }

    #[test]
    fn buttons_fit_inside_the_window_and_do_not_overlap() {
        for ph in [Phase::Failed, Phase::AskData, Phase::ConfirmRemove] {
            let b = buttons(&snap(ph));
            for p in &b {
                assert!(
                    p.rect.x >= 0.0
                        && p.rect.x + p.rect.w <= WIDTH
                        && p.rect.y + p.rect.h <= HEIGHT,
                    "{p:?}"
                );
            }
            assert!(b[0].rect.x + b[0].rect.w <= b[1].rect.x);
        }
    }

    #[test]
    fn a_click_on_a_button_is_a_button_and_elsewhere_drags() {
        let s = snap(Phase::Failed);
        let b = buttons(&s)[0];
        assert_eq!(
            hit(&s, b.rect.x + 1.0, b.rect.y + 1.0),
            Hit::Button(Button::CopyDetails)
        );
        assert_eq!(hit(&s, 10.0, 200.0), Hit::Drag);
        assert_eq!(hit(&s, -1.0, 5.0), Hit::Client);
        // While working the same spot is just the window.
        assert_eq!(
            hit(&snap(Phase::Working), b.rect.x + 1.0, b.rect.y + 1.0),
            Hit::Drag
        );
    }

    #[test]
    fn wrap_breaks_on_words_and_splits_a_word_wider_than_the_line() {
        let l = wrap("alpha beta gamma delta", 7.0 * 11.0, 9, &chars);
        assert_eq!(l, ["alpha beta", "gamma delta"]);
        let long = wrap(
            "C:\\a\\very\\long\\path\\without\\spaces",
            7.0 * 10.0,
            9,
            &chars,
        );
        assert!(
            long.len() > 1 && long.iter().all(|s| chars(s) <= 70.0),
            "{long:?}"
        );
        assert_eq!(long.concat(), "C:\\a\\very\\long\\path\\without\\spaces");
    }

    #[test]
    fn wrap_truncates_with_an_ellipsis_and_keeps_paragraphs() {
        let l = wrap("one\ntwo\nthree\nfour", 100.0, 2, &chars);
        assert_eq!(l, ["one", "two…"]);
        assert!(wrap("", 100.0, 3, &chars).is_empty(), "nothing to draw");
        // The failure details separate the reason from the log path with a blank line: it stays.
        assert_eq!(
            wrap("reason\n\nLog: x", 100.0, 5, &chars),
            ["reason", "", "Log: x"]
        );
    }
}
