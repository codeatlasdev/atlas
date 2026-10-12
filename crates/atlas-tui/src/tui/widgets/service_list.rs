use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use super::health::Health;
use super::spinner;
use crate::runtime::manager::MobileBuildPhase;
use crate::runtime::service::{ServiceState, ServiceStatus};
use crate::theme;
use crate::tui::app::{Message, Operation};
use crate::tui::clicks::ClickMap;

const PULSE: [&str; 4] = ["●", "◉", "○", "◉"];

/// Per-service extras the sidebar shows next to the row.
#[derive(Debug, Clone, Default)]
pub struct SvcExtra {
    /// Scheduler annotation (`waiting for :3000 (12s)`, `restarted ×2`, …).
    pub note: Option<String>,
    pub exit_code: Option<i32>,
    pub restarts: u32,
    /// Completed one-shot (Stopped + done).
    pub done: bool,
    /// Left out of this run (`atlas dev SERVICE…` / `--profile`): shown `off`.
    pub off: bool,
}

/// Everything the sidebar needs, so `height` and `render` see identical data.
pub struct Data<'a> {
    pub services: &'a [&'a ServiceStatus],
    pub extras: &'a HashMap<String, SvcExtra>,
    pub critical: &'a [(String, String, Option<String>)],
    pub tunnel_kill_on_exit: bool,
    pub has_tunnel: bool,
    pub tunnel: Option<&'a crate::runtime::tunnel::TunnelStatus>,
    pub mobile_phases: &'a HashMap<String, MobileBuildPhase>,
    pub operation: Option<&'a Operation>,
}

/// What a built line is, for click mapping.
enum Row {
    Plain,
    Service(String),
}

struct Built<'a> {
    lines: Vec<Line<'a>>,
    rows: Vec<Row>,
    name_w: u16,
    port_w: u16,
}

fn column_widths(services: &[&ServiceStatus], width: u16) -> (u16, u16) {
    let port_w = services
        .iter()
        .map(|s| s.port.map(|p| format!(":{p}").len()).unwrap_or(1))
        .max()
        .unwrap_or(5)
        .max(5) as u16;
    // icon(2) + name + space(1) + port + hint(3) + lead(1) = 7 fixed
    let max_name_w = width.saturating_sub(port_w + 9).max(8);
    let name_w = services
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(8)
        .max(8) as u16;
    (name_w.min(max_name_w), port_w)
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn build<'a>(d: &Data<'a>, width: u16, tick: usize) -> Built<'a> {
    let t = theme::current();
    let mut lines: Vec<Line> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let push = |lines: &mut Vec<Line<'a>>, rows: &mut Vec<Row>, l: Line<'a>, r: Row| {
        lines.push(l);
        rows.push(r);
    };

    // ── Health summary ──────────────────────────────────────────────
    let health = Health::from_states(d.services.iter().map(|s| {
        let done = d.extras.get(&s.name).is_some_and(|e| e.done);
        (s.state.clone(), done)
    }));
    if d.services.is_empty() {
        push(
            &mut lines,
            &mut rows,
            Line::from(vec![
                Span::styled("○  ", Style::default().fg(t.text_dim)),
                Span::styled("No services", Style::default().fg(t.text_dim)),
            ]),
            Row::Plain,
        );
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
        push(
            &mut lines,
            &mut rows,
            Line::from(vec![
                Span::styled(format!("{dot}  "), Style::default().fg(color).bold()),
                Span::styled(
                    clip(&text, width.saturating_sub(3) as usize),
                    Style::default().fg(t.text),
                ),
            ]),
            Row::Plain,
        );
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
            push(
                &mut lines,
                &mut rows,
                Line::from(vec![
                    Span::styled("█".repeat(filled), Style::default().fg(gauge_color)),
                    Span::styled(
                        "░".repeat(bar_w.saturating_sub(filled)),
                        Style::default().fg(t.border),
                    ),
                    Span::styled(
                        format!("  {}%", health.percent()),
                        Style::default().fg(t.text_dim),
                    ),
                ]),
                Row::Plain,
            );
        }
    }
    push(&mut lines, &mut rows, Line::from(""), Row::Plain);

    // ── Service rows ────────────────────────────────────────────────
    push(
        &mut lines,
        &mut rows,
        Line::from(Span::styled("SERVICES", Style::default().fg(t.text_dim))),
        Row::Plain,
    );
    let (name_w, port_w) = column_widths(d.services, width);

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
        push(
            &mut lines,
            &mut rows,
            Line::from(spans),
            Row::Service(svc.name.clone()),
        );

        if let Some(note) = &extra.note {
            let note_color = if svc.state == ServiceState::Failed {
                t.status_failed
            } else {
                t.warning
            };
            push(
                &mut lines,
                &mut rows,
                Line::from(Span::styled(
                    format!("   ↳ {}", clip(note, width.saturating_sub(5) as usize)),
                    Style::default().fg(note_color),
                )),
                Row::Plain,
            );
        }
    }
    push(&mut lines, &mut rows, Line::from(""), Row::Plain);

    // ── Active mobile sessions ──────────────────────────────────────
    if !d.mobile_phases.is_empty() {
        push(
            &mut lines,
            &mut rows,
            Line::from(Span::styled("MOBILE", Style::default().fg(t.text_dim))),
            Row::Plain,
        );
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
            push(
                &mut lines,
                &mut rows,
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled(
                        format!("{short_label:<name_w$}", name_w = name_w as usize),
                        Style::default().fg(t.text_muted),
                    ),
                    Span::raw(" "),
                    Span::styled(phase.label(), Style::default().fg(color)),
                ]),
                Row::Plain,
            );
        }
        push(&mut lines, &mut rows, Line::from(""), Row::Plain);
    }

    // ── Running operation (non-session action) ──────────────────────
    if let Some(op) = d.operation {
        push(
            &mut lines,
            &mut rows,
            Line::from(Span::styled("OPERATIONS", Style::default().fg(t.text_dim))),
            Row::Plain,
        );
        let failed = op.failed.is_some();
        let (icon, color) = if failed {
            ("✗", t.status_failed)
        } else {
            (spinner::frame(tick), t.warning)
        };
        let label_w = width.saturating_sub(9).max(4) as usize;
        push(
            &mut lines,
            &mut rows,
            Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(
                    format!("{:<label_w$}", clip(&op.label, label_w)),
                    Style::default().fg(t.text),
                ),
                Span::raw(" "),
                Span::styled(op.elapsed_label(), Style::default().fg(t.text_dim)),
            ]),
            Row::Plain,
        );
        let detail = match &op.failed {
            Some((_, last)) => last.as_str(),
            None => op.detail.as_str(),
        };
        if !detail.is_empty() {
            push(
                &mut lines,
                &mut rows,
                Line::from(Span::styled(
                    format!("   {}", clip(detail, width.saturating_sub(3) as usize)),
                    Style::default().fg(if failed { t.error } else { t.text_dim }),
                )),
                Row::Plain,
            );
        }
        push(&mut lines, &mut rows, Line::from(""), Row::Plain);
    }

    // ── Critical alerts ─────────────────────────────────────────────
    if !d.critical.is_empty() {
        push(
            &mut lines,
            &mut rows,
            Line::from(Span::styled(
                "⚠  ISSUES",
                Style::default().fg(t.error).bold(),
            )),
            Row::Plain,
        );
        for (name, snippet, help) in d.critical {
            push(
                &mut lines,
                &mut rows,
                Line::from(Span::styled(
                    format!("  {name} failed"),
                    Style::default().fg(t.error),
                )),
                Row::Plain,
            );
            if !snippet.is_empty() {
                push(
                    &mut lines,
                    &mut rows,
                    Line::from(Span::styled(
                        format!("  {}", clip(snippet, width.saturating_sub(2) as usize)),
                        Style::default().fg(t.text_dim),
                    )),
                    Row::Plain,
                );
            }
            if let Some(help) = help.as_deref().filter(|h| !h.is_empty()) {
                push(
                    &mut lines,
                    &mut rows,
                    Line::from(Span::styled(
                        format!("  → {}", clip(help, width.saturating_sub(4) as usize)),
                        Style::default().fg(t.warning),
                    )),
                    Row::Plain,
                );
            }
        }
        push(&mut lines, &mut rows, Line::from(""), Row::Plain);
    }

    // ── Tunnel ──────────────────────────────────────────────────────
    if d.has_tunnel {
        push(
            &mut lines,
            &mut rows,
            Line::from(Span::styled("TUNNEL", Style::default().fg(t.text_dim))),
            Row::Plain,
        );
        match d.tunnel {
            Some(tn) => {
                let (icon, color) = match tn.state {
                    ServiceState::Running => ("●", t.status_running),
                    ServiceState::Starting => (spinner::frame(tick), t.status_starting),
                    ServiceState::Failed => ("✗", t.status_failed),
                    _ => ("○", t.status_stopped),
                };
                push(
                    &mut lines,
                    &mut rows,
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(format!("{icon} "), Style::default().fg(color)),
                        Span::styled("ssh ", Style::default().fg(t.text)),
                        Span::styled(
                            format!(":{} → {}", tn.local_port, tn.ssh_host),
                            Style::default().fg(t.text_dim),
                        ),
                    ]),
                    Row::Plain,
                );
                push(
                    &mut lines,
                    &mut rows,
                    Line::from(Span::styled(
                        clip(
                            &format!("   → {}", tn.remote),
                            width.saturating_sub(1) as usize,
                        ),
                        Style::default().fg(t.text_dim),
                    )),
                    Row::Plain,
                );
            }
            None => push(
                &mut lines,
                &mut rows,
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled("○ ", Style::default().fg(t.status_stopped)),
                    Span::styled("off (opens on demand)", Style::default().fg(t.text_dim)),
                ]),
                Row::Plain,
            ),
        }
        let check = if d.tunnel_kill_on_exit { "✓" } else { " " };
        push(
            &mut lines,
            &mut rows,
            Line::from(vec![
                Span::styled(format!(" [{check}] "), Style::default().fg(t.primary)),
                Span::styled("Kill tunnel on exit", Style::default().fg(t.text_dim)),
            ]),
            Row::Plain,
        );
        push(&mut lines, &mut rows, Line::from(""), Row::Plain);
    }

    Built {
        lines,
        rows,
        name_w,
        port_w,
    }
}

/// Rows the sidebar needs at `width` (trailing blank line trimmed).
pub fn content_height(d: &Data, width: u16) -> u16 {
    let mut n = build(d, width, 0).lines.len() as u16;
    // The last pushed line is always a spacer.
    n = n.saturating_sub(1);
    n
}

pub fn render(frame: &mut Frame, area: Rect, d: &Data, tick: usize, clicks: &ClickMap) {
    let built = build(d, area.width, tick);
    // Row geometry for the click map: `⋯` is the last 3 columns of the row text.
    let row_w = 1 + 2 + built.name_w + 1 + built.port_w + 3;
    for (i, row) in built.rows.iter().enumerate() {
        let Row::Service(name) = row else { continue };
        let y = area.y.saturating_add(i as u16);
        if y >= area.bottom() {
            break;
        }
        let full = Rect::new(area.x, y, row_w, 1).intersection(area);
        let menu = Rect::new(area.x + row_w - 3, y, 3, 1).intersection(area);
        clicks.click(full, Message::SelectService(name.clone()));
        clicks.click(menu, Message::ShowServiceMenu(name.clone()));
    }
    frame.render_widget(Paragraph::new(built.lines).block(Block::default()), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn svc(name: &str, state: ServiceState, port: Option<u16>) -> ServiceStatus {
        ServiceStatus {
            name: name.into(),
            state,
            port,
            health_url: None,
            pid: None,
        }
    }

    fn draw(services: Vec<ServiceStatus>, extras: HashMap<String, SvcExtra>) -> Vec<String> {
        let refs: Vec<&ServiceStatus> = services.iter().collect();
        let phases = HashMap::new();
        let d = Data {
            services: &refs,
            extras: &extras,
            critical: &[],
            tunnel_kill_on_exit: true,
            has_tunnel: false,
            tunnel: None,
            mobile_phases: &phases,
            operation: None,
        };
        let mut term = Terminal::new(TestBackend::new(40, 20)).unwrap();
        term.draw(|f| render(f, f.area(), &d, 0, &ClickMap::default()))
            .unwrap();
        let buf = term.backend().buffer();
        (0..20)
            .map(|y| (0..40).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    #[test]
    fn summary_excludes_done_and_stopped_from_denominator() {
        let mut ex = HashMap::new();
        ex.insert(
            "migrate".to_string(),
            SvcExtra {
                done: true,
                ..Default::default()
            },
        );
        let rows = draw(
            vec![
                svc("api", ServiceState::Running, Some(1)),
                svc("web", ServiceState::Running, Some(2)),
                svc("migrate", ServiceState::Stopped, None),
                svc("old", ServiceState::Stopped, None),
            ],
            ex,
        );
        assert!(
            rows[0].contains("2/2 running · 1 done · 1 stopped"),
            "{}",
            rows[0]
        );
        assert!(rows[1].contains("100%"));
    }

    #[test]
    fn failed_row_shows_exit_code_and_restart_count() {
        let mut ex = HashMap::new();
        ex.insert(
            "api".to_string(),
            SvcExtra {
                exit_code: Some(137),
                restarts: 3,
                note: Some("gave up after 3 restarts".into()),
                ..Default::default()
            },
        );
        let rows = draw(vec![svc("api", ServiceState::Failed, Some(1))], ex);
        let row = rows.iter().find(|r| r.contains("api")).unwrap();
        assert!(row.contains("exit 137"), "{row}");
        assert!(row.contains("↻3"), "{row}");
        assert!(rows.iter().any(|r| r.contains("gave up after 3")));
    }

    #[test]
    fn waiting_note_is_rendered_verbatim() {
        let mut ex = HashMap::new();
        ex.insert(
            "api".to_string(),
            SvcExtra {
                note: Some("waiting for :3000 (12s)".into()),
                ..Default::default()
            },
        );
        let rows = draw(vec![svc("api", ServiceState::Starting, Some(3000))], ex);
        assert!(rows.iter().any(|r| r.contains("waiting for :3000 (12s)")));
    }

    #[test]
    fn deselected_service_row_shows_off() {
        let mut ex = HashMap::new();
        ex.insert(
            "worker".to_string(),
            SvcExtra {
                off: true,
                ..Default::default()
            },
        );
        let rows = draw(
            vec![
                svc("api", ServiceState::Running, Some(1)),
                svc("worker", ServiceState::Disabled, None),
            ],
            ex,
        );
        let w = rows.iter().find(|r| r.contains("worker")).unwrap();
        assert!(w.contains("⊘") && w.contains("off"), "{w}");
        let a = rows.iter().find(|r| r.contains("api")).unwrap();
        assert!(!a.contains("off"), "{a}");
    }

    #[test]
    fn content_height_matches_rendered_lines() {
        let services = [svc("api", ServiceState::Running, Some(1))];
        let refs: Vec<&ServiceStatus> = services.iter().collect();
        let (ex, ph) = (HashMap::new(), HashMap::new());
        let d = Data {
            services: &refs,
            extras: &ex,
            critical: &[],
            tunnel_kill_on_exit: true,
            has_tunnel: false,
            tunnel: None,
            mobile_phases: &ph,
            operation: None,
        };
        // health + gauge + blank + SERVICES + row = 5
        assert_eq!(content_height(&d, 34), 5);
    }
}
