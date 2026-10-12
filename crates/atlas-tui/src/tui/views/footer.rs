//! Footer + narrow status strip. Hints come from the one keymap table
//! (`tui/keymap.rs`) filtered by context and service state, so the footer can
//! never advertise a key that does nothing here.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
use crate::theme;
use crate::tui::app::ServiceMenuAction;
use crate::tui::app::{App, Layer, Message, SearchMode};
use crate::tui::clicks::ClickMap;
use crate::tui::keymap::{self, Ctx, KeyCtx};
use crate::tui::views::service_menu::visible_actions;

/// One footer hint: key cap, label, optional click message, display priority
/// (lower = dropped last when space runs out).
#[derive(Debug, Clone)]
pub struct Hint {
    pub key: String,
    pub label: String,
    pub click: Option<Message>,
    /// Style override for the label (Following / Paused).
    pub label_color: Option<Color>,
}

impl Hint {
    fn new(key: &str, label: &str, click: Option<Message>) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            click,
            label_color: None,
        }
    }

    pub fn width(&self) -> usize {
        self.key.chars().count() + self.label.chars().count()
    }
}

const GAP: usize = 2;

/// Keep as many leading hints as fit in `max` columns (gap between each).
/// Hints are ordered by importance, so the rightmost are dropped first.
pub fn fit(hints: Vec<Hint>, max: usize) -> Vec<Hint> {
    let mut used = 0;
    let mut out = Vec::new();
    for h in hints {
        let w = h.width() + if out.is_empty() { 0 } else { GAP };
        if used + w > max {
            break;
        }
        used += w;
        out.push(h);
    }
    out
}

/// Context for one footer: which tab, what state, what mode.
pub struct Ctxt<'a> {
    pub active: Option<&'a str>,
    pub state: Option<ServiceState>,
    pub on_session: bool,
    pub keys: KeyCtx,
    pub following: bool,
    pub searching: bool,
    /// The custom time-range prompt is open.
    pub timing: bool,
    pub find_active: bool,
    pub zoom: bool,
}

/// Build the ordered hint list for the normal (non-attached, non-session) footer.
pub fn hints_for(c: &Ctxt) -> Vec<Hint> {
    let on_service = c.active.is_some();
    if c.timing {
        return vec![
            Hint::new(" Enter ", "apply", None),
            Hint::new("Esc ", "all time", None),
        ];
    }
    if c.searching {
        return vec![
            Hint::new(" Enter ", "keep", None),
            Hint::new("Esc ", "clear", None),
            Hint::new("Ctrl+R ", "regex", None),
            Hint::new("Tab ", "find/filter", None),
        ];
    }
    let t = theme::current();
    let mut out: Vec<Hint> = Vec::new();
    // Allowed per-service ops from the same table the menu uses.
    let ops: &[ServiceMenuAction] = c.state.as_ref().map(visible_actions).unwrap_or(&[]);
    for b in keymap::KEYMAP.iter().filter(|b| b.footer.is_some()) {
        if !keymap::applies(b, on_service, c.on_session, c.keys) {
            continue;
        }
        // Per-service ops only when the menu would offer them in this state.
        if b.ctx == Ctx::Service && !c.on_session {
            let allowed = match b.keys {
                "r" => {
                    ops.contains(&ServiceMenuAction::Restart)
                        || ops.contains(&ServiceMenuAction::Start)
                }
                "s" => ops.contains(&ServiceMenuAction::Stop),
                _ => true,
            };
            if !allowed {
                continue;
            }
        }
        let mut label = b.footer.unwrap_or_default().to_string();
        // `r` on a stopped service starts it.
        if b.keys == "r" && b.ctx == Ctx::Service && ops.contains(&ServiceMenuAction::Start) {
            label = "Start".to_string();
        }
        let mut h = Hint::new(
            &format!("{} ", b.keys),
            &label,
            keymap::click_message(b, c.active),
        );
        match b.keys {
            "f" => {
                h.label = if c.following { "Following" } else { "Paused" }.to_string();
                h.label_color = Some(if c.following {
                    t.status_running
                } else {
                    t.warning
                });
            }
            "z" => {
                h.label = if c.zoom { "Zoom on" } else { "Zoom" }.to_string();
                if c.zoom {
                    h.label_color = Some(t.warning);
                }
            }
            "/" if c.find_active => h.label = "Search (n/N next)".to_string(),
            _ => {}
        }
        out.push(h);
    }
    // Most useful first: help/quit stay, scroll goes first when cramped.
    let rank = |h: &Hint| match h.key.trim() {
        "?" => 0,
        "q" => 1,
        "r" | "s" => 2,
        "/" => 3,
        ":" => 4,
        "f" => 5,
        "m" => 6,
        "v" => 7,
        "V" => 7,
        "z" => 8,
        "i" => 2,
        _ => 9,
    };
    out.sort_by_key(rank);
    out
}

/// Lay out hints on one line starting at `x0`, registering clicks only for hints
/// that were actually drawn. Returns the spans.
pub fn layout_line(
    area: Rect,
    lead: Vec<Span<'static>>,
    hints: &[Hint],
    clicks: &ClickMap,
    clickable: bool,
) -> Line<'static> {
    let t = theme::current();
    let dim = Style::default().fg(t.text_dim);
    let mut x = area.x + lead.iter().map(|s| s.width() as u16).sum::<u16>();
    let mut spans = lead;
    for (i, h) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(GAP)));
            x += GAP as u16;
        }
        let w = h.width() as u16;
        if clickable && let Some(msg) = h.click.clone() {
            clicks.click(Rect::new(x, area.y, w, 1).intersection(area), msg);
        }
        x += w;
        spans.push(Span::styled(h.key.clone(), dim));
        spans.push(Span::styled(
            h.label.clone(),
            h.label_color.map_or(dim, |c| Style::default().fg(c).bold()),
        ));
    }
    Line::from(spans)
}

/// Right-aligned running-operation chip for the footer.
fn operation_chip(app: &App) -> Option<Line<'static>> {
    let t = theme::current();
    let op = app.operation.as_ref()?;
    let (icon, color, detail) = match &op.failed {
        Some((_, last)) => ("✗", t.error, last.clone()),
        None => (
            crate::tui::widgets::spinner::frame(app.tick_count as usize),
            t.primary,
            op.detail.clone(),
        ),
    };
    Some(Line::from(vec![
        Span::styled(
            format!("{icon} {} ", op.label),
            Style::default().fg(color).bold(),
        ),
        Span::styled(
            format!("{} ", op.elapsed_label()),
            Style::default().fg(t.text_dim),
        ),
        Span::styled(format!("{detail} "), Style::default().fg(t.text_muted)),
    ]))
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, hide_operation: bool) {
    let t = theme::current();
    let dim = Style::default().fg(t.text_dim);
    let active = app.tabs.get(app.active_tab).cloned();
    let on_session = active
        .as_deref()
        .is_some_and(|a| app.sessions.tabs.contains(a));
    let is_attached = matches!(app.layer, Layer::SessionAttached(_));
    // While another layer owns the keyboard, footer clicks would be shadowed anyway.
    let keys_live = !app.logs.log_searching && app.logs.time_input.is_none();

    // ── Attached ────────────────────────────────────────────────────
    if is_attached {
        let layer_label = match &app.layer {
            Layer::SessionAttached(l) => l.clone(),
            _ => active.clone().unwrap_or_default(),
        };
        let lead = vec![
            Span::styled(
                " ── ATTACHED ── ",
                Style::default()
                    .fg(Color::Rgb(0x09, 0x09, 0x0B))
                    .bg(t.warning)
                    .bold(),
            ),
            Span::raw("  "),
        ];
        let mut x = area.x + lead.iter().map(|s| s.width() as u16).sum::<u16>();
        let mut spans = lead;
        let items = [
            ("Ctrl+B ", t.warning, "detach", Message::DetachSession),
            (
                "Ctrl+C ",
                t.error,
                "kill session",
                Message::StopSession(layer_label),
            ),
        ];
        for (i, (k, kc, l, msg)) in items.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("    "));
                x += 4;
            }
            let w = (k.chars().count() + l.chars().count()) as u16;
            app.clicks
                .click(Rect::new(x, area.y, w, 1).intersection(area), msg);
            x += w;
            spans.push(Span::styled(k, Style::default().fg(kc).bold()));
            spans.push(Span::styled(l, dim));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    // ── Hints (all / service / session tab) ─────────────────────────
    let svc = (app.active_tab > 0).then(|| active.clone()).flatten();
    let state = svc.as_deref().and_then(|n| app.manager.get_state(n));
    let find_active = !app.logs.log_search_input.is_empty()
        && app.logs.search_mode == SearchMode::Find
        && !app.logs.log_searching;
    let ctxt = Ctxt {
        active: svc.as_deref(),
        state,
        on_session,
        keys: app.key_ctx(),
        following: app.logs.follow_logs,
        searching: app.logs.log_searching,
        timing: app.logs.time_input.is_some(),
        find_active,
        zoom: app.zoom,
    };
    let mut lead: Vec<Span<'static>> = Vec::new();
    if on_session
        && let Some(label) = &active
        && app.sessions.platforms.contains_key(label)
    {
        let phase = app
            .sessions
            .phases
            .get(label)
            .map(|p| p.label())
            .unwrap_or("running");
        let icon = match app.sessions.platforms.get(label) {
            Some(crate::config::SessionPlatform::Ios) => " ",
            Some(crate::config::SessionPlatform::Android) => " ",
            None => "",
        };
        lead.push(Span::styled(
            format!("{icon}{phase}"),
            Style::default().fg(t.primary).bold(),
        ));
        lead.push(Span::raw("   "));
    }
    let lead_w: usize = lead.iter().map(|s| s.width()).sum();
    let chip = if hide_operation {
        None
    } else {
        operation_chip(app)
    };
    let chip_w = chip.as_ref().map_or(0, |c| c.width());
    // Reserve the chip's width (+1 gap) so hints never run under it.
    let budget = (area.width as usize)
        .saturating_sub(lead_w)
        .saturating_sub(if chip_w > 0 { chip_w + 1 } else { 0 });
    let all = hints_for(&ctxt);
    let shown = fit(all, budget);
    let line = layout_line(area, lead, &shown, &app.clicks, keys_live);
    frame.render_widget(Paragraph::new(line), area);

    if let Some(chip) = chip {
        let w = (chip.width() as u16).min(area.width);
        let r = Rect::new(area.right() - w, area.y, w, 1);
        frame.render_widget(Clear, r);
        frame.render_widget(Paragraph::new(chip), r);
    }
}

/// One-line status strip shown above the footer when the sidebar is hidden:
/// worst issue first, then the health count; clickable to the offending tab.
pub fn render_strip(frame: &mut Frame, area: Rect, app: &App) {
    let t = theme::current();
    let health = crate::tui::widgets::health::Health::from_states(
        app.manager
            .services()
            .iter()
            .map(|s| (s.state.clone(), app.manager.completed_ok(&s.name))),
    );
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    if let Some(issue) = app.strip_issue() {
        let color = if issue.failed {
            t.status_failed
        } else {
            t.warning
        };
        let icon = if issue.failed {
            issue.icon.to_string()
        } else {
            crate::tui::widgets::spinner::frame(app.tick_count as usize).to_string()
        };
        spans.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(color).bold(),
        ));
        spans.push(Span::styled(issue.text.clone(), Style::default().fg(color)));
        spans.push(Span::styled(" · ", Style::default().fg(t.text_dim)));
        app.clicks.click(area, Message::GotoTab(issue.tab.clone()));
    } else if health.all_ok() {
        spans.push(Span::styled("● ", Style::default().fg(t.status_running)));
    }
    spans.push(Span::styled(
        health.label(),
        Style::default().fg(t.text_muted),
    ));
    if let Some(op) = &app.operation {
        let (icon, color) = match &op.failed {
            Some(_) => ("✗", t.error),
            None => (
                crate::tui::widgets::spinner::frame(app.tick_count as usize),
                t.primary,
            ),
        };
        spans.push(Span::styled(" · ", Style::default().fg(t.text_dim)));
        spans.push(Span::styled(
            format!("{icon} {} {}", op.label, op.elapsed_label()),
            Style::default().fg(color),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(t.bg_panel)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(active: Option<&'a str>, state: Option<ServiceState>) -> Ctxt<'a> {
        Ctxt {
            active,
            state,
            on_session: false,
            keys: KeyCtx::default(),
            following: true,
            searching: false,
            timing: false,
            find_active: false,
            zoom: false,
        }
    }

    fn labels(h: &[Hint]) -> Vec<String> {
        h.iter().map(|h| format!("{}{}", h.key, h.label)).collect()
    }

    #[test]
    fn running_service_offers_restart_and_stop_not_start() {
        let h = hints_for(&ctx(Some("api"), Some(ServiceState::Running)));
        let l = labels(&h).join("|");
        assert!(l.contains("r Restart"), "{l}");
        assert!(l.contains("s Stop"), "{l}");
        assert!(l.contains("/ Search") && l.contains("m Menu"), "{l}");
    }

    #[test]
    fn stopped_service_offers_start_and_no_stop() {
        let h = hints_for(&ctx(Some("api"), Some(ServiceState::Stopped)));
        let l = labels(&h).join("|");
        assert!(l.contains("r Start"), "{l}");
        assert!(!l.contains("s Stop"), "{l}");
    }

    #[test]
    fn failed_service_has_no_stop() {
        let l = labels(&hints_for(&ctx(Some("api"), Some(ServiceState::Failed)))).join("|");
        assert!(l.contains("r Restart") && !l.contains("s Stop"), "{l}");
    }

    #[test]
    fn all_tab_offers_restart_all_and_no_service_ops() {
        let l = labels(&hints_for(&ctx(None, None))).join("|");
        assert!(l.contains("r Restart all"), "{l}");
        assert!(!l.contains("s Stop") && !l.contains("m Menu"), "{l}");
    }

    #[test]
    fn follow_hint_reflects_state_with_color() {
        let mut c = ctx(None, None);
        let h = hints_for(&c);
        let f = h.iter().find(|h| h.key.trim() == "f").unwrap();
        assert_eq!(f.label, "Following");
        c.following = false;
        let h = hints_for(&c);
        let f = h.iter().find(|h| h.key.trim() == "f").unwrap();
        assert_eq!(f.label, "Paused");
        assert_eq!(f.label_color, Some(theme::current().warning));
    }

    #[test]
    fn search_mode_swaps_in_search_hints() {
        let mut c = ctx(None, None);
        c.searching = true;
        let l = labels(&hints_for(&c)).join("|");
        assert!(l.contains("Enter keep") && l.contains("Esc clear"), "{l}");
        assert!(!l.contains("Quit"));
    }

    #[test]
    fn fit_drops_from_the_right_and_never_exceeds_budget() {
        let h = hints_for(&ctx(Some("api"), Some(ServiceState::Running)));
        let total: usize = h.iter().map(|h| h.width() + GAP).sum();
        for max in [10usize, 25, 40, 60, total] {
            let shown = fit(h.clone(), max);
            let used: usize = shown.iter().map(|h| h.width()).sum::<usize>()
                + GAP * shown.len().saturating_sub(1);
            assert!(used <= max, "used {used} > {max}");
        }
        // help survives the tightest budget that fits anything
        let shown = fit(h, 12);
        assert_eq!(shown.first().map(|h| h.key.trim()), Some("?"));
    }

    #[test]
    fn only_drawn_hints_are_clickable() {
        use ratatui::{Terminal, backend::TestBackend};
        let hints = hints_for(&ctx(Some("api"), Some(ServiceState::Running)));
        let shown = fit(hints.clone(), 30);
        assert!(shown.len() < hints.len());
        let clicks = ClickMap::default();
        let mut term = Terminal::new(TestBackend::new(80, 1)).unwrap();
        term.draw(|f| {
            let line = layout_line(f.area(), vec![], &shown, &clicks, true);
            f.render_widget(Paragraph::new(line), f.area());
        })
        .unwrap();
        let drawn: usize = shown.iter().filter(|h| h.click.is_some()).count();
        assert_eq!(clicks.targets.borrow().len(), drawn);
        // nothing registered beyond the drawn width
        let limit: usize =
            shown.iter().map(|h| h.width()).sum::<usize>() + GAP * shown.len().saturating_sub(1);
        for (r, _) in clicks.targets.borrow().iter() {
            assert!(r.right() as usize <= limit);
        }
    }
}
