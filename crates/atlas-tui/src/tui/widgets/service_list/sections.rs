//! The sidebar's sections, one function each. They append to a `Sidebar`
//! (lines + click rows) and always end with a spacer line.

use ratatui::prelude::*;

use super::{PULSE, Row, Sidebar, clip};
use crate::runtime::manager::MobileBuildPhase;
use crate::runtime::service::ServiceState;
use crate::theme::Theme;
use crate::tui::widgets::health::Health;
use crate::tui::widgets::spinner;

use super::Data;

/// Overall health line + gauge.
pub(super) fn health<'a>(out: &mut Sidebar<'a>, d: &Data<'a>, t: &Theme, width: u16, tick: usize) {
    let health = Health::from_states(d.services.iter().map(|s| {
        let done = d.extras.get(&s.name).is_some_and(|e| e.done);
        (s.state.clone(), done)
    }));
    if d.services.is_empty() {
        out.plain(Line::from(vec![
            Span::styled("○  ", Style::default().fg(t.text_dim)),
            Span::styled("No services", Style::default().fg(t.text_dim)),
        ]));
    } else {
        let (dot, color) = if health.all_ok() {
            ("●", t.status_running)
        } else {
            let color = if health.running == 0 && health.expected() > 0 {
                t.status_failed
            } else if health.failed > 0 {
                t.warning
            } else {
                t.status_starting
            };
            // Pulse only while something is failed or starting.
            let dot = if health.pulses() {
                PULSE[tick % PULSE.len()]
            } else {
                "●"
            };
            (dot, color)
        };
        let text = if health.all_ok() && health.done == 0 && health.stopped == 0 {
            "All systems operational".to_string()
        } else {
            health.label()
        };
        out.plain(Line::from(vec![
            Span::styled(format!("{dot}  "), Style::default().fg(color).bold()),
            Span::styled(
                clip(&text, width.saturating_sub(3) as usize),
                Style::default().fg(t.text),
            ),
        ]));
        if health.expected() > 0 {
            let bar_w = (width.saturating_sub(6) as usize).min(24);
            let filled = (health.running * bar_w) / health.expected();
            let gauge_color = if health.all_ok() {
                t.status_running
            } else if health.running == 0 {
                t.status_failed
            } else {
                t.warning
            };
            out.plain(Line::from(vec![
                Span::styled("█".repeat(filled), Style::default().fg(gauge_color)),
                Span::styled(
                    "░".repeat(bar_w.saturating_sub(filled)),
                    Style::default().fg(t.border),
                ),
                Span::styled(
                    format!("  {}%", health.percent()),
                    Style::default().fg(t.text_dim),
                ),
            ]));
        }
    }
    out.spacer();
}

/// One row per service (+ its scheduler note); returns nothing, the column
/// widths come from `column_widths`.
pub(super) fn services<'a>(
    out: &mut Sidebar<'a>,
    d: &Data<'a>,
    t: &Theme,
    width: u16,
    tick: usize,
    name_w: u16,
    port_w: u16,
) {
    out.plain(Line::from(Span::styled(
        "SERVICES",
        Style::default().fg(t.text_dim),
    )));
    for svc in d.services {
        let extra = d.extras.get(&svc.name).cloned().unwrap_or_default();
        let done = svc.state == ServiceState::Stopped && extra.done;
        let (icon, color) = match svc.state {
            ServiceState::Running => ("●", t.status_running),
            ServiceState::Starting => (spinner::frame(tick), t.status_starting),
            ServiceState::Failed => ("✗", t.status_failed),
            ServiceState::Stopped if done => ("✓", t.status_stopped),
            ServiceState::Stopped => ("○", t.status_stopped),
            ServiceState::Disabled => ("⊘", t.text_muted),
            ServiceState::Suspended => ("⏸", t.text_dim),
            ServiceState::Unhealthy => ("‼", t.status_failed),
            ServiceState::Degraded => ("◐", t.warning),
        };
        let port_str = svc
            .port
            .map(|p| format!(":{p}"))
            .unwrap_or_else(|| "─".to_string());
        let name: String = svc.name.chars().take(name_w as usize).collect();

        // Failed rows carry the exit code; restarted ones the count.
        let mut tail: Vec<Span> = Vec::new();
        if svc.state == ServiceState::Failed
            && let Some(code) = extra.exit_code
        {
            tail.push(Span::styled(
                format!(" exit {code}"),
                Style::default().fg(t.status_failed),
            ));
        }
        if extra.off && svc.state == ServiceState::Disabled {
            tail.push(Span::styled(" off", Style::default().fg(t.text_muted)));
        }
        if extra.restarts > 0 {
            tail.push(Span::styled(
                format!(" ↻{}", extra.restarts),
                Style::default().fg(t.warning),
            ));
        }

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(format!("{icon} "), Style::default().fg(color)),
            Span::styled(
                format!("{name:<name_w$}", name_w = name_w as usize),
                Style::default().fg(t.text),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{port_str:<port_w$}", port_w = port_w as usize),
                Style::default().fg(t.text_dim),
            ),
            // ⋯ affordance — every row has a context menu
            Span::styled("  ⋯", Style::default().fg(t.text_dim)),
        ];
        spans.extend(tail);
        out.push(Line::from(spans), Row::Service(svc.name.clone()));

        if let Some(note) = &extra.note {
            let note_color = if matches!(svc.state, ServiceState::Failed | ServiceState::Unhealthy)
            {
                t.status_failed
            } else {
                t.warning
            };
            out.plain(Line::from(Span::styled(
                format!("   ↳ {}", clip(note, width.saturating_sub(5) as usize)),
                Style::default().fg(note_color),
            )));
        }
    }
    out.spacer();
}

/// Active mobile build sessions with their live phase.
pub(super) fn mobile<'a>(out: &mut Sidebar<'a>, d: &Data<'a>, t: &Theme, tick: usize, name_w: u16) {
    if !d.mobile_phases.is_empty() {
        out.plain(Line::from(Span::styled(
            "MOBILE",
            Style::default().fg(t.text_dim),
        )));
        for (label, phase) in d.mobile_phases {
            let (icon, color) = match phase {
                MobileBuildPhase::Detecting => (spinner::frame(tick), t.text_muted),
                MobileBuildPhase::CompilingRust
                | MobileBuildPhase::GradleBuild
                | MobileBuildPhase::XcodeBuild => (spinner::frame(tick), t.warning),
                MobileBuildPhase::Installing | MobileBuildPhase::Launching => {
                    (spinner::frame(tick), t.primary)
                }
                MobileBuildPhase::Running => ("●", t.status_running),
                MobileBuildPhase::Failed(_) => ("✗", t.status_failed),
            };
            let short_label: String = label.chars().take(name_w as usize).collect();
            out.plain(Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(
                    format!("{short_label:<name_w$}", name_w = name_w as usize),
                    Style::default().fg(t.text_muted),
                ),
                Span::raw(" "),
                Span::styled(phase.label(), Style::default().fg(color)),
            ]));
        }
        out.spacer();
    }
}

/// The non-session action in flight (or its failure).
pub(super) fn operation<'a>(
    out: &mut Sidebar<'a>,
    d: &Data<'a>,
    t: &Theme,
    width: u16,
    tick: usize,
) {
    if let Some(op) = d.operation {
        out.plain(Line::from(Span::styled(
            "OPERATIONS",
            Style::default().fg(t.text_dim),
        )));
        let failed = op.failed.is_some();
        let (icon, color) = if failed {
            ("✗", t.status_failed)
        } else {
            (spinner::frame(tick), t.warning)
        };
        let label_w = width.saturating_sub(9).max(4) as usize;
        out.plain(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("{icon} "), Style::default().fg(color)),
            Span::styled(
                format!("{:<label_w$}", clip(&op.label, label_w)),
                Style::default().fg(t.text),
            ),
            Span::raw(" "),
            Span::styled(op.elapsed_label(), Style::default().fg(t.text_dim)),
        ]));
        let detail = match &op.failed {
            Some((_, last)) => last.as_str(),
            None => op.detail.as_str(),
        };
        if !detail.is_empty() {
            out.plain(Line::from(Span::styled(
                format!("   {}", clip(detail, width.saturating_sub(3) as usize)),
                Style::default().fg(if failed { t.error } else { t.text_dim }),
            )));
        }
        out.spacer();
    }
}

/// Critical alerts with their snippet and hint.
pub(super) fn critical<'a>(out: &mut Sidebar<'a>, d: &Data<'a>, t: &Theme, width: u16) {
    if !d.critical.is_empty() {
        out.plain(Line::from(Span::styled(
            "⚠  ISSUES",
            Style::default().fg(t.error).bold(),
        )));
        for (name, snippet, help) in d.critical {
            out.plain(Line::from(Span::styled(
                format!("  {name} failed"),
                Style::default().fg(t.error),
            )));
            if !snippet.is_empty() {
                out.plain(Line::from(Span::styled(
                    format!("  {}", clip(snippet, width.saturating_sub(2) as usize)),
                    Style::default().fg(t.text_dim),
                )));
            }
            if let Some(help) = help.as_deref().filter(|h| !h.is_empty()) {
                out.plain(Line::from(Span::styled(
                    format!("  → {}", clip(help, width.saturating_sub(4) as usize)),
                    Style::default().fg(t.warning),
                )));
            }
        }
        out.spacer();
    }
}

/// Tunnel state and the kill-on-exit toggle.
pub(super) fn tunnel<'a>(out: &mut Sidebar<'a>, d: &Data<'a>, t: &Theme, width: u16, tick: usize) {
    if d.has_tunnel {
        out.plain(Line::from(Span::styled(
            "TUNNEL",
            Style::default().fg(t.text_dim),
        )));
        match d.tunnel {
            Some(tn) => {
                let (icon, color) = match tn.state {
                    ServiceState::Running => ("●", t.status_running),
                    ServiceState::Starting => (spinner::frame(tick), t.status_starting),
                    ServiceState::Failed => ("✗", t.status_failed),
                    _ => ("○", t.status_stopped),
                };
                out.plain(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled("ssh ", Style::default().fg(t.text)),
                    Span::styled(
                        format!(":{} → {}", tn.local_port, tn.ssh_host),
                        Style::default().fg(t.text_dim),
                    ),
                ]));
                out.plain(Line::from(Span::styled(
                    clip(
                        &format!("   → {}", tn.remote),
                        width.saturating_sub(1) as usize,
                    ),
                    Style::default().fg(t.text_dim),
                )));
            }
            None => out.plain(Line::from(vec![
                Span::raw(" "),
                Span::styled("○ ", Style::default().fg(t.status_stopped)),
                Span::styled("off (opens on demand)", Style::default().fg(t.text_dim)),
            ])),
        }
        let check = if d.tunnel_kill_on_exit { "✓" } else { " " };
        out.plain(Line::from(vec![
            Span::styled(format!(" [{check}] "), Style::default().fg(t.primary)),
            Span::styled("Kill tunnel on exit", Style::default().fg(t.text_dim)),
        ]));
        out.spacer();
    }
}
