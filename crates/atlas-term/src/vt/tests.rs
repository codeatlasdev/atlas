//! `Vt` behavior: parsing, snapshot, prompt marks, images, selection, search, URLs.

use super::*;
use crate::colors::{NamedColor, Palette, rgba};
use alacritty_terminal::vte::ansi::Rgb;
use parking_lot::Mutex;
use std::sync::Arc;

fn pal() -> Palette {
    Palette::new(
        Rgb {
            r: 200,
            g: 200,
            b: 200,
        },
        Rgb {
            r: 10,
            g: 10,
            b: 10,
        },
        None,
    )
}

#[test]
fn parses_sgr_and_snapshots_colors() {
    let vt = Vt::new(10, 2, |_| {});
    vt.feed(b"\x1b[31mA\x1b[0;44mB\x1b[0m");
    let mut f = Frame {
        focused: true,
        blink_on: true,
        ..Frame::default()
    };
    vt.snapshot(&pal(), &mut f);
    assert_eq!(vt.text(), "AB");
    assert_eq!(f.glyphs[0].fg, rgba(pal().base[1]));
    assert_eq!(f.bg[1], rgba(pal().base[4]));
    // Cursor sits on col 2 with the cursor color.
    assert_eq!(f.bg[2], rgba(pal().base[256]));
}

const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

fn focused_frame() -> Frame {
    Frame {
        focused: true,
        blink_on: true,
        selection_color: [38, 79, 120, 170],
        ..Frame::default()
    }
}

#[test]
fn themed_selection_is_opaque_and_keeps_its_foreground() {
    let vt = Vt::new(10, 2, |_| {});
    vt.feed(b"\x1b[31mAB\x1b[0m");
    vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
    vt.update_selection(0, 0, Side::Right);
    let mut p = pal();
    // Unset: the translucent blend over the cell background, text untouched.
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.bg[0], [28, 56, 83, 255]);
    assert_eq!(f.glyphs[0].fg, rgba(p.base[1]));
    // Set: the opaque theme color, whatever the cell had; selected text takes selection_fg.
    let sel = Rgb {
        r: 70,
        g: 71,
        b: 90,
    };
    p.selection_bg = Some(sel);
    p.selection_fg = Some(WHITE);
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.bg[0], rgba(sel));
    assert_eq!(f.glyphs[0].fg, rgba(WHITE));
    // The unselected neighbour is untouched.
    assert_eq!(f.bg[1], rgba(p.bg(&Default::default())));
    assert_eq!(f.glyphs[1].fg, rgba(p.base[1]));
    // selection_bg alone keeps the cell's own foreground.
    p.selection_fg = None;
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.bg[0], rgba(sel));
    assert_eq!(f.glyphs[0].fg, rgba(p.base[1]));
}

#[test]
fn selection_foreground_survives_min_contrast() {
    let vt = Vt::new(10, 2, |_| {});
    vt.feed(b"A");
    vt.start_selection(0, 0, SelectionType::Simple, Side::Left);
    vt.update_selection(0, 0, Side::Right);
    let near = Rgb {
        r: 80,
        g: 80,
        b: 80,
    };
    let mut p = pal();
    p.min_contrast = 7.0;
    p.selection_bg = Some(near);
    p.selection_fg = Some(Rgb {
        r: 82,
        g: 82,
        b: 82,
    });
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(
        f.glyphs[0].fg,
        rgba(Rgb {
            r: 82,
            g: 82,
            b: 82
        })
    );
}

#[test]
fn block_cursor_uses_cursor_and_cursor_text_colors() {
    let vt = Vt::new(10, 2, |_| {});
    vt.feed(b"A\x1b[1D");
    let (cur, text) = (Rgb { r: 1, g: 200, b: 3 }, Rgb { r: 9, g: 9, b: 99 });
    // Cursor color override only: text = the cell background (today's swap).
    let mut p = pal().with_cursor(Some(cur));
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.bg[0], rgba(cur));
    assert_eq!(f.glyphs[0].fg, rgba(p.bg(&Default::default())));
    // With cursor_text.
    p.cursor_text = Some(text);
    p.min_contrast = 21.0;
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.bg[0], rgba(cur));
    assert_eq!(f.glyphs[0].fg, rgba(text));
    // Bar cursors use the cursor color too.
    vt.set_options(100, "bar", false);
    let mut f = focused_frame();
    vt.snapshot(&p, &mut f);
    assert_eq!(f.cursor.unwrap().color, rgba(cur));
}

#[test]
fn answers_device_attributes_through_the_proxy() {
    let out = Arc::new(Mutex::new(String::new()));
    let o = out.clone();
    let vt = Vt::new(10, 2, move |e| {
        if let VtEvent::Write(s) = e {
            o.lock().push_str(&s);
        }
    });
    vt.feed(b"\x1b[c");
    assert!(out.lock().starts_with("\x1b[?"), "{:?}", out.lock());
}

#[test]
fn resize_reflows_the_grid() {
    let vt = Vt::new(4, 2, |_| {});
    vt.feed(b"abcdef");
    assert_eq!(vt.text(), "abcd\nef");
    vt.resize(8, 2);
    assert_eq!(vt.size(), (8, 2));
    assert_eq!(vt.text(), "abcdef");
}

fn events(vt_cols: u16, rows: u16) -> (Vt, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let vt = Vt::new(vt_cols, rows, move |e| {
        let s = match e {
            VtEvent::Cwd(p) => format!("cwd {p}"),
            VtEvent::CommandFinished(c) => format!("exit {c:?}"),
            _ => return,
        };
        l.lock().push(s);
    });
    (vt, log)
}

#[test]
fn osc_133_and_7_split_across_chunks() {
    let (vt, log) = events(20, 3);
    vt.feed(b"\x1b]7;file://mac/Users/me/My%20Dir\x1b");
    vt.feed(b"\\\x1b]133;A\x07$ ls\r\n\x1b]13");
    vt.feed(b"3;D;2\x07");
    assert_eq!(*log.lock(), ["cwd /Users/me/My Dir", "exit Some(2)"]);
    assert_eq!(vt.last_exit(), Some(2));
    // The OSCs are invisible; the text is untouched.
    assert_eq!(vt.text(), "$ ls");
}

// Select command output: OSC 133 C/D bound a command's output exactly, so an agent's last
// answer can be copied without dragging over the prompt, the command or the next prompt.
fn prompt_cycle(vt: &Vt, cmd: &str, out: &str, code: i32) {
    vt.feed(
        format!("\x1b]133;A\x07$ \x1b]133;B\x07{cmd}\r\n\x1b]133;C\x07{out}\x1b]133;D;{code}\x07")
            .as_bytes(),
    );
}

#[test]
fn selects_the_output_of_the_last_finished_command() {
    let (vt, _) = events(20, 4);
    prompt_cycle(&vt, "ls", "a.txt\r\nb.txt\r\n", 0);
    prompt_cycle(&vt, "cat x", "line1\r\nline2\r\nline3\r\n", 0);
    vt.feed(b"\x1b]133;A\x07$ ");
    assert!(vt.select_last_output());
    assert_eq!(
        vt.end_selection().unwrap().trim_end(),
        "line1\nline2\nline3"
    );
    // Asking again from the next prompt walks to the one before.
    assert!(vt.select_last_output());
    assert_eq!(vt.end_selection().unwrap().trim_end(), "a.txt\nb.txt");
}

#[test]
fn select_output_has_nothing_to_select_without_marks_or_output() {
    let (vt, _) = events(20, 4);
    vt.feed(b"plain text\r\n");
    assert!(!vt.select_last_output(), "no OSC 133: nothing is bounded");
    // A command with no output (`true`) selects nothing and keeps looking for older output.
    prompt_cycle(&vt, "ls", "a.txt\r\n", 0);
    prompt_cycle(&vt, "true", "", 0);
    vt.feed(b"\x1b]133;A\x07$ ");
    assert!(vt.select_last_output());
    assert_eq!(vt.end_selection().unwrap().trim_end(), "a.txt");
}

#[test]
fn select_output_skips_a_running_command_and_survives_scrollback() {
    let (vt, _) = events(20, 3);
    prompt_cycle(&vt, "seq 5", "1\r\n2\r\n3\r\n4\r\n5\r\n", 0);
    // A command that has started (C) but not ended (no D) has no complete output yet.
    vt.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07sleep 9\r\n\x1b]133;C\x07partial\r\n");
    assert!(vt.select_last_output());
    assert_eq!(vt.end_selection().unwrap().trim_end(), "1\n2\n3\n4\n5");
    // Not on the alternate screen: a full-screen program owns it.
    vt.feed(b"\x1b[?1049h");
    assert!(!vt.select_last_output());
}

// ⌘K erases the scrollback the marks point into; the walk must not carry over to new output
// (a stale "already selected" position would hide every later command's output).
#[test]
fn select_output_after_clear_screen_starts_over() {
    let (vt, _) = events(20, 4);
    prompt_cycle(&vt, "seq 3", "1\r\n2\r\n3\r\n", 0);
    vt.feed(b"\x1b]133;A\x07$ ");
    assert!(vt.select_last_output());
    assert!(vt.clear_screen());
    prompt_cycle(&vt, "echo hi", "hi\r\n", 0);
    vt.feed(b"\x1b]133;A\x07$ ");
    assert!(vt.select_last_output());
    assert_eq!(vt.end_selection().unwrap().trim_end(), "hi");
}

#[test]
fn jumps_between_prompts_in_scrollback() {
    let (vt, _) = events(10, 2);
    for i in 0..3 {
        vt.feed(format!("\x1b]133;A\x07p{i}\r\nout\r\nout\r\n").as_bytes());
    }
    // 10 lines on a 2-row screen: p0/p1/p2 sit at scrollback offsets 8/5/2
    // (`text()` reads the live screen, so the viewport is checked by offset).
    for want in [2, 5, 8] {
        assert!(vt.jump_to_prompt(true));
        assert_eq!(vt.scrollbar().0, want);
    }
    assert!(!vt.jump_to_prompt(true));
    assert!(vt.jump_to_prompt(false));
    assert_eq!(vt.scrollbar().0, 5);
}

#[test]
fn mark_inside_synchronized_update_lands_on_its_line() {
    let (vt, _) = events(10, 2);
    // a, b, p, x, y: `p` is line 2 of 5, i.e. offset 1 from the bottom.
    vt.feed(b"\x1b[?2026ha\r\nb\r\n\x1b]133;A\x07p\r\nx\r\ny\x1b[?2026l");
    assert!(vt.jump_to_prompt(true));
    assert_eq!(vt.scrollbar().0, 1);
    let (vt, log) = events(10, 2);
    vt.feed(b"\x1b]7;file://h/a;b\x07");
    assert_eq!(*log.lock(), ["cwd /a;b"]);
    // OSC 7 inside an update does not release the buffered frame.
    vt.feed(b"\x1b[?2026hframe\x1b]7;file://h/c\x07");
    assert_eq!(vt.text(), "");
    vt.feed(b"\x1b[?2026l");
    assert_eq!(vt.text(), "frame");
}

#[test]
fn cursor_shape_focus_and_blink() {
    let vt = Vt::new(10, 2, |_| {});
    let mut f = Frame {
        focused: true,
        blink_on: true,
        ..Frame::default()
    };
    // DECSCUSR 6: steady beam → bars, cell colors untouched.
    vt.feed(b"\x1b[6 q");
    vt.snapshot(&pal(), &mut f);
    assert_eq!(f.cursor.map(|c| c.bars), Some(CursorBars::Beam));
    assert!(!f.cursor_blinks);
    // Unfocused: hollow block regardless of shape.
    f.focused = false;
    vt.snapshot(&pal(), &mut f);
    assert_eq!(f.cursor.map(|c| c.bars), Some(CursorBars::Hollow));
    // DECSCUSR 1 blinking block: off phase hides it.
    vt.feed(b"\x1b[1 q");
    f.focused = true;
    f.blink_on = false;
    vt.snapshot(&pal(), &mut f);
    assert!(f.cursor_blinks);
    assert!(f.cursor.is_none());
    assert_ne!(f.bg[0], rgba(pal().base[256]));
}

#[test]
fn synchronized_output_holds_until_end_or_flush() {
    let vt = Vt::new(10, 2, |_| {});
    vt.feed(b"\x1b[?2026hhidden");
    assert_eq!(vt.text(), "");
    assert!(vt.sync_deadline().is_some());
    vt.flush_sync();
    assert_eq!(vt.text(), "hidden");
    vt.feed(b"\x1b[?2026h!\x1b[?2026l");
    assert_eq!(vt.text(), "hidden!");
}

fn snap(vt: &Vt) -> Frame {
    let mut f = Frame::default();
    vt.snapshot(&pal(), &mut f);
    f
}

#[test]
fn kitty_image_anchors_at_cursor_and_moves_it_past() {
    let vt = Vt::new(20, 5, |_| {});
    vt.set_cell_px(10.0, 20.0);
    let px = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![0u8; 20 * 40 * 3],
    );
    // 20×40 px = 2×2 cells at 10×20 px per cell.
    vt.feed(format!("ab\x1b_Ga=T,f=24,s=20,v=40,i=1,q=2;{px}\x1b\\x").as_bytes());
    let f = snap(&vt);
    assert_eq!(f.images.len(), 1);
    let im = &f.images[0];
    assert_eq!((im.col, im.row, im.cols, im.rows), (2.0, 0.0, 2.0, 2.0));
    // Cursor went to the last image row, after its last column: `x` at (4, 1).
    assert_eq!(vt.text(), "ab\n    x");
}

#[test]
fn sixel_lands_below_and_alt_screen_images_go_away() {
    let vt = Vt::new(20, 5, |_| {});
    vt.set_cell_px(10.0, 20.0);
    // 2×6 px red sixel: one cell row; the cursor ends on the next line.
    vt.feed(b"\x1bPq#0;2;100;0;0~~\x1b\\y");
    assert_eq!(snap(&vt).images.len(), 1);
    assert_eq!(vt.text(), "\ny");
    vt.feed(b"\x1b[?1049h\x1bPq~\x1b\\");
    assert_eq!(snap(&vt).images.len(), 1, "only the alt-screen image shows");
    vt.feed(b"\x1b[?1049l");
    let f = snap(&vt);
    assert_eq!(f.images.len(), 1, "primary image back, alt image dropped");
    assert_eq!(f.images[0].row, 0.0);
}

#[test]
fn osc_inside_dcs_is_not_a_prompt_mark() {
    let (vt, _) = events(10, 2);
    vt.feed(b"\x1bPq\x1b]133;A\x07\x1b\\");
    vt.feed(b"\r\n\r\n\r\n");
    assert!(!vt.jump_to_prompt(true));
}

/// `(title, body)` of each `VtEvent::Notify` seen.
type NoticeLog = Arc<Mutex<Vec<(Option<String>, String)>>>;

fn notifications(vt_cols: u16, rows: u16) -> (Vt, NoticeLog) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let vt = Vt::new(vt_cols, rows, move |e| {
        if let VtEvent::Notify { title, body } = e {
            l.lock().push((title, body));
        }
    });
    (vt, log)
}

// Desktop notifications (Ghostty: OSC 9 and OSC 777;notify). An agent that rings these when it
// finishes must reach the app, split across reads and with either terminator.
#[test]
fn osc_9_and_777_raise_notifications_without_touching_the_text() {
    let (vt, log) = notifications(20, 2);
    vt.feed(b"a\x1b]9;build done\x07b");
    vt.feed(b"\x1b]777;notify;Claude;needs you\x1b");
    vt.feed(b"\\c\x1b]777;notify;;body only\x07");
    assert_eq!(
        *log.lock(),
        [
            (None, "build done".to_string()),
            (Some("Claude".to_string()), "needs you".to_string()),
            (None, "body only".to_string()),
        ]
    );
    assert_eq!(vt.text(), "abc");
}

// Edge cases of the same contract: ConEmu progress (`9;4;…`) and other 777 verbs are not
// notifications; a body with `;` keeps its tail; control bytes never reach the OS banner.
#[test]
fn notifications_ignore_progress_and_unknown_verbs_and_keep_semicolons() {
    let (vt, log) = notifications(20, 2);
    vt.feed(b"\x1b]9;4;1;50\x07");
    vt.feed(b"\x1b]777;preexec\x07");
    vt.feed(b"\x1b]9;\x07");
    vt.feed(b"\x1b]777;notify;T;a;b;c\x07");
    vt.feed(b"\x1b]9;hi\x08\x7fthere\x07");
    assert_eq!(
        *log.lock(),
        [
            (Some("T".to_string()), "a;b;c".to_string()),
            (None, "hithere".to_string()),
        ]
    );
}

#[test]
fn select_all_covers_scrollback_and_screen() {
    let vt = Vt::new(4, 2, |_| {});
    vt.feed(b"aa\r\nbb\r\ncc\r\ndd");
    vt.select_all();
    assert_eq!(vt.end_selection().unwrap().trim_end(), "aa\nbb\ncc\ndd");
}

#[test]
fn scroll_to_top_reaches_the_oldest_line() {
    let vt = Vt::new(4, 2, |_| {});
    vt.feed(b"a\r\nb\r\nc\r\nd\r\ne");
    assert_eq!(vt.scrollbar().0, 0);
    vt.scroll_to_top();
    let (off, max) = vt.scrollbar();
    assert_eq!((off, max), (3, 3));
    vt.scroll_to_bottom();
    assert_eq!(vt.scrollbar().0, 0);
}

#[test]
fn clear_screen_erases_scrollback_and_marks_but_not_the_alt_screen() {
    let vt = Vt::new(6, 2, |_| {});
    vt.feed(b"\x1b]133;A\x07one\r\ntwo\r\nthree");
    assert!(vt.scrollbar().1 > 0);
    assert!(vt.clear_screen());
    assert_eq!(vt.scrollbar().1, 0);
    assert_eq!(vt.text().trim(), "");
    assert!(!vt.jump_to_prompt(true), "marks point at erased lines");
    // A full-screen program owns the alternate screen: left alone.
    vt.feed(b"\x1b[?1049hvim");
    assert!(!vt.clear_screen());
    assert_eq!(vt.text().trim(), "vim");
}

#[test]
fn click_to_move_only_while_a_prompt_is_being_edited() {
    let vt = Vt::new(20, 4, |_| {});
    // No OSC 133 at all: never move.
    vt.feed(b"$ echo hi");
    assert_eq!(vt.click_to_move(2, 0), None);
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(b"\x1b]133;A\x07$ echo hi");
    // The cursor is at column 9 of row 0: a click at column 4 is 5 cells left.
    assert_eq!(vt.click_to_move(4, 0), Some((-5, 0)));
    assert_eq!(vt.click_to_move(12, 0), Some((3, 0)));
    // Below the cursor's line is not the prompt.
    assert_eq!(vt.click_to_move(4, 2), None);
    // The command runs: arrows would reach the program, not the line editor.
    vt.feed(b"\x1b]133;C\x07\r\nout");
    assert_eq!(vt.click_to_move(4, 1), None);
    // Finished, and the next prompt is drawn at row 2.
    vt.feed(b"\x1b]133;D;0\x07\r\n\x1b]133;A\x07$ ");
    assert_eq!(vt.click_to_move(0, 2), Some((-2, 0)));
    // Rows above the prompt's first line are not editable.
    assert_eq!(vt.click_to_move(0, 0), None);
    // Full-screen programs never get click-to-move.
    vt.feed(b"\x1b[?1049h");
    assert_eq!(vt.click_to_move(0, 0), None);
}

#[test]
fn reports_text_area_pixels() {
    let out = Arc::new(Mutex::new(String::new()));
    let o = out.clone();
    let vt = Vt::new(20, 5, move |e| {
        if let VtEvent::Write(s) = e {
            o.lock().push_str(&s);
        }
    });
    vt.set_cell_px(10.0, 20.0);
    vt.feed(b"\x1b[14t");
    assert_eq!(*out.lock(), "\x1b[4;100;200t");
}

fn lines(n: usize) -> String {
    (0..n).map(|i| format!("line{i}\r\n")).collect()
}

#[test]
fn search_finds_matches_across_scrollback_and_wraps() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(lines(30).as_bytes());
    // `line1`, `line10`..`line19` all contain "line1": 11 matches, 8 in history.
    let s = vt.search("line1", false, false, true).unwrap();
    assert_eq!(s.total, 11);
    assert_eq!(s.index, 1);
    // The first match is far above the live screen: the viewport followed it.
    assert!(vt.scrollbar().0 > 0);
    let s = vt.search("line1", false, false, true).unwrap();
    assert_eq!(s.index, 2);
    // Backwards from the first wraps to the last.
    vt.search("line1", false, false, false).unwrap();
    let s = vt.search("line1", false, false, false).unwrap();
    assert_eq!((s.index, s.total), (11, 11));
    let s = vt.search("line1", false, false, true).unwrap();
    assert_eq!(s.index, 1, "forward from the last wraps to the first");
}

#[test]
fn search_regex_case_and_literal_modes() {
    let vt = Vt::new(30, 4, |_| {});
    vt.feed(b"Foo foo FOO a.c abc\r\n");
    assert_eq!(vt.search("foo", false, false, true).unwrap().total, 3);
    assert_eq!(vt.search("foo", false, true, true).unwrap().total, 1);
    // Literal mode escapes the dot; regex mode does not.
    assert_eq!(vt.search("a.c", false, false, true).unwrap().total, 1);
    assert_eq!(vt.search("a.c", true, false, true).unwrap().total, 2);
    assert_eq!(vt.search(r"fo+ ", true, false, true).unwrap().total, 3);
    assert_eq!(vt.search(r"fo+ ", true, true, true).unwrap().total, 1);
    assert!(vt.search("(", true, false, true).is_err());
    assert_eq!(vt.search("nope", false, false, true).unwrap().total, 0);
    vt.clear_search();
    assert_eq!(
        vt.search("", false, false, true).unwrap(),
        SearchStat::default()
    );
}

#[test]
fn search_highlights_visible_matches_and_follows_output() {
    let vt = Vt::new(20, 3, |_| {});
    vt.feed(b"ab ab\r\n");
    vt.search("ab", false, false, true).unwrap();
    let mut f = Frame {
        search_color: [255, 0, 0, 255],
        search_current_color: [0, 255, 0, 255],
        ..Frame::default()
    };
    vt.snapshot(&pal(), &mut f);
    assert_eq!(f.bg[0], [0, 255, 0, 255], "current match");
    assert_eq!(f.bg[3], [255, 0, 0, 255], "other match");
    assert_ne!(f.bg[2], [255, 0, 0, 255]);
    // New output re-runs the search on the next frame.
    vt.feed(b"ab\r\n");
    vt.snapshot(&pal(), &mut f);
    assert_eq!(f.bg[20 + 1], f.bg[20], "second line matched too");
    assert_ne!(f.bg[20], f.clear);
}

#[test]
fn plain_urls_are_found_and_underlined_on_hover() {
    let vt = Vt::new(40, 3, |_| {});
    vt.feed(b"see (https://a.dev/x?q=1). ok http://b.io, end\r\n");
    assert_eq!(vt.url_at(8, 0).as_deref(), Some("https://a.dev/x?q=1"));
    assert_eq!(vt.url_at(0, 0), None);
    assert_eq!(vt.url_at(34, 0).as_deref(), Some("http://b.io"));
    assert!(vt.set_hover(Some((8, 0))));
    assert!(!vt.set_hover(Some((9, 0))), "same URL: no change");
    let f = snap(&vt);
    let ul: Vec<u16> = f.decos.iter().map(|d| d.col).collect();
    assert_eq!(ul.len(), "https://a.dev/x?q=1".len());
    assert_eq!(ul[0], 5);
    assert!(vt.set_hover(None));
    assert!(snap(&vt).decos.is_empty());
}

#[test]
fn urls_follow_soft_wrapped_lines() {
    let vt = Vt::new(10, 4, |_| {});
    vt.feed(b"https://example.com/path");
    // Wrapped over three rows; every row resolves to the whole URL.
    for row in 0..3 {
        assert_eq!(
            vt.url_at(2, row).as_deref(),
            Some("https://example.com/path"),
            "row {row}"
        );
    }
}

// ⌘-click on `src/a.ts:12:3` in compiler or agent output: the terminal hands the token under
// the pointer to the app, which decides whether it is a file of the worktree.
#[test]
fn path_token_under_the_pointer_keeps_line_and_column() {
    let vt = Vt::new(60, 3, |_| {});
    vt.feed(b"error: src/lib/a.ts:12:3 - bad (see ./b.rs:7). also \"c d/e.go\"\r\n");
    assert_eq!(vt.path_at(10, 0).as_deref(), Some("src/lib/a.ts:12:3"));
    assert_eq!(vt.path_at(23, 0).as_deref(), Some("src/lib/a.ts:12:3"));
    // Brackets and the sentence's period around a token are not part of it.
    assert_eq!(vt.path_at(40, 0).as_deref(), Some("./b.rs:7"));
    // Blank cells and rows with no text are nothing.
    assert_eq!(vt.path_at(24, 0), None);
    assert_eq!(vt.path_at(0, 2), None);
    // The closing quote of `"c d/e.go"` is not part of the token.
    assert_eq!(vt.path_at(58, 0).as_deref(), Some("d/e.go"));
}

#[test]
fn path_token_follows_soft_wrapped_lines_and_wide_cells() {
    let vt = Vt::new(10, 4, |_| {});
    vt.feed(b"x packages/core/src/main.rs:99\r\n");
    // The path wraps over rows 0..=2 (cols 10): every row of it resolves to the whole token.
    for row in 0..3 {
        assert_eq!(
            vt.path_at(4, row).as_deref(),
            Some("packages/core/src/main.rs:99"),
            "row {row}"
        );
    }
    // A wide char before it must not shift the columns.
    let vt = Vt::new(30, 2, |_| {});
    vt.feed("日本 src/x.ts:1".as_bytes());
    assert_eq!(vt.path_at(6, 0).as_deref(), Some("src/x.ts:1"));
    assert_eq!(vt.path_at(1, 0).as_deref(), Some("日本"));
}

#[test]
fn osc8_link_wins_over_text() {
    let vt = Vt::new(40, 2, |_| {});
    vt.feed(b"\x1b]8;;https://real.dev\x1b\\click\x1b]8;;\x1b\\");
    assert_eq!(vt.url_at(1, 0).as_deref(), Some("https://real.dev"));
}

#[test]
fn clusters_expose_base_plus_zerowidth_and_wide() {
    let vt = Vt::new(10, 2, |_| {});
    // e + combining acute; thumbs-up + skin tone (zerowidth modifier in alacritty is
    // its own wide cell, so check the ZWJ/VS16 forms): heart + VS16, a + ZWJ.
    vt.feed("e\u{301}x\u{2764}\u{fe0f}\u{1f600}".as_bytes());
    let f = snap(&vt);
    let g = |col| f.glyphs.iter().find(|g| g.col == col).unwrap();
    assert_eq!((g(0).ch, g(0).extra.as_deref()), ('e', Some("\u{301}")));
    assert!(!g(0).wide);
    assert_eq!(g(1).extra, None);
    assert_eq!(g(2).extra.as_deref(), Some("\u{fe0f}"));
    let emoji = f.glyphs.iter().find(|g| g.ch == '\u{1f600}').unwrap();
    assert!(emoji.wide);
}

#[test]
fn answers_grapheme_cluster_mode_query() {
    let out = Arc::new(Mutex::new(String::new()));
    let o = out.clone();
    let vt = Vt::new(10, 2, move |e| {
        if let VtEvent::Write(s) = e {
            o.lock().push_str(&s);
        }
    });
    vt.feed(b"\x1b[?2027$p");
    assert_eq!(*out.lock(), "\x1b[?2027;3$y");
}

// #97: the viewport snapped to the bottom on every pty chunk, so nobody could scroll up
// while a program kept printing. Output now leaves a scrolled-up viewport where it is;
// the desktop snaps on keystroke/paste (Ghostty `scroll-to-bottom = keystroke`).
fn numbered(from: usize, n: usize) -> Vec<u8> {
    (from..from + n)
        .flat_map(|i| format!("line {i}\r\n").into_bytes())
        .collect()
}

/// The rows the user sees (scrolled into history), unlike `text()` (the live screen).
fn viewport(vt: &Vt) -> String {
    let t = vt.term.lock();
    let off = t.grid().display_offset() as i32;
    (0..t.screen_lines() as i32)
        .map(|r| {
            let row = &t.grid()[Line(r - off)];
            (0..t.columns())
                .map(|c| row[Column(c)].c)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn output_keeps_a_scrolled_up_viewport_in_place() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    vt.scroll(5);
    assert_eq!(vt.scrollbar().0, 5);
    let seen = viewport(&vt);
    vt.feed(&numbered(40, 3));
    assert_eq!(vt.scrollbar().0, 8);
    assert_eq!(viewport(&vt), seen);
}

#[test]
fn output_at_the_bottom_keeps_following() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    vt.feed(&numbered(40, 3));
    assert_eq!(vt.scrollbar().0, 0);
    assert!(vt.text().contains("line 42"), "{}", vt.text());
}

#[test]
fn cursor_only_redraw_does_not_move_the_viewport() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    vt.scroll(5);
    vt.feed(b"\x1b[s\x1b[2;1Hxyz\x1b[u");
    assert_eq!(vt.scrollbar().0, 5);
    vt.feed(&[]);
    assert_eq!(vt.scrollbar().0, 5);
}

#[test]
fn resize_while_scrolled_clamps_the_offset() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    vt.scroll(10_000);
    vt.resize(20, 8);
    let (off, max) = vt.scrollbar();
    assert!(off <= max, "{off} > {max}");
    assert!(!vt.text().is_empty());
}

#[test]
fn scroll_delta_clamps_at_both_ends_and_scroll_to_bottom_snaps() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    vt.scroll(10_000);
    let (off, max) = vt.scrollbar();
    assert_eq!(off, max);
    vt.scroll(-10_000);
    assert_eq!(vt.scrollbar().0, 0);
    vt.scroll(5);
    vt.scroll_to_bottom();
    assert_eq!(vt.scrollbar().0, 0);
}

#[test]
fn search_recenter_keeps_a_live_screen_match_visible() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 39));
    vt.feed(b"zzz");
    vt.scroll(10);
    vt.search("zzz", false, false, true).unwrap();
    assert_eq!(vt.scrollbar().0, 0);
}

// A GPU tab reattached after a hostd restart gets the ring replayed from scratch.
#[test]
fn reset_clears_screen_scrollback_and_marks() {
    let vt = Vt::new(20, 4, |_| {});
    vt.feed(&numbered(0, 40));
    prompt_cycle(&vt, "ls", "out", 0);
    vt.reset();
    assert_eq!(vt.text(), "");
    assert_eq!(vt.scrollbar(), (0, 0));
    assert!(!vt.select_last_output());
}

// TUIs probing OSC 10/11/4 for dark/light waited for a timeout: the query was dropped.
#[test]
fn answers_osc_10_11_and_4_color_queries() {
    let out = Arc::new(Mutex::new(String::new()));
    let o = out.clone();
    let vt = Vt::new(10, 2, move |e| {
        if let VtEvent::Write(s) = e {
            o.lock().push_str(&s);
        }
    });
    vt.feed(b"\x1b]11;?\x1b\\");
    assert_eq!(*out.lock(), "", "no palette yet: no answer");
    let mut p = pal();
    p.base[NamedColor::Background as usize] = Rgb {
        r: 0x10,
        g: 0x10,
        b: 0x10,
    };
    vt.set_palette(&p);
    vt.feed(b"\x1b]11;?\x1b\\");
    assert_eq!(*out.lock(), "\x1b]11;rgb:1010/1010/1010\x1b\\");
    out.lock().clear();
    vt.feed(b"\x1b]10;?\x07\x1b]4;1;?\x07");
    let fg = p.base[NamedColor::Foreground as usize];
    let c1 = p.base[1];
    assert_eq!(
        *out.lock(),
        format!(
            "\x1b]10;rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}\x07\x1b]4;1;rgb:{3:02x}{3:02x}/{4:02x}{4:02x}/{5:02x}{5:02x}\x07",
            fg.r, fg.g, fg.b, c1.r, c1.g, c1.b
        )
    );
}
