use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::ProjectConfig;

use super::health;
use super::service::{ServiceState, ServiceStatus};

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    StateChanged { name: String, state: ServiceState },
    AllStarted,
    AllStopped,
    LogLine { name: String, line: String },
}

pub struct ServiceManager {
    root_dir: PathBuf,
    log_dir: PathBuf,
    services: HashMap<String, ManagedService>,
    order: Vec<String>,
    event_tx: mpsc::UnboundedSender<ManagerEvent>,
}

struct ManagedService {
    command: String,
    status: ServiceStatus,
    child: Option<Child>,
}

impl ServiceManager {
    pub fn new(
        root_dir: PathBuf,
        config: &ProjectConfig,
        event_tx: mpsc::UnboundedSender<ManagerEvent>,
    ) -> Self {
        let log_dir = root_dir.join(".atlas").join("logs");

        let order = resolve_order(config);

        let mut services = HashMap::new();
        for name in &order {
            if let Some(def) = config.services.get(name) {
                if !def.is_enabled() {
                    continue;
                }
                services.insert(
                    name.clone(),
                    ManagedService {
                        command: def.command.clone(),
                        status: ServiceStatus {
                            name: name.clone(),
                            state: ServiceState::Stopped,
                            port: def.port,
                            health_url: def.health.clone(),
                            pid: None,
                        },
                        child: None,
                    },
                );
            }
        }

        Self {
            root_dir,
            log_dir,
            services,
            order,
            event_tx,
        }
    }

    pub fn services(&self) -> Vec<&ServiceStatus> {
        self.order
            .iter()
            .filter_map(|name| self.services.get(name).map(|s| &s.status))
            .collect()
    }

    pub fn service_names(&self) -> Vec<&str> {
        self.order
            .iter()
            .filter(|name| self.services.contains_key(name.as_str()))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn update_state(&mut self, name: &str, state: ServiceState) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.status.state = state;
        }
    }

    pub fn get_state(&self, name: &str) -> Option<ServiceState> {
        self.services.get(name).map(|s| s.status.state.clone())
    }

    pub async fn start_all(&mut self) {
        let _ = std::fs::create_dir_all(&self.log_dir);

        let names: Vec<String> = self.order.clone();
        for name in &names {
            self.start_service(name).await;
        }

        let _ = self.event_tx.send(ManagerEvent::AllStarted);
    }

    pub async fn stop_all(&mut self) {
        let names: Vec<String> = self.order.iter().rev().cloned().collect();
        for name in &names {
            self.stop_service(name).await;
        }

        let _ = self.event_tx.send(ManagerEvent::AllStopped);
    }

    pub async fn restart_all(&mut self) {
        self.stop_all().await;
        self.start_all().await;
    }

    pub async fn restart_service(&mut self, name: &str) {
        self.stop_service(name).await;
        self.start_service(name).await;
    }

    pub async fn check_health(&mut self) {
        let timeout = Duration::from_secs(2);

        let names: Vec<String> = self.order.clone();
        for name in &names {
            let svc = match self.services.get(name.as_str()) {
                Some(s) => s,
                None => continue,
            };

            if svc.status.state == ServiceState::Stopped {
                continue;
            }

            let healthy = if let Some(ref url) = svc.status.health_url {
                health::check_http(url, timeout).await
            } else if let Some(port) = svc.status.port {
                health::check_port("127.0.0.1", port, timeout).await
            } else if let Some(pid) = svc.status.pid {
                health::check_process_alive(pid).await
            } else {
                svc.child
                    .as_ref()
                    .map(|c| c.id().is_some())
                    .unwrap_or(false)
            };

            let new_state = if healthy {
                ServiceState::Running
            } else {
                ServiceState::Failed
            };

            let svc = self.services.get_mut(name.as_str()).unwrap();
            if svc.status.state != new_state {
                svc.status.state = new_state.clone();
                let _ = self.event_tx.send(ManagerEvent::StateChanged {
                    name: name.clone(),
                    state: new_state,
                });
            }
        }
    }

    async fn start_service(&mut self, name: &str) {
        let svc = match self.services.get_mut(name) {
            Some(s) => s,
            None => return,
        };

        if svc.child.is_some() {
            return;
        }

        let parts: Vec<&str> = svc.command.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        svc.status.state = ServiceState::Starting;
        let _ = self.event_tx.send(ManagerEvent::StateChanged {
            name: name.to_string(),
            state: ServiceState::Starting,
        });

        let result = Command::new(parts[0])
            .args(&parts[1..])
            .current_dir(&self.root_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn();

        match result {
            Ok(mut child) => {
                let pid = child.id();
                svc.status.pid = pid;

                // Spawn stdout reader
                if let Some(stdout) = child.stdout.take() {
                    let tx = self.event_tx.clone();
                    let svc_name = name.to_string();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx.send(ManagerEvent::LogLine {
                                name: svc_name.clone(),
                                line,
                            });
                        }
                    });
                }

                // Spawn stderr reader
                if let Some(stderr) = child.stderr.take() {
                    let tx = self.event_tx.clone();
                    let svc_name = name.to_string();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stderr);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx.send(ManagerEvent::LogLine {
                                name: svc_name.clone(),
                                line,
                            });
                        }
                    });
                }

                svc.child = Some(child);
            }
            Err(_) => {
                svc.status.state = ServiceState::Failed;
                let _ = self.event_tx.send(ManagerEvent::StateChanged {
                    name: name.to_string(),
                    state: ServiceState::Failed,
                });
            }
        }
    }

    async fn stop_service(&mut self, name: &str) {
        let svc = match self.services.get_mut(name) {
            Some(s) => s,
            None => return,
        };

        if let Some(ref child) = svc.child {
            if let Some(pid) = child.id() {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;

                // SIGTERM to process group
                let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);

                // Wait up to 2 seconds for graceful shutdown
                tokio::time::sleep(Duration::from_secs(2)).await;

                // SIGKILL survivors
                if health::check_process_alive(pid).await {
                    let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
                }
            }
        }

        svc.child = None;
        svc.status.state = ServiceState::Stopped;
        svc.status.pid = None;

        let _ = self.event_tx.send(ManagerEvent::StateChanged {
            name: name.to_string(),
            state: ServiceState::Stopped,
        });
    }
}

/// Topological sort of services based on depends_on
fn resolve_order(config: &ProjectConfig) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut visited: HashMap<&str, bool> = HashMap::new();

    fn visit<'a>(
        name: &'a str,
        config: &'a ProjectConfig,
        visited: &mut HashMap<&'a str, bool>,
        order: &mut Vec<String>,
    ) {
        if visited.contains_key(name) {
            return;
        }
        visited.insert(name, true);

        if let Some(def) = config.services.get(name) {
            if let Some(ref deps) = def.depends_on {
                for dep in deps {
                    visit(dep, config, visited, order);
                }
            }
        }

        order.push(name.to_string());
    }

    for name in config.services.keys() {
        visit(name, config, &mut visited, &mut order);
    }

    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceDef;

    fn test_config() -> ProjectConfig {
        let mut services = HashMap::new();
        services.insert(
            "db".to_string(),
            ServiceDef {
                command: "docker compose up postgres".to_string(),
                port: Some(5432),
                health: None,
                critical: Some(true),
                depends_on: None,
                enabled: None,
            },
        );
        services.insert(
            "api".to_string(),
            ServiceDef {
                command: "cargo run --bin api".to_string(),
                port: Some(3000),
                health: Some("http://localhost:3000/health".to_string()),
                critical: Some(true),
                depends_on: Some(vec!["db".to_string()]),
                enabled: None,
            },
        );
        services.insert(
            "web".to_string(),
            ServiceDef {
                command: "bun run dev".to_string(),
                port: Some(5173),
                health: None,
                critical: None,
                depends_on: Some(vec!["api".to_string()]),
                enabled: None,
            },
        );

        ProjectConfig {
            name: "test-project".to_string(),
            tunnel: None,
            services,
            infra: None,
        }
    }

    #[test]
    fn test_manager_new_from_config() {
        let config = test_config();
        let (tx, _rx) = mpsc::unbounded_channel();

        let manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);

        assert_eq!(manager.services.len(), 3);
        assert!(manager.services.contains_key("db"));
        assert!(manager.services.contains_key("api"));
        assert!(manager.services.contains_key("web"));

        // All should start as Stopped
        for svc in manager.services.values() {
            assert_eq!(svc.status.state, ServiceState::Stopped);
        }
    }

    #[test]
    fn test_manager_services_order() {
        let config = test_config();
        let (tx, _rx) = mpsc::unbounded_channel();

        let manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);

        let services = manager.services();
        let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();

        // db must come before api, api must come before web
        let db_idx = names.iter().position(|&n| n == "db").unwrap();
        let api_idx = names.iter().position(|&n| n == "api").unwrap();
        let web_idx = names.iter().position(|&n| n == "web").unwrap();

        assert!(db_idx < api_idx, "db should come before api");
        assert!(api_idx < web_idx, "api should come before web");
    }
}
