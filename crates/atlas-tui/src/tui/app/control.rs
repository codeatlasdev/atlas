//! Control-socket requests (`atlas mcp` → running TUI): validated, then fed through
//! the same `Message`s the keyboard produces so guards/toasts behave identically.

use serde_json::json;

use crate::control::{ControlCmd, ControlRequest, ControlResponse, Resolved, resolve};

use super::{App, Message};

impl App {
    pub(super) fn handle_control(&mut self, req: ControlRequest) {
        let resp = match req.cmd.clone() {
            ControlCmd::Info => ControlResponse::ok(json!({
                "mode": "tui",
                "pid": std::process::id(),
                "project": self.config.name,
                "clients": 0,
                "version": env!("CARGO_PKG_VERSION"),
            })),
            ControlCmd::Shutdown => {
                self.should_quit = true;
                ControlResponse::ok(json!({ "stopping": true }))
            }
            ControlCmd::Subscribe => ControlResponse::err(
                "this atlas dev owns a terminal and cannot be attached to; \
                 run `atlas dev --detach` to keep services running in the background",
            ),
            ControlCmd::AddService { .. } => ControlResponse::err(
                "this atlas dev owns a terminal and cannot take new services live; \
                 run `atlas down`, then `atlas -- <cmd>` (it starts the stack in the background)",
            ),
            cmd => self.service_control(cmd),
        };
        let _ = req.reply.send(resp);
        self.dirty = true;
    }

    fn service_control(&mut self, cmd: ControlCmd) -> ControlResponse {
        // The control server already audited this request (with the real caller); the reducer
        // arms below audit keyboard actions only.
        self.from_socket = true;
        let resp = self.service_control_inner(cmd);
        self.from_socket = false;
        resp
    }

    fn service_control_inner(&mut self, cmd: ControlCmd) -> ControlResponse {
        match resolve(cmd, &self.config) {
            Err(e) => ControlResponse::err(e),
            Ok(Resolved::StartService(n) | Resolved::RestartService(n)) => {
                self.update(Message::RestartService(n.clone()));
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::StopService(n)) => {
                self.update(Message::StopService(n.clone()));
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::RunAction { key, label }) => {
                let session = self
                    .config
                    .actions
                    .iter()
                    .any(|a| a.key == key && a.session);
                if !session && self.manager.action_running(&label) {
                    ControlResponse::err(format!("action '{label}' is already running"))
                } else {
                    self.update(Message::RunActionConfirmed(key));
                    ControlResponse::ok(json!({ "action": label, "queued": true }))
                }
            }
        }
    }

    /// Record a developer action (keyboard, palette, menu) in the audit trail. Silent for
    /// control-socket requests (the server audited those with the real caller) and when
    /// attached to a runner (the runner's server audits the request this App forwards).
    pub(super) fn audit_human(&self, tool: &str, target: &str) {
        if self.from_socket || self.remote.is_some() {
            return;
        }
        let entry = crate::control::audit::AuditEntry::new(
            &crate::control::Caller::Human,
            tool,
            target,
            &json!({}),
            crate::control::audit::Outcome::Allowed,
            None,
        );
        crate::control::audit::record(self.manager.log_dir(), &entry);
    }

    /// [`App::audit_human`] for `run_action`, naming the action by its label.
    pub(super) fn audit_human_action(&self, key: char) {
        let label = self
            .manager
            .find_action(key)
            .map_or_else(|| key.to_string(), |a| a.label);
        self.audit_human("run_action", &label);
    }
}
