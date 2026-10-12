use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use super::spinner;
use crate::runtime::service::ServiceState;
use crate::theme;
use crate::tui::app::{Message, TabBadge};
use crate::tui::clicks::ClickMap;

/// Tab chip label: `"  {digit} {tab}  "` for tabs 0-9 (the jump keys), `"  {tab}  "` beyond.
pub fn tab_label(i: usize, tab: &str) -> String {
    if i < 10 {
        format!("  {i} {tab}  ")
    } else {
        format!("  {tab}  ")
    }
}

/// Extra columns after the label: status dot (`●`) and badge (` ✗3` / ` ◆`).
fn extra_width(has_dot: bool, badge: &TabBadge) -> usize {
    let dot = usize::from(has_dot);
    let b = if badge.errors > 0 {
        1 + 1 + badge.errors.to_string().len() // " ✗N"
    } else if badge.activity {
        2 // " ◆"
    } else {
        0
    };
    dot + b
}

/// Full width of one tab entry: label + dot + badge + 1 gap column.
fn tab_width(i: usize, tab: &str, has_dot: bool, badge: &TabBadge) -> usize {
    tab_label(i, tab).chars().count() + extra_width(has_dot, badge) + 1
}

/// Which tabs are fully visible, and how many are hidden on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabWindow {
    /// Index range of tabs drawn, all of them whole.
    pub first: usize,
    pub last_exclusive: usize,
    pub hidden_left: usize,
    pub hidden_right: usize,
}

/// Pick the widest run of whole tabs around `active` that fits in `avail`
/// columns, reserving room for `‹ +N` / `› +N` indicators when tabs are hidden.
pub fn window(widths: &[usize], active: usize, avail: usize) -> TabWindow {
    let n = widths.len();
    let total: usize = widths.iter().sum();
    if total <= avail || n == 0 {
        return TabWindow {
            first: 0,
            last_exclusive: n,
            hidden_left: 0,
            hidden_right: 0,
        };
    }
    let active = active.min(n - 1);
    // Indicator width for `hidden` tabs: "‹ +N " / " › +N".
    let ind = |hidden: usize| {
        if hidden == 0 {
            0
        } else {
            4 + hidden.to_string().len()
        }
    };
    // Grow a window around the active tab, alternating sides, while it still fits
    // together with the indicators the remaining hidden tabs would need.
    let (mut lo, mut hi) = (active, active + 1);
    let fits = |lo: usize, hi: usize| {
        let used: usize = widths[lo..hi].iter().sum();
        used + ind(lo) + ind(n - hi) <= avail
    };
    // The active tab itself must always be drawn whole; if even it can't fit with
    // indicators, it is still the only tab shown (clipped by the area, never mid-label
    // of a neighbour).
    let mut grow_left = true;
    loop {
        let can_left = lo > 0 && fits(lo - 1, hi);
        let can_right = hi < n && fits(lo, hi + 1);
        match (can_left, can_right) {
            (false, false) => break,
            (true, false) => lo -= 1,
            (false, true) => hi += 1,
            (true, true) => {
                if grow_left {
                    lo -= 1;
                } else {
                    hi += 1;
                }
                grow_left = !grow_left;
            }
        }
    }
    TabWindow {
        first: lo,
        last_exclusive: hi,
        hidden_left: lo,
        hidden_right: n - hi,
    }
}

/// Everything the tab bar draws from.
pub struct TabsView<'a> {
    pub tabs: &'a [String],
    pub active: usize,
    pub states: &'a [(String, ServiceState)],
    pub badges: &'a [TabBadge],
    pub tick: usize,
}

pub fn render(frame: &mut Frame, area: Rect, view: &TabsView, clicks: &ClickMap) {
    let TabsView {
        tabs,
        active,
        states,
        badges,
        tick,
    } = *view;
    let t = theme::current();
    let state_map: HashMap<&str, &ServiceState> =
        states.iter().map(|(n, s)| (n.as_str(), s)).collect();
    let badge_of = |i: usize| badges.get(i).copied().unwrap_or_default();
    let has_dot = |i: usize, tab: &str| i > 0 && state_map.contains_key(tab);

    let widths: Vec<usize> = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| tab_width(i, tab, has_dot(i, tab), &badge_of(i)))
        .collect();
    let avail = area.width.saturating_sub(1) as usize; // 1 col left padding
    let win = window(&widths, active, avail);

    let mut tab_spans: Vec<Span> = Vec::new();
    let mut under_spans: Vec<Span> = Vec::new();
    // Content starts after the 1-col left padding; `col` mirrors what is pushed.
    let mut col = area.x + 1;
    let hit = |x: u16, w: u16| Rect::new(x, area.y, w, 2).intersection(area);

    if win.hidden_left > 0 {
        let s = format!("‹ +{} ", win.hidden_left);
        let w = s.chars().count() as u16;
        tab_spans.push(Span::styled(s, Style::default().fg(t.text_dim)));
        under_spans.push(Span::raw(" ".repeat(w as usize)));
        clicks.click(hit(col, w), Message::PrevTab);
        col += w;
    }

    for i in win.first..win.last_exclusive {
        let tab = &tabs[i];
        let label = tab_label(i, tab);
        let label_width = label.chars().count();
        let badge = badge_of(i);
        let dot = has_dot(i, tab);
        let chip_w = (label_width + extra_width(dot, &badge)) as u16;
        clicks.click(hit(col, chip_w), Message::SwitchTab(i));
        col += widths[i] as u16;

        let label_style = if i == active {
            Style::default().fg(t.text).bold()
        } else {
            Style::default().fg(t.text_dim)
        };
        tab_spans.push(Span::styled(label, label_style));
        if dot && let Some(state) = state_map.get(tab.as_str()) {
            let (glyph, color) = match state {
                ServiceState::Running => ("●", t.status_running),
                ServiceState::Starting => (spinner::frame(tick), t.status_starting),
                ServiceState::Failed => ("✗", t.status_failed),
                ServiceState::Stopped => ("○", t.status_stopped),
                ServiceState::Disabled => ("⊘", t.status_stopped),
                ServiceState::Suspended => ("⏸", t.text_dim),
            };
            tab_spans.push(Span::styled(glyph, Style::default().fg(color)));
        }
        if badge.errors > 0 {
            let s = format!(" ✗{}", badge.errors);
            let w = s.chars().count() as u16;
            // Badge click sits on top of the chip click (pushed later = wins).
            let bx = col - 1 - w;
            clicks.click(hit(bx, w), Message::OpenTabErrors(i));
            tab_spans.push(Span::styled(s, Style::default().fg(t.error).bold()));
        } else if badge.activity {
            tab_spans.push(Span::styled(" ◆", Style::default().fg(t.accent)));
        }
        tab_spans.push(Span::raw(" "));

        let under = if i == active {
            Span::styled("━".repeat(chip_w as usize), Style::default().fg(t.accent))
        } else {
            Span::styled(
                "─".repeat(chip_w as usize),
                Style::default().fg(t.border_subtle),
            )
        };
        under_spans.push(under);
        under_spans.push(Span::raw(" "));
    }

    if win.hidden_right > 0 {
        let s = format!("› +{}", win.hidden_right);
        let w = s.chars().count() as u16;
        tab_spans.push(Span::styled(s, Style::default().fg(t.text_dim)));
        under_spans.push(Span::raw(" ".repeat(w as usize)));
        clicks.click(hit(col, w), Message::NextTab);
    }

    let paragraph = Paragraph::new(vec![Line::from(tab_spans), Line::from(under_spans)])
        .block(Block::default().padding(Padding::left(1)));
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn names(n: usize) -> Vec<String> {
        std::iter::once("All".to_string())
            .chain((1..n).map(|i| format!("service-{i}")))
            .collect()
    }

    fn draw(w: u16, tabs: &[String], active: usize) -> (String, ClickMap) {
        let states: Vec<(String, ServiceState)> = tabs
            .iter()
            .skip(1)
            .map(|n| (n.clone(), ServiceState::Running))
            .collect();
        let mut term = Terminal::new(TestBackend::new(w, 2)).unwrap();
        let clicks = ClickMap::default();
        term.draw(|f| {
            let view = TabsView {
                tabs,
                active,
                states: &states,
                badges: &[],
                tick: 0,
            };
            render(f, f.area(), &view, &clicks)
        })
        .unwrap();
        let buf = term.backend().buffer();
        let row0: String = (0..w).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        (row0, clicks)
    }

    #[test]
    fn everything_fits_no_indicators() {
        let (row, _) = draw(120, &names(4), 0);
        assert!(!row.contains('‹') && !row.contains('›'), "{row}");
        assert!(row.contains("service-3"));
    }

    #[test]
    fn overflow_never_cuts_a_label_and_always_draws_indicator() {
        let tabs = names(9);
        for w in [40u16, 60, 70, 90] {
            for active in 0..tabs.len() {
                let (row, _) = draw(w, &tabs, active);
                let chars: Vec<char> = row.chars().collect();
                // every `service-` prefix is followed by its full number
                let needle: Vec<char> = "service-".chars().collect();
                for start in 0..chars.len().saturating_sub(needle.len()) {
                    if chars[start..start + needle.len()] == needle[..] {
                        let digit = chars.get(start + needle.len());
                        assert!(
                            digit.is_some_and(|c| c.is_ascii_digit()),
                            "cut label at w={w} active={active}: {row}"
                        );
                    }
                }
                assert!(
                    row.contains('›') || row.contains('‹'),
                    "indicator missing at w={w} active={active}: {row}"
                );
                if active > 0 {
                    assert!(
                        row.contains(&format!("service-{active}")),
                        "active tab visible at w={w}: {row}"
                    );
                }
            }
        }
    }

    #[test]
    fn indicators_carry_hidden_counts() {
        let (row, _) = draw(70, &names(12), 0);
        assert!(row.contains("› +"), "{row}");
        let (row, _) = draw(70, &names(12), 11);
        assert!(row.contains("‹ +"), "{row}");
    }

    #[test]
    fn indicator_and_chip_clicks_are_inside_the_area() {
        let tabs = names(12);
        let (_, clicks) = draw(60, &tabs, 5);
        let area = Rect::new(0, 0, 60, 2);
        for (r, _) in clicks.targets.borrow().iter() {
            assert_eq!(r.intersection(area), *r, "click rect outside area: {r:?}");
        }
        let kinds: Vec<_> = clicks
            .targets
            .borrow()
            .iter()
            .map(|(_, t)| format!("{t:?}"))
            .collect();
        assert!(kinds.iter().any(|k| k.contains("PrevTab")));
        assert!(kinds.iter().any(|k| k.contains("NextTab")));
    }

    #[test]
    fn window_active_always_whole_and_counts_add_up() {
        let widths = vec![9usize; 10];
        for active in 0..10 {
            let w = window(&widths, active, 40);
            assert!(w.first <= active && active < w.last_exclusive);
            assert_eq!(
                w.hidden_left + (w.last_exclusive - w.first) + w.hidden_right,
                10
            );
        }
    }

    #[test]
    fn error_badge_gets_its_own_click_target() {
        let tabs = names(3);
        let states: Vec<(String, ServiceState)> = tabs
            .iter()
            .skip(1)
            .map(|n| (n.clone(), ServiceState::Running))
            .collect();
        let badges = vec![
            TabBadge::default(),
            TabBadge {
                activity: false,
                errors: 3,
            },
            TabBadge::default(),
        ];
        let mut term = Terminal::new(TestBackend::new(100, 2)).unwrap();
        let clicks = ClickMap::default();
        term.draw(|f| {
            let view = TabsView {
                tabs: &tabs,
                active: 0,
                states: &states,
                badges: &badges,
                tick: 0,
            };
            render(f, f.area(), &view, &clicks)
        })
        .unwrap();
        let buf = term.backend().buffer();
        let row: String = (0..100).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row.contains("✗3"), "{row}");
        let cols: Vec<char> = row.chars().collect();
        let x = cols.windows(2).position(|w| w == ['✗', '3']).unwrap() as u16;
        assert!(matches!(
            clicks.click_at(x, 0),
            Some(Message::OpenTabErrors(1))
        ));
    }
}
