//! Chrome-level App behavior: persisted UI state, zoom, restart-all planning,
//! and the narrow status strip's "worst issue" pick.

use crate::runtime::service::ServiceState;

use super::{App, LevelFilter, Message};

/// The one-line summary shown above the footer when there is no sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripIssue {
    pub icon: &'static str,
    pub text: String,
    /// Service whose tab a click opens.
    pub tab: String,
    pub failed: bool,
}

impl App {
    /// Path of the UI state file's directory.
    fn ui_dir(&self) -> std::path::PathBuf {
        self.manager.log_dir().to_path_buf()
    }

    /// Restore tab / zoom / toggles / level from `.logs/.atlas-ui.json`.
    pub fn load_ui_state(&mut self) {
        self.ui = super::super::ui_state::UiState::load(&self.ui_dir());
        self.zoom = self.ui.zoom;
        if let Some(name) = self.ui.tab.clone()
            && let Some(idx) = self.tabs.iter().position(|t| *t == name)
        {
            self.active_tab = idx;
        }
        self.logs.level_filter = match self.ui.level.as_deref() {
            Some("warn") => LevelFilter::Warn,
            Some("error") => LevelFilter::Error,
            _ => LevelFilter::All,
        };
        if let Some(v) = self.ui.wrap {
            self.logs.log_wrap = v;
        }
        if let Some(v) = self.ui.timestamps {
            self.logs.log_timestamps = v;
        }
        if let Some(v) = self.ui.json_pretty {
            self.logs.log_json_pretty = v;
        }
    }

    /// Snapshot the live UI state into `self.ui` and write it. Cheap; called on change.
    pub fn save_ui_state(&mut self) {
        self.ui.tab = self.tabs.get(self.active_tab).cloned();
        self.ui.zoom = self.zoom;
        self.ui.wrap = Some(self.logs.log_wrap);
        self.ui.timestamps = Some(self.logs.log_timestamps);
        self.ui.json_pretty = Some(self.logs.log_json_pretty);
        self.ui.level = Some(
            match self.logs.level_filter {
                LevelFilter::All => "all",
                LevelFilter::Warn => "warn",
                LevelFilter::Error => "error",
            }
            .to_string(),
        );
        self.ui.save(&self.ui_dir());
    }

    /// Names `r` on the All tab would restart: everything except completed one-shots
    /// and disabled services.
    pub fn restart_all_targets(&self) -> Vec<String> {
        self.manager
            .services()
            .iter()
            .filter(|s| s.state != ServiceState::Disabled)
            .filter(|s| !(s.state == ServiceState::Stopped && self.manager.completed_ok(&s.name)))
            .map(|s| s.name.clone())
            .collect()
    }

    /// `r` on All: confirm first (a stray key must not bounce the whole stack).
    pub(super) fn ask_restart_all(&mut self) {
        let n = self.restart_all_targets().len();
        if n == 0 {
            self.set_toast("Nothing to restart", super::ToastVariant::Info);
            return;
        }
        let noun = if n == 1 { "service" } else { "services" };
        self.layer = super::Layer::Confirm {
            title: "Restart all".to_string(),
            body: format!("Restart {n} {noun}? Completed oneshots will not re-run."),
            on_confirm: Box::new(Message::RestartAllConfirmed),
        };
    }

    /// Worst problem first: failed services (critical first), then ones still starting.
    pub fn strip_issue(&self) -> Option<StripIssue> {
        let services = self.manager.services();
        let critical: std::collections::HashSet<String> = self
            .manager
            .critical_services_failed()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        let mut failed: Vec<_> = services
            .iter()
            .filter(|s| s.state == ServiceState::Failed)
            .collect();
        failed.sort_by_key(|s| !critical.contains(&s.name));
        if let Some(s) = failed.first() {
            let why = self
                .manager
                .service_note(&s.name)
                .or_else(|| self.manager.exit_code(&s.name).map(|c| format!("exit {c}")))
                .unwrap_or_else(|| "failed".to_string());
            let extra = failed.len() - 1;
            let more = if extra > 0 {
                format!(" (+{extra})")
            } else {
                String::new()
            };
            return Some(StripIssue {
                icon: "✗",
                text: format!("{} {}{more}", s.name, why),
                tab: s.name.clone(),
                failed: true,
            });
        }
        services
            .iter()
            .find(|s| s.state == ServiceState::Starting)
            .map(|s| StripIssue {
                icon: "⠋",
                text: self
                    .manager
                    .service_note(&s.name)
                    .map(|n| format!("{} {n}", s.name))
                    .unwrap_or_else(|| format!("{} starting", s.name)),
                tab: s.name.clone(),
                failed: false,
            })
    }

    /// Sidebar hidden: narrow terminal or the user zoomed.
    pub fn sidebar_hidden(&self, width: u16, sidebar_w: u16) -> bool {
        self.zoom || width < 90 + sidebar_w
    }
}
