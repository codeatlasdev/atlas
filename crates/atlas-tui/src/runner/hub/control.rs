//! The runner's answers to control-socket requests.

use serde_json::json;

use super::Hub;
use crate::config::ProjectConfig;
use crate::config::adhoc;
use crate::control::{ControlCmd, ControlRequest, ControlResponse, Resolved, resolve};
use crate::runtime::manager::ServiceManager;

impl Hub {
    /// Handle one control request on the runner. Returns `true` for `shutdown`.
    pub async fn handle(
        &mut self,
        manager: &mut ServiceManager,
        cfg: &mut ProjectConfig,
        req: ControlRequest,
    ) -> bool {
        let ControlRequest { cmd, reply, stream } = req;
        let resp = match cmd {
            ControlCmd::Info => ControlResponse::ok(json!({
                "mode": self.mode,
                "pid": std::process::id(),
                "project": self.project,
                "clients": self.clients(),
                "version": env!("CARGO_PKG_VERSION"),
            })),
            ControlCmd::Shutdown => {
                let _ = reply.send(ControlResponse::ok(json!({ "stopping": true })));
                return true;
            }
            ControlCmd::AddService { command } => {
                let name = adhoc::add_one(cfg, &command);
                let port = cfg.services[&name].port;
                manager.add_service(&name, &cfg.services[&name]).await;
                ControlResponse::ok(json!({ "service": name, "port": port }))
            }
            ControlCmd::Subscribe => match stream {
                Some(stream) => {
                    let snapshot = self.subscribe(manager, stream);
                    manager.set_clients(self.clients());
                    ControlResponse::ok(snapshot)
                }
                None => ControlResponse::err("subscribe needs a streaming connection"),
            },
            service_cmd => self.service_command(manager, cfg, service_cmd).await,
        };
        let _ = reply.send(resp);
        // A client that went away since the last turn must not keep the count up.
        self.subscribers.retain(|s| !s.tx.is_closed());
        manager.set_clients(self.clients());
        false
    }

    async fn service_command(
        &mut self,
        manager: &mut ServiceManager,
        cfg: &ProjectConfig,
        cmd: ControlCmd,
    ) -> ControlResponse {
        match resolve(cmd, cfg) {
            Err(e) => ControlResponse::err(e),
            Ok(Resolved::StartService(n) | Resolved::RestartService(n)) => {
                manager.restart_service(&n).await;
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::StopService(n)) => {
                manager.stop_service_disabled(&n).await;
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::RunAction { key, label }) => {
                let action = manager.find_action(key);
                if action.as_ref().is_some_and(|a| a.platform.is_some()) {
                    ControlResponse::err(format!(
                        "action '{label}' needs the interactive device picker; run it from an owner TUI (atlas down, then atlas dev)"
                    ))
                } else if action.as_ref().is_some_and(|a| a.session) {
                    ControlResponse::err(format!(
                        "action '{label}' is an interactive session; sessions are owner-only (atlas down, then atlas dev)"
                    ))
                } else if manager.action_running(&label) {
                    ControlResponse::err(format!("action '{label}' is already running"))
                } else {
                    manager.run_action(key).await;
                    self.operation = Some((label.clone(), String::new()));
                    ControlResponse::ok(json!({ "action": label, "queued": true }))
                }
            }
        }
    }
}
