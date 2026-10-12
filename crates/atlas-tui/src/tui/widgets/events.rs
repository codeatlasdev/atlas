use std::collections::VecDeque;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;

/// What happened — decides the icon and color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Crash,
    Done,
    Started,
    Stopped,
    /// Paused by the named action.
    Paused(String),
    Waiting,
    Restart,
    Alert,
    Tunnel,
    Info,
}

/// One sidebar event: `subject` is the service (or tunnel), `text` what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideEvent {
    pub ts_ms: u64,
    pub kind: EventKind,
    pub subject: String,
    pub text: String,
}

impl SideEvent {
    /// Icon + color for the kind.
    pub fn glyph(&self) -> (&'static str, Color) {
        let t = theme::current();
        match self.kind {
            EventKind::Crash => ("✗", t.status_failed),
            EventKind::Done => ("✓", t.status_running),
            EventKind::Started => ("●", t.status_running),
            EventKind::Stopped => ("○", t.status_stopped),
            EventKind::Paused(_) => ("⏸", t.text_dim),
            EventKind::Waiting => ("⧗", t.warning),
            EventKind::Restart => ("↻", t.warning),
            EventKind::Alert => ("⚠", t.warning),
            EventKind::Tunnel => ("⇄", t.primary),
            EventKind::Info => ("·", t.text_muted),
        }
    }

    /// `api crashed` / `web paused by Reset DB`; the icon already carries the kind,
    /// so no `Failed:` / `Stopped:` prefixes.
    pub fn message(&self) -> String {
        match &self.kind {
            EventKind::Paused(by) => format!("{} paused by {by}", self.subject),
            _ if self.text.is_empty() => self.subject.clone(),
            _ => format!("{} {}", self.subject, self.text),
        }
    }
}

fn fmt_time(ms: u64) -> String {
    let mut t = crate::runtime::logs::hms(ms as i64);
    t.truncate(5); // HH:MM
    t
}

/// Build display rows (newest first): a time column only when the minute changes
/// from the row above, blank (aligned) otherwise.
pub fn rows(events: &VecDeque<SideEvent>, max: usize) -> Vec<(String, &SideEvent)> {
    let mut out = Vec::new();
    let mut prev: Option<String> = None;
    for ev in events.iter().rev().take(max) {
        let t = fmt_time(ev.ts_ms);
        let shown = if prev.as_deref() == Some(t.as_str()) {
            "     ".to_string()
        } else {
            t.clone()
        };
        prev = Some(t);
        out.push((shown, ev));
    }
    out
}

/// Renders the sidebar event log, newest first.
pub fn render(frame: &mut Frame, area: Rect, events: &VecDeque<SideEvent>) {
    let t = theme::current();

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "EVENTS",
        Style::default().fg(t.text_dim),
    ))];

    if events.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no events yet",
            Style::default().fg(t.text_dim),
        )));
    } else {
        let available = area.height.saturating_sub(1) as usize; // header
        // 1 indent + 5 time + 1 + icon + 1
        let max_w = area.width.saturating_sub(9) as usize;
        for (time, ev) in rows(events, available) {
            let (icon, color) = ev.glyph();
            let msg: String = ev.message().chars().take(max_w).collect();
            lines.push(Line::from(vec![
                Span::styled(format!(" {time} "), Style::default().fg(t.text_dim)),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(msg, Style::default().fg(t.text_muted)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(Block::default()), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: u64, kind: EventKind, subject: &str, text: &str) -> SideEvent {
        SideEvent {
            ts_ms: ts,
            kind,
            subject: subject.into(),
            text: text.into(),
        }
    }

    #[test]
    fn kinds_have_distinct_icons() {
        let kinds = [
            EventKind::Crash,
            EventKind::Done,
            EventKind::Paused("x".into()),
            EventKind::Waiting,
            EventKind::Restart,
        ];
        let icons: Vec<_> = kinds
            .into_iter()
            .map(|k| ev(0, k, "s", "").glyph().0)
            .collect();
        assert_eq!(icons, ["✗", "✓", "⏸", "⧗", "↻"]);
    }

    #[test]
    fn paused_names_the_action_and_no_redundant_prefix() {
        let e = ev(0, EventKind::Paused("Reset DB".into()), "web", "");
        assert_eq!(e.message(), "web paused by Reset DB");
        let c = ev(0, EventKind::Crash, "api", "crashed");
        assert_eq!(c.message(), "api crashed");
    }

    #[test]
    fn same_minute_timestamps_collapse_to_blank() {
        let base = 1_700_000_000_000u64;
        let mut q = VecDeque::new();
        q.push_back(ev(base, EventKind::Started, "a", ""));
        q.push_back(ev(base + 5_000, EventKind::Started, "b", ""));
        q.push_back(ev(base + 3_600_000, EventKind::Started, "c", ""));
        let r = rows(&q, 10);
        // newest first: c (own minute), b (own minute? differs from c), a (same as b)
        assert_ne!(r[0].0.trim(), "");
        assert_ne!(r[1].0.trim(), "");
        assert_eq!(r[2].0, "     ", "a shares b's minute");
        assert_eq!(r[1].0.len(), r[2].0.len(), "columns stay aligned");
    }
}
