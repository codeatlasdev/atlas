//! ANSI text → styled spans, with the fidelity of a terminal for log output:
//! SGR bold/dim/italic/underline (plain, double `21`, `4:0-5` styles)/blink/
//! reverse/hidden/strike, 16/256/truecolor foreground, background and
//! underline color (`58`), the matching resets, both `;` and `:` parameter
//! separators, and OSC 8 hyperlinks. Everything else (cursor moves, erase,
//! other OSC) is dropped. Truncated sequences never panic or leak bytes.

use ratatui::prelude::*;

use crate::theme;

/// A `[start, end)` byte range of the *visible* text that is an OSC 8 link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub url: String,
}

/// Styled spans plus the OSC 8 links found in them (byte ranges of the
/// concatenated span text).
#[derive(Debug, Default)]
pub struct Parsed {
    pub spans: Vec<Span<'static>>,
    pub links: Vec<Link>,
}

/// Parse `text`; `base` is what SGR 0 returns to.
pub fn parse(t: &theme::Theme, text: &str, base: Style) -> Parsed {
    let mut out = Parsed::default();
    let mut style = base;
    let mut visible = 0usize; // bytes of visible text emitted so far
    let mut open_link: Option<(usize, String)> = None;
    let mut rest = text;

    let emit = |chunk: &str, style: Style, visible: &mut usize, out: &mut Parsed| {
        if !chunk.is_empty() {
            out.spans.push(Span::styled(chunk.to_owned(), style));
            *visible += chunk.len();
        }
    };

    while let Some(esc) = rest.find('\x1b') {
        emit(&rest[..esc], style, &mut visible, &mut out);
        rest = &rest[esc + 1..];
        match rest.as_bytes().first() {
            // CSI: ESC [ params final
            Some(b'[') => {
                rest = &rest[1..];
                let end = rest
                    .bytes()
                    .position(|b| (0x40..=0x7e).contains(&b))
                    .unwrap_or(rest.len());
                if end == rest.len() {
                    rest = ""; // truncated sequence: drop it
                    break;
                }
                let params = &rest[..end];
                let fin = rest.as_bytes()[end];
                rest = &rest[end + 1..];
                if fin == b'm' {
                    apply_sgr(t, params, base, &mut style);
                }
            }
            // OSC: ESC ] ... (BEL | ESC \)
            Some(b']') => {
                rest = &rest[1..];
                let (body, after) = split_string_terminator(rest);
                rest = after;
                if let Some(uri) = body.strip_prefix("8;") {
                    // `8;params;URI` — empty URI closes the link.
                    let url = uri.split_once(';').map_or("", |(_, u)| u);
                    if let Some((start, url)) = open_link.take()
                        && visible > start
                    {
                        out.links.push(Link {
                            start,
                            end: visible,
                            url,
                        });
                    }
                    if !url.is_empty() {
                        open_link = Some((visible, url.to_string()));
                    }
                }
            }
            // DCS / SOS / PM / APC strings: skip to the terminator.
            Some(b'P' | b'X' | b'^' | b'_') => {
                let (_, after) = split_string_terminator(&rest[1..]);
                rest = after;
            }
            // nF escapes (ESC ( B, ESC # 8 …): intermediates 0x20-0x2F then a
            // final byte; plain two-byte escapes (ESC c, ESC 7) have no intermediate.
            Some(_) => {
                let b = rest.as_bytes();
                let mut n = b.iter().take_while(|c| (0x20..=0x2f).contains(*c)).count();
                if n < b.len() {
                    n += rest[n..].chars().next().map_or(0, char::len_utf8);
                }
                rest = &rest[n..];
            }
            None => {}
        }
    }
    emit(rest, style, &mut visible, &mut out);
    if let Some((start, url)) = open_link
        && visible > start
    {
        out.links.push(Link {
            start,
            end: visible,
            url,
        });
    }
    out
}

/// `body` of a string control sequence and the text after its terminator
/// (BEL or ST = ESC `\`). An unterminated string swallows the rest.
fn split_string_terminator(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == 0x07 {
            return (&s[..i], &s[i + 1..]);
        }
        if b == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
            return (&s[..i], &s[i + 2..]);
        }
    }
    (s, "")
}

/// Apply one SGR parameter string to `style`.
fn apply_sgr(t: &theme::Theme, params: &str, base: Style, style: &mut Style) {
    if params.is_empty() {
        *style = base;
        return;
    }
    // Each `;`-separated group may carry `:` sub-parameters (4:3, 38:2::r:g:b).
    let groups: Vec<Vec<u16>> = params
        .split(';')
        .map(|g| {
            g.split(':')
                .map(|p| p.parse::<u16>().unwrap_or(0))
                .collect()
        })
        .collect();
    let mut i = 0;
    while i < groups.len() {
        let g = &groups[i];
        let code = g[0];
        match code {
            0 => *style = base,
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => {
                // 4:0 turns underline off; every other style draws as underline.
                if g.get(1) == Some(&0) {
                    *style = style.remove_modifier(Modifier::UNDERLINED);
                } else {
                    *style = style.add_modifier(Modifier::UNDERLINED);
                }
            }
            5 | 6 => *style = style.add_modifier(Modifier::SLOW_BLINK),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            8 => *style = style.add_modifier(Modifier::HIDDEN),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            21 => *style = style.add_modifier(Modifier::UNDERLINED),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            25 => *style = style.remove_modifier(Modifier::SLOW_BLINK),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            28 => *style = style.remove_modifier(Modifier::HIDDEN),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => *style = style.fg(std_color(t, code - 30, false)),
            90..=97 => *style = style.fg(std_color(t, code - 90, true)),
            40..=47 => *style = style.bg(std_color(t, code - 40, false)),
            100..=107 => *style = style.bg(std_color(t, code - 100, true)),
            39 => style.fg = base.fg,
            49 => style.bg = base.bg,
            59 => style.underline_color = None,
            38 | 48 | 58 => {
                let (color, used) = extended_color(t, &groups, i);
                if let Some(c) = color {
                    match code {
                        38 => *style = style.fg(c),
                        48 => *style = style.bg(c),
                        _ => *style = style.underline_color(c),
                    }
                }
                i += used;
            }
            _ => {}
        }
        i += 1;
    }
}

/// Color of a `38/48/58` group at `groups[i]`, and how many *extra* groups it
/// consumed (`;`-form) — the `:` form lives entirely inside the group.
fn extended_color(t: &theme::Theme, groups: &[Vec<u16>], i: usize) -> (Option<Color>, usize) {
    let g = &groups[i];
    if g.len() > 1 {
        // 38:5:n  /  38:2:r:g:b  /  38:2::r:g:b (colorspace slot present).
        return match g.get(1) {
            Some(5) => (g.get(2).map(|&n| color_256(t, n)), 0),
            Some(2) => {
                let rgb = if g.len() >= 6 { &g[3..6] } else { &g[2..] };
                (rgb_of(rgb), 0)
            }
            _ => (None, 0),
        };
    }
    match groups.get(i + 1).map(|g| g[0]) {
        Some(5) => (groups.get(i + 2).map(|g| color_256(t, g[0])), 2),
        Some(2) => {
            let rgb: Vec<u16> = groups[i + 2..].iter().take(3).map(|g| g[0]).collect();
            (rgb_of(&rgb), 4.min(groups.len() - 1 - i))
        }
        _ => (None, 0),
    }
}

fn rgb_of(v: &[u16]) -> Option<Color> {
    (v.len() >= 3).then(|| Color::Rgb(v[0] as u8, v[1] as u8, v[2] as u8))
}

fn std_color(t: &theme::Theme, idx: u16, bright: bool) -> Color {
    t.ansi16
        .get(idx as usize + if bright { 8 } else { 0 })
        .copied()
        .unwrap_or(Color::Reset)
}

/// 256-color index → color: 0-15 the theme's palette, 16-231 the 6×6×6 cube
/// (xterm levels 0,95,135,175,215,255), 232-255 the grayscale ramp.
pub fn color_256(t: &theme::Theme, n: u16) -> Color {
    match n {
        0..=15 => std_color(t, n % 8, n >= 8),
        16..=231 => {
            let n = n - 16;
            let level = |c: u16| if c == 0 { 0 } else { (55 + c * 40) as u8 };
            Color::Rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        232..=255 => {
            let v = ((n - 232) * 10 + 8) as u8;
            Color::Rgb(v, v, v)
        }
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Parsed {
        parse(theme::current(), text, Style::default())
    }

    fn plain(p: &Parsed) -> String {
        p.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_text_and_non_sgr_sequences_are_dropped() {
        let r = p("a\x1b[2Kb\x1b[1;1Hc\x1b]0;title\x07d\x1b(Be");
        assert_eq!(plain(&r), "abcde");
    }

    #[test]
    fn truncated_sequences_never_panic_or_leak() {
        for s in [
            "\x1b",
            "\x1b[",
            "\x1b[3",
            "\x1b]8;;http://x",
            "x\x1b[38;2;1;2",
        ] {
            let r = p(s);
            assert!(!plain(&r).contains('\x1b'), "{s:?}");
        }
    }

    #[test]
    fn basic_modifiers_and_resets() {
        let r = p("\x1b[1;3;9;7mx\x1b[22;23;29;27my");
        let (a, b) = (r.spans[0].style, r.spans[1].style);
        for m in [
            Modifier::BOLD,
            Modifier::ITALIC,
            Modifier::CROSSED_OUT,
            Modifier::REVERSED,
        ] {
            assert!(a.add_modifier.contains(m));
            assert!(!b.add_modifier.contains(m) || b.sub_modifier.contains(m));
        }
    }

    #[test]
    fn underline_styles_double_and_color() {
        let r =
            p("\x1b[4:3mcurly\x1b[24m|\x1b[21mdouble\x1b[4:0m|\x1b[4;58;2;255;0;0mred\x1b[59mx");
        assert!(r.spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(!r.spans[1].style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(r.spans[2].style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(!r.spans[3].style.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(
            r.spans[4].style.underline_color,
            Some(Color::Rgb(255, 0, 0))
        );
        assert_eq!(r.spans[5].style.underline_color, None);
    }

    #[test]
    fn colors_256_truecolor_both_separator_forms() {
        let t = theme::current();
        let c = |s: &str| p(s).spans[0].style;
        assert_eq!(c("\x1b[38;5;196mx").fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(c("\x1b[38:5:196mx").fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(c("\x1b[38;2;1;2;3mx").fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(c("\x1b[38:2:1:2:3mx").fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(c("\x1b[38:2::1:2:3mx").fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(c("\x1b[48;2;9;8;7mx").bg, Some(Color::Rgb(9, 8, 7)));
        assert_eq!(c("\x1b[48;5;232mx").bg, Some(Color::Rgb(8, 8, 8)));
        assert_eq!(c("\x1b[31mx").fg, Some(t.ansi16[1]));
        assert_eq!(c("\x1b[91mx").fg, Some(t.ansi16[9]));
        assert_eq!(c("\x1b[41mx").bg, Some(t.ansi16[1]));
        assert_eq!(c("\x1b[105mx").bg, Some(t.ansi16[13]));
        // xterm cube level 1 is 95, not 51.
        assert_eq!(c("\x1b[38;5;17mx").fg, Some(Color::Rgb(0, 0, 95)));
    }

    #[test]
    fn extended_color_followed_by_more_params() {
        let s = p("\x1b[38;2;10;20;30;1;4mx").spans[0].style;
        assert_eq!(s.fg, Some(Color::Rgb(10, 20, 30)));
        assert!(
            s.add_modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
        let s = p("\x1b[38;5;9;1mx").spans[0].style;
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn reset_returns_to_base_and_39_49_restore_base_colors() {
        let base = Style::default().fg(Color::Rgb(5, 5, 5));
        let r = parse(theme::current(), "\x1b[31;44ma\x1b[39;49mb\x1b[0mc", base);
        assert_eq!(r.spans[1].style.fg, Some(Color::Rgb(5, 5, 5)));
        assert_eq!(r.spans[1].style.bg, None);
        assert_eq!(r.spans[2].style, base);
    }

    #[test]
    fn osc8_links_report_visible_byte_ranges() {
        let r = p(
            "see \x1b]8;;http://a.test/x\x1b\\here\x1b]8;;\x1b\\ and \x1b]8;id=1;file:///b\x07there\x1b]8;;\x07!",
        );
        assert_eq!(plain(&r), "see here and there!");
        assert_eq!(
            r.links,
            vec![
                Link {
                    start: 4,
                    end: 8,
                    url: "http://a.test/x".into()
                },
                Link {
                    start: 13,
                    end: 18,
                    url: "file:///b".into()
                },
            ]
        );
    }

    #[test]
    fn unterminated_link_closes_at_end_of_line() {
        let r = p("\x1b]8;;http://x\x07abc");
        assert_eq!(r.links.len(), 1);
        assert_eq!((r.links[0].start, r.links[0].end), (0, 3));
    }
}
