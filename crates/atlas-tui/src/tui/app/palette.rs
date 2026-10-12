use crate::runtime::service::ServiceState;
use crate::tui::views::service_menu::visible_actions;
use crate::tui::widgets::fuzzy;

use super::{App, Message, PaletteCommand, ServiceMenuAction};

fn cmd(
    id: impl Into<String>,
    name: impl Into<String>,
    description: impl Into<String>,
    shortcut: Option<&str>,
    action: Message,
) -> PaletteCommand {
    PaletteCommand {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        shortcut: shortcut.map(str::to_string),
        action,
        group: None,
        matched: Vec::new(),
    }
}

impl App {
    /// Every command that is valid right now, in display order for an empty query:
    /// recent first, then project actions/open, then built-ins, then per-service ops.
    pub(super) fn palette_commands(&self) -> Vec<PaletteCommand> {
        let mut actions = Vec::new();
        for (group, members) in crate::config::grouped_actions(&self.config.actions) {
            for action in members {
                let mut c = cmd(
                    format!("action:{}", action.key),
                    action.label.clone(),
                    format!("Run: {}", action.command),
                    Some(&action.key.to_string()),
                    Message::RunAction(action.key),
                );
                c.group = group.map(str::to_string);
                actions.push(c);
            }
        }
        for (key, name, port) in self.manager.open_services() {
            let has_target = port.is_some()
                || self
                    .config
                    .services
                    .get(&name)
                    .is_some_and(|d| d.open_url.is_some());
            let desc = if has_target {
                let url = self.service_open_url(&name, port);
                format!("Open {} at {}", name, url.trim_start_matches("http://"))
            } else {
                format!("Open {}", name)
            };
            actions.push(cmd(
                format!("open:{key}"),
                format!("Open {name}"),
                desc,
                Some(&key.to_string()),
                Message::OpenService(key),
            ));
        }

        let mut builtins = vec![
            cmd(
                "b:restart-all",
                "Restart All",
                "Restart all services (asks first)",
                Some("r"),
                Message::RestartAll,
            ),
            cmd(
                "b:help",
                "Help",
                "Show shortcuts",
                Some("?"),
                Message::ShowHelp,
            ),
            cmd(
                "b:zoom",
                "Toggle Zoom",
                "Hide the sidebar, logs full width",
                Some("z"),
                Message::ToggleZoom,
            ),
            cmd(
                "b:bottom",
                "Scroll to Bottom",
                "Jump to latest logs",
                Some("G"),
                Message::ScrollToBottom,
            ),
            cmd(
                "b:top",
                "Scroll to Top",
                "Jump to first log",
                Some("g"),
                Message::ScrollToTop,
            ),
            cmd(
                "b:copy",
                "Copy Logs",
                "Copy recent logs to clipboard",
                Some("L"),
                Message::CopyLogs,
            ),
            cmd(
                "b:copy-ai",
                "Copy for AI",
                "Copy logs formatted for AI prompt",
                Some("P"),
                Message::CopyLogsForAI,
            ),
            cmd(
                "b:copy-err",
                "Copy Errors",
                "Copy only errors",
                Some("E"),
                Message::CopyErrors,
            ),
            cmd(
                "b:quit",
                "Quit",
                "Stop all and exit",
                Some("q"),
                Message::ShowQuit,
            ),
        ];
        builtins.retain(|c| c.id != "b:restart-all" || !self.manager.services().is_empty());

        // Per-service ops: only what the service menu would offer for its state.
        let mut per_service = Vec::new();
        for svc in self.manager.services() {
            // Stopped-by-hand services stay out of the palette; ones left out of
            // this run (`atlas dev SERVICE…` / `--profile`) must be startable here.
            if svc.state == ServiceState::Disabled && !self.manager.is_deselected(&svc.name) {
                continue;
            }
            for action in visible_actions(&svc.state) {
                let (verb, desc, msg) = match action {
                    ServiceMenuAction::Restart => (
                        "Restart",
                        format!("Restart the {} service", svc.name),
                        Message::RestartService(svc.name.clone()),
                    ),
                    ServiceMenuAction::Stop => (
                        "Stop",
                        format!("Stop the {} service", svc.name),
                        Message::StopService(svc.name.clone()),
                    ),
                    ServiceMenuAction::Start => (
                        "Start",
                        format!("Start the {} service", svc.name),
                        Message::RestartService(svc.name.clone()),
                    ),
                };
                per_service.push(cmd(
                    format!("svc:{}:{verb}", svc.name),
                    format!("{verb} {}", svc.name),
                    desc,
                    None,
                    msg,
                ));
            }
        }

        let mut all: Vec<PaletteCommand> = Vec::new();
        all.extend(actions);
        all.extend(builtins);
        all.extend(per_service);

        // Recent commands float to the top, most recent first.
        let mut out = Vec::with_capacity(all.len());
        for id in &self.ui.recent {
            if let Some(pos) = all.iter().position(|c| &c.id == id) {
                out.push(all.remove(pos));
            }
        }
        out.extend(all);
        out
    }

    /// Commands matching the query. Empty query keeps display order; otherwise a
    /// fuzzy subsequence match on `group + name` (then description) ranks results.
    pub(super) fn filter_palette(&self) -> Vec<PaletteCommand> {
        let all = self.palette_commands();
        if self.palette_input.is_empty() {
            return all;
        }
        let q = self.palette_input.as_str();
        let mut scored: Vec<(i32, usize, PaletteCommand)> = all
            .into_iter()
            .enumerate()
            .filter_map(|(i, mut c)| {
                if let Some((s, hits)) = fuzzy::score(q, &c.name) {
                    c.matched = hits;
                    // Name matches always outrank description-only matches.
                    return Some((s + 1000, i, c));
                }
                let hay = match &c.group {
                    Some(g) => format!("{g} {}", c.description),
                    None => c.description.clone(),
                };
                fuzzy::score(q, &hay).map(|(s, _)| (s, i, c))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, c)| c).collect()
    }

    /// Keep `palette_selected` inside the visible window of `rows` rows.
    pub(crate) fn palette_scroll_into_view(&mut self, rows: usize) {
        let rows = rows.max(1);
        let max_off = self.palette_filtered.len().saturating_sub(rows);
        if self.palette_selected < self.palette_offset {
            self.palette_offset = self.palette_selected;
        } else if self.palette_selected >= self.palette_offset + rows {
            self.palette_offset = self.palette_selected + 1 - rows;
        }
        self.palette_offset = self.palette_offset.min(max_off);
    }

    /// Remember a palette command as recently used and persist it.
    pub(super) fn remember_command(&mut self, id: &str) {
        self.ui.touch_recent(id);
        self.save_ui_state();
    }
}
