use std::cell::Cell;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::config::{ActionConfig, grouped_actions};
use crate::theme;
use crate::tui::app::Message;
use crate::tui::clicks::ClickMap;
use crate::tui::keymap::{self, KeyCtx, Section};
use crate::tui::widgets::modal::Modal;

/// Width at which help switches to two columns.
pub const TWO_COL_MIN: u16 = 100;

/// One help line: a section header, a key row, or blank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Blank,
    Header(String),
    Sub(String),
    Key { keys: String, label: String },
}

/// Build every help row from the keymap table plus project actions / open keys.
/// Pure so tests can check completeness without rendering.
pub fn rows(
    actions: &[ActionConfig],
    open_svcs: &[(char, String, Option<u16>)],
    keys: KeyCtx,
) -> Vec<Vec<Row>> {
    let mut sections: Vec<Vec<Row>> = Vec::new();
    for sec in Section::ALL {
        let mut v = vec![Row::Header(sec.title().to_string())];
        for b in keymap::help_rows(sec, keys) {
            let ctx = match b.ctx {
                keymap::Ctx::Service => " (service tab)",
                keymap::Ctx::All => " (All tab)",
                keymap::Ctx::Session => " (session tab)",
                keymap::Ctx::Tunnel | keymap::Ctx::Attached | keymap::Ctx::Any => "",
            };
            v.push(Row::Key {
                keys: b.keys.to_string(),
                label: format!("{}{ctx}", b.help),
            });
        }
        sections.push(v);
    }
    if !open_svcs.is_empty() {
        let mut v = vec![Row::Header("OPEN".to_string())];
        for (k, name, port) in open_svcs {
            let label = match port {
                Some(p) => format!("{name} (:{p})"),
                None => name.clone(),
            };
            v.push(Row::Key {
                keys: k.to_string(),
                label,
            });
        }
        sections.push(v);
    }
    if !actions.is_empty() {
        let mut v = vec![Row::Header("PROJECT ACTIONS".to_string())];
        for (group, members) in grouped_actions(actions) {
            if let Some(g) = group {
                v.push(Row::Sub(g.to_string()));
            }
            for a in members {
                v.push(Row::Key {
                    keys: a.key.to_string(),
                    label: a.label.clone(),
                });
            }
        }
        sections.push(v);
    }
    sections
}

/// Flatten sections into one column of rows (blank line between sections).
fn flatten(sections: &[Vec<Row>]) -> Vec<Row> {
    let mut out = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        if i > 0 {
            out.push(Row::Blank);
        }
        out.extend(s.iter().cloned());
    }
    out
}

/// Split sections into two columns of near-equal height, never splitting a section.
fn two_columns(sections: &[Vec<Row>]) -> (Vec<Row>, Vec<Row>) {
    let total: usize = sections.iter().map(|s| s.len() + 1).sum();
    let mut left: Vec<Vec<Row>> = Vec::new();
    let mut used = 0;
    let mut split = sections.len();
    for (i, s) in sections.iter().enumerate() {
        if used + s.len() / 2 >= total / 2 && i > 0 {
            split = i;
            break;
        }
        used += s.len() + 1;
        left.push(s.clone());
    }
    (flatten(&left), flatten(&sections[split..]))
}

fn render_row<'a>(row: &Row, key_w: usize) -> Line<'a> {
    let t = theme::current();
    match row {
        Row::Blank => Line::from(""),
        Row::Header(h) => Line::from(Span::styled(h.clone(), Style::default().fg(t.text_dim))),
        Row::Sub(g) => Line::from(Span::styled(
            format!(" {g}"),
            Style::default().fg(t.text_muted).bold(),
        )),
        Row::Key { keys, label } => Line::from(vec![
            Span::styled(
                format!(" {keys:<key_w$} "),
                Style::default().fg(t.primary).bold(),
            ),
            Span::styled(label.clone(), Style::default().fg(t.text)),
        ]),
    }
}

/// Rows of text `render` would need at `width` (for scroll clamping).
pub fn content_rows(
    actions: &[ActionConfig],
    open_svcs: &[(char, String, Option<u16>)],
    keys: KeyCtx,
    width: u16,
) -> usize {
    let sections = rows(actions, open_svcs, keys);
    if width >= TWO_COL_MIN {
        let (a, b) = two_columns(&sections);
        a.len().max(b.len())
    } else {
        flatten(&sections).len()
    }
}

/// `scroll`/`max_scroll`: row offset; `render` writes back the largest valid offset.
pub fn render(
    frame: &mut Frame,
    actions: &[ActionConfig],
    open_svcs: &[(char, String, Option<u16>)],
    keys: KeyCtx,
    scroll: usize,
    max_scroll: &Cell<usize>,
    clicks: &ClickMap,
) {
    let t = theme::current();
    let area = frame.area();

    let sections = rows(actions, open_svcs, keys);
    let two = area.width >= TWO_COL_MIN;
    let key_w = sections
        .iter()
        .flatten()
        .filter_map(|r| match r {
            Row::Key { keys, .. } => Some(keys.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(4)
        .max(4);
    let widest_label = sections
        .iter()
        .flatten()
        .filter_map(|r| match r {
            Row::Key { label, .. } => Some(label.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(10);
    let col_w = (key_w + 2 + widest_label + 2) as u16;
    let wanted = if two { col_w * 2 + 3 } else { col_w } + 4;
    let width = wanted.min(area.width.saturating_sub(2)).max(20);

    let (left, right) = if two {
        two_columns(&sections)
    } else {
        (flatten(&sections), Vec::new())
    };
    let body_rows = left.len().max(right.len());
    // borders(2) + top pad(1) + footer hint(2)
    let chrome = 5u16;
    let max_h = area.height.saturating_sub(2);
    let height = (body_rows as u16 + chrome).min(max_h).max(6);
    let view_rows = height.saturating_sub(chrome) as usize;
    let max = body_rows.saturating_sub(view_rows);
    max_scroll.set(max);
    let scroll = scroll.min(max);

    let clipped = body_rows > view_rows;
    let title = if clipped {
        format!(
            " Shortcuts · {}-{}/{} ",
            scroll + 1,
            (scroll + view_rows).min(body_rows),
            body_rows
        )
    } else {
        " Shortcuts ".to_string()
    };
    // The backdrop closes help (like Esc / ?); the body swallows clicks.
    let (_, inner) = Modal::new(&title, Message::HideHelp).frame(frame, width, height, clicks);

    let body = Rect::new(
        inner.x + 1,
        inner.y + 1,
        inner.width.saturating_sub(2),
        inner.height.saturating_sub(3),
    );
    let window = |col: &[Row]| -> Vec<Line> {
        col.iter()
            .skip(scroll)
            .take(view_rows)
            .map(|r| render_row(r, key_w))
            .collect()
    };
    if two && right.len() + left.len() > 0 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(body);
        frame.render_widget(Paragraph::new(window(&left)), cols[0]);
        frame.render_widget(Paragraph::new(window(&right)), cols[2]);
    } else {
        frame.render_widget(Paragraph::new(window(&left)), body);
    }

    let foot = if clipped {
        let up = if scroll > 0 { "↑" } else { " " };
        let down = if scroll < max { "↓" } else { " " };
        format!(" {up}{down} j/k scroll · esc / ? close")
    } else {
        " esc / ? to close".to_string()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(foot, Style::default().fg(t.text_dim))),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    const ALL_KEYS: KeyCtx = KeyCtx {
        has_tunnel: true,
        attached: true,
    };

    fn action(key: char, label: &str, group: Option<&str>) -> ActionConfig {
        serde_yaml::from_str(&format!(
            "key: '{key}'\nlabel: {label}\ncommand: echo\n{}",
            group.map(|g| format!("group: {g}\n")).unwrap_or_default()
        ))
        .unwrap()
    }

    fn rendered(w: u16, h: u16, actions: &[ActionConfig], scroll: usize) -> (Vec<String>, usize) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let max = Cell::new(0);
        terminal
            .draw(|f| {
                render(
                    f,
                    actions,
                    &[],
                    ALL_KEYS,
                    scroll,
                    &max,
                    &ClickMap::default(),
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let lines = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        (lines, max.get())
    }

    #[test]
    fn project_actions_render_ungrouped_first_then_groups_in_first_seen_order() {
        let actions = vec![
            action('a', "IosRun", Some("iOS")),
            action('b', "Seed", None),
            action('c', "AndroidRun", Some("Android")),
            action('d', "IosBuild", Some("iOS")),
            action('e', "Migrate", None),
        ];
        let (rows, _) = rendered(80, 90, &actions, 0);
        let pos = |needle: &str| {
            rows.iter()
                .position(|r| r.contains(needle))
                .unwrap_or_else(|| panic!("{needle} not rendered:\n{}", rows.join("\n")))
        };
        let (seed, migrate) = (pos("Seed"), pos("Migrate"));
        let (ios, ios_run, ios_build) = (pos(" iOS"), pos("IosRun"), pos("IosBuild"));
        let (android, android_run) = (pos(" Android"), pos("AndroidRun"));
        assert!(pos("PROJECT ACTIONS") < seed);
        assert!(seed < migrate && migrate < ios, "ungrouped before groups");
        assert!(ios < ios_run && ios_run < ios_build, "iOS members together");
        assert!(
            ios_build < android && android < android_run,
            "groups in first-seen order"
        );
    }

    /// Every keymap row (for the current context set) appears in the help rows.
    #[test]
    fn help_lists_every_keymap_binding() {
        let flat: Vec<Row> = rows(&[], &[], ALL_KEYS).into_iter().flatten().collect();
        for b in keymap::KEYMAP {
            assert!(
                flat.iter().any(|r| matches!(r, Row::Key { keys, label }
                    if keys == b.keys && label.starts_with(b.help))),
                "help is missing {} / {}",
                b.keys,
                b.help
            );
        }
    }

    #[test]
    fn tunnel_key_hidden_without_tunnel() {
        let with: Vec<Row> = rows(&[], &[], ALL_KEYS).into_iter().flatten().collect();
        let without: Vec<Row> = rows(&[], &[], KeyCtx::default())
            .into_iter()
            .flatten()
            .collect();
        let has_t = |v: &[Row]| {
            v.iter()
                .any(|r| matches!(r, Row::Key { keys, .. } if keys == "t"))
        };
        assert!(has_t(&with) && !has_t(&without));
    }

    #[test]
    fn r_is_labelled_per_context() {
        let flat: Vec<Row> = rows(&[], &[], KeyCtx::default())
            .into_iter()
            .flatten()
            .collect();
        let labels: Vec<&str> = flat
            .iter()
            .filter_map(|r| match r {
                Row::Key { keys, label } if keys == "r" => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            labels
                .iter()
                .any(|l| l.contains("Restart this service (service tab)"))
        );
        assert!(
            labels
                .iter()
                .any(|l| l.contains("Restart all") && l.contains("All tab"))
        );
    }

    #[test]
    fn two_columns_at_wide_one_column_at_narrow() {
        let (wide, _) = rendered(160, 50, &[], 0);
        let (narrow, _) = rendered(90, 90, &[], 0);
        let header_row = |rows: &[String]| {
            rows.iter()
                .find(|r| r.contains("GENERAL"))
                .cloned()
                .unwrap_or_default()
        };
        // Wide: GENERAL shares a row with the second column's first section header.
        assert!(
            header_row(&wide).contains("LOGS") || header_row(&wide).contains("NAVIGATION"),
            "{}",
            header_row(&wide)
        );
        assert!(!header_row(&narrow).contains("SERVICES"));
    }

    #[test]
    fn clipped_help_scrolls_and_shows_hint() {
        let (rows, max) = rendered(90, 14, &[], 0);
        assert!(max > 0, "content must exceed a 14-row screen");
        assert!(rows.join("\n").contains("j/k scroll"));
        let (rows2, _) = rendered(90, 14, &[], max);
        // Scrolled to the end: the last section's last key is visible; the first isn't.
        assert!(
            rows2.join("\n").contains("Clear logs"),
            "{}",
            rows2.join("\n")
        );
        assert!(!rows2.join("\n").contains("GENERAL"));
    }
}
