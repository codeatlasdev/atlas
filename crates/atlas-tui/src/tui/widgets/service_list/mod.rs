//! The sidebar: health summary, service rows, mobile sessions, the running
//! operation, critical alerts and the tunnel. `build` lays the sections out
//! (see `sections`); `render` draws them and registers the click targets.

mod sections;

use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::manager::MobileBuildPhase;
use crate::runtime::service::ServiceStatus;
use crate::theme;
use crate::tui::app::{Message, Operation};
use crate::tui::clicks::ClickMap;

pub(super) const PULSE: [&str; 4] = ["●", "◉", "○", "◉"];

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
    /// Memory facts (#131); a line is drawn only when there is something worth saying.
    pub memory: Option<crate::runtime::manager::MemoryFacts>,
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
pub(super) enum Row {
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

/// The sidebar under construction: text lines plus, per line, what a click on
/// it means.
pub(super) struct Sidebar<'a> {
    lines: Vec<Line<'a>>,
    rows: Vec<Row>,
}

impl<'a> Sidebar<'a> {
    pub(super) fn push(&mut self, line: Line<'a>, row: Row) {
        self.lines.push(line);
        self.rows.push(row);
    }

    /// A non-clickable line.
    pub(super) fn plain(&mut self, line: Line<'a>) {
        self.push(line, Row::Plain);
    }

    pub(super) fn spacer(&mut self) {
        self.plain(Line::from(""));
    }
}

fn build<'a>(d: &Data<'a>, width: u16, tick: usize) -> Built<'a> {
    let t = theme::current();
    let mut out = Sidebar {
        lines: Vec::new(),
        rows: Vec::new(),
    };
    let (name_w, port_w) = column_widths(d.services, width);

    sections::health(&mut out, d, t, width, tick);
    sections::services(&mut out, d, t, width, tick, name_w, port_w);
    if !d.mobile_phases.is_empty() {
        sections::mobile(&mut out, d, t, tick, name_w);
    }
    if d.operation.is_some() {
        sections::operation(&mut out, d, t, width, tick);
    }
    if !d.critical.is_empty() {
        sections::critical(&mut out, d, t, width);
    }
    if d.has_tunnel {
        sections::tunnel(&mut out, d, t, width, tick);
    }

    Built {
        lines: out.lines,
        rows: out.rows,
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
    use crate::runtime::service::ServiceState;
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

    /// Facts from the REAL wire payload (`__fixtures__/service-memory.json`, written by the
    /// `runner::wire` test), parsed the way an attached TUI parses them.
    fn real_memory() -> (
        crate::runtime::manager::MemoryFacts,
        crate::runtime::manager::MemoryFacts,
        String,
    ) {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../apps/desktop/src/lib/__fixtures__/service-memory.json"
        ))
        .unwrap();
        let note = rows[0]["note"].as_str().unwrap().to_string();
        (
            crate::runtime::manager::MemoryFacts::from_json(&rows[0]),
            crate::runtime::manager::MemoryFacts::from_json(&rows[1]),
            note,
        )
    }

    #[test]
    fn a_leaking_degraded_service_shows_reason_size_trend_budget_and_the_leak_each_on_a_line() {
        let (api, _, note) = real_memory();
        let mut ex = HashMap::new();
        ex.insert(
            "api".to_string(),
            SvcExtra {
                note: Some(note),
                memory: Some(api),
                ..Default::default()
            },
        );
        let rows = draw(vec![svc("api", ServiceState::Degraded, Some(1))], ex);
        let at = rows.iter().position(|r| r.contains("api")).unwrap();
        assert!(rows[at].contains("◐"), "{}", rows[at]);
        assert!(rows[at + 1].contains("using 1.3 GB"), "{}", rows[at + 1]);
        assert!(rows[at + 2].contains("↳ memory 1.3 GB"), "{}", rows[at + 2]);
        assert!(
            rows[at + 3].contains("↳ rising +360 MB/min"),
            "{}",
            rows[at + 3]
        );
        assert!(rows[at + 4].contains("↳ budget warn"), "{}", rows[at + 4]);
        assert!(
            rows[at + 5].contains("↳ leak suspected: +660 MB"),
            "{}",
            rows[at + 5]
        );
    }

    #[test]
    fn a_steady_service_in_budget_gets_no_memory_line() {
        let (_, web, _) = real_memory();
        let mut ex = HashMap::new();
        ex.insert(
            "web".to_string(),
            SvcExtra {
                memory: Some(web),
                ..Default::default()
            },
        );
        let rows = draw(vec![svc("web", ServiceState::Running, Some(1))], ex);
        assert!(rows.iter().all(|r| !r.contains("memory")), "{rows:?}");
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
