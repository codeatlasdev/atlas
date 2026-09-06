use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::ProjectConfig;

use super::health;
use super::logs::LogStream;
use super::service::{ServiceState, ServiceStatus};

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    StateChanged {
        name: String,
        state: ServiceState,
    },
    AllStarted,
    AllStopped,
    LogLine {
        name: String,
        line: String,
        stream: LogStream,
    },
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
            .filter_map(|name| self.services.get(name).map(|service| &service.status))
            .collect()
    }

    pub fn service_names(&self) -> Vec<&str> {
        self.order
            .iter()
            .filter(|name| self.services.contains_key(name.as_str()))
            .map(String::as_str)
            .collect()
    }

    pub fn update_state(&mut self, name: &str, state: ServiceState) {
        if let Some(service) = self.services.get_mut(name) {
            service.status.state = state;
        }
    }

    pub(crate) fn set_state_if_changed(&mut self, name: &str, state: ServiceState) {
        let changed = match self.services.get_mut(name) {
            Some(service) if service.status.state != state => {
                service.status.state = state.clone();
                true
            }
            _ => false,
        };

        if changed {
            let _ = self.event_tx.send(ManagerEvent::StateChanged {
                name: name.to_string(),
                state,
            });
        }
    }

    pub fn get_state(&self, name: &str) -> Option<ServiceState> {
        self.services
            .get(name)
            .map(|service| service.status.state.clone())
    }

    pub async fn start_all(&mut self) {
        let _ = std::fs::create_dir_all(&self.log_dir);

        let names = self.order.clone();
        for name in names {
            self.start_service(&name).await;
        }

        let _ = self.event_tx.send(ManagerEvent::AllStarted);
    }

    pub async fn stop_all(&mut self) {
        let names: Vec<String> = self.order.iter().rev().cloned().collect();
        for name in names {
            self.stop_service(&name).await;
        }

        let _ = self.event_tx.send(ManagerEvent::AllStopped);
    }

    pub async fn restart_all(&mut self) {
        self.stop_all().await;
        self.start_all().await;
    }

    pub async fn restart_service(&mut self, name: &str) {
        if !self.services.contains_key(name) {
            return;
        }
        self.stop_service(name).await;
        self.start_service(name).await;
    }

    pub async fn check_health(&mut self) {
        let timeout = Duration::from_secs(2);
        let names = self.order.clone();

        for name in names {
            let child_exited = match self.services.get_mut(name.as_str()) {
                Some(service) if service.status.state != ServiceState::Stopped => {
                    match service.child.as_mut() {
                        Some(child) => match child.try_wait() {
                            Ok(Some(_)) | Err(_) => true,
                            Ok(None) => false,
                        },
                        None => false,
                    }
                }
                _ => false,
            };

            if child_exited {
                if let Some(service) = self.services.get_mut(name.as_str()) {
                    service.child = None;
                    service.status.pid = None;
                }
                self.set_state_if_changed(&name, ServiceState::Failed);
                continue;
            }

            let (state, health_url, port, pid, child_alive) = match self.services.get(name.as_str())
            {
                Some(service) => (
                    service.status.state.clone(),
                    service.status.health_url.clone(),
                    service.status.port,
                    service.status.pid,
                    service
                        .child
                        .as_ref()
                        .is_some_and(|child| child.id().is_some()),
                ),
                None => continue,
            };

            if state == ServiceState::Stopped {
                continue;
            }

            // A failed service without a child cannot become healthy because
            // an unrelated process happens to own its configured port.
            if state == ServiceState::Failed && !child_alive && pid.is_none() {
                continue;
            }

            let healthy = if let Some(url) = health_url {
                health::check_http(&url, timeout).await
            } else if let Some(port) = port {
                health::check_port("127.0.0.1", port, timeout).await
            } else if let Some(pid) = pid {
                health::check_process_alive(pid).await
            } else {
                child_alive
            };

            self.set_state_if_changed(
                &name,
                if healthy {
                    ServiceState::Running
                } else {
                    ServiceState::Failed
                },
            );
        }
    }

    async fn start_service(&mut self, name: &str) {
        let can_start = match self.services.get_mut(name) {
            Some(service) => match service.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(None) => false,
                    Ok(Some(_)) | Err(_) => {
                        service.child = None;
                        service.status.pid = None;
                        true
                    }
                },
                None => true,
            },
            None => false,
        };

        if !can_start {
            return;
        }

        let command = match self.services.get(name) {
            Some(service) => service.command.clone(),
            None => return,
        };
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            self.set_state_if_changed(name, ServiceState::Failed);
            return;
        }

        self.set_state_if_changed(name, ServiceState::Starting);

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
                if let Some(service) = self.services.get_mut(name) {
                    service.status.pid = pid;
                }

                if let Some(stdout) = child.stdout.take() {
                    let tx = self.event_tx.clone();
                    let service_name = name.to_string();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx.send(ManagerEvent::LogLine {
                                name: service_name.clone(),
                                line,
                                stream: LogStream::Stdout,
                            });
                        }
                    });
                }

                if let Some(stderr) = child.stderr.take() {
                    let tx = self.event_tx.clone();
                    let service_name = name.to_string();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx.send(ManagerEvent::LogLine {
                                name: service_name.clone(),
                                line,
                                stream: LogStream::Stderr,
                            });
                        }
                    });
                }

                if let Some(service) = self.services.get_mut(name) {
                    service.child = Some(child);
                }
            }
            Err(_) => self.set_state_if_changed(name, ServiceState::Failed),
        }
    }

    async fn stop_service(&mut self, name: &str) {
        let pid = match self.services.get(name) {
            Some(service) => service.child.as_ref().and_then(Child::id),
            None => return,
        };

        if let Some(pid) = pid {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
            tokio::time::sleep(Duration::from_secs(2)).await;

            if health::check_process_alive(pid).await {
                let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
            }
        }

        if let Some(service) = self.services.get_mut(name) {
            service.child = None;
            service.status.pid = None;
        }
        self.set_state_if_changed(name, ServiceState::Stopped);
    }
}

/// Topological sort of services based on depends_on.
fn resolve_order(config: &ProjectConfig) -> Vec<String> {
    let mut order = Vec::new();
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

        if let Some(definition) = config.services.get(name) {
            if let Some(dependencies) = &definition.depends_on {
                for dependency in dependencies {
                    visit(dependency, config, visited, order);
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

        for service in manager.services.values() {
            assert_eq!(service.status.state, ServiceState::Stopped);
        }
    }

    #[test]
    fn test_manager_services_order() {
        let config = test_config();
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);
        let services = manager.services();
        let names: Vec<&str> = services
            .iter()
            .map(|service| service.name.as_str())
            .collect();

        let db_idx = names.iter().position(|&name| name == "db").unwrap();
        let api_idx = names.iter().position(|&name| name == "api").unwrap();
        let web_idx = names.iter().position(|&name| name == "web").unwrap();

        assert!(db_idx < api_idx, "db should come before api");
        assert!(api_idx < web_idx, "api should come before web");
    }

    /// A service whose process exits must be reported Failed even when an
    /// unrelated process happens to be listening on its configured port.
    #[tokio::test]
    async fn test_check_health_detects_exited_child() {
        let mut services = HashMap::new();
        services.insert(
            "short".to_string(),
            ServiceDef {
                command: "true".to_string(),
                // Port 22 is commonly bound by sshd; a naive port probe
                // would report this dead service as Running.
                port: Some(22),
                health: None,
                critical: None,
                depends_on: None,
                enabled: None,
            },
        );
        let config = ProjectConfig {
            name: "crash".to_string(),
            tunnel: None,
            services,
            infra: None,
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let tmp = std::env::temp_dir();
        let mut manager = ServiceManager::new(tmp, &config, tx);

        manager.start_service("short").await;
        // Let `true` exit before probing.
        tokio::time::sleep(Duration::from_millis(300)).await;
        manager.check_health().await;

        assert_eq!(manager.get_state("short"), Some(ServiceState::Failed));
        let service = manager.services.get("short").unwrap();
        assert!(service.child.is_none(), "exited child must be cleared");
        assert!(service.status.pid.is_none(), "stale pid must be cleared");

        let mut saw_failed = false;
        while let Ok(event) = rx.try_recv() {
            if let ManagerEvent::StateChanged { name, state } = event
                && name == "short"
                && state == ServiceState::Failed
            {
                saw_failed = true;
            }
        }
        assert!(saw_failed, "crash must emit a Failed state change");
    }

    /// Repeated health checks on an unchanged service must stay silent so a
    /// stable service cannot flood the UI channel.
    #[tokio::test]
    async fn test_repeated_state_emits_once() {
        let config = test_config();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);

        manager.set_state_if_changed("db", ServiceState::Running);
        manager.set_state_if_changed("db", ServiceState::Running);
        manager.set_state_if_changed("db", ServiceState::Running);

        let mut changes = 0;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, ManagerEvent::StateChanged { .. }) {
                changes += 1;
            }
        }
        assert_eq!(changes, 1, "duplicate states must be suppressed");
    }

    /// A crashed service must be restartable: `start_service` has to notice
    /// the dead child instead of treating the slot as still occupied.
    #[tokio::test]
    async fn test_start_service_replaces_exited_child() {
        let mut services = HashMap::new();
        services.insert(
            "short".to_string(),
            ServiceDef {
                command: "true".to_string(),
                port: None,
                health: None,
                critical: None,
                depends_on: None,
                enabled: None,
            },
        );
        let config = ProjectConfig {
            name: "restart".to_string(),
            tunnel: None,
            services,
            infra: None,
        };

        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = ServiceManager::new(std::env::temp_dir(), &config, tx);

        manager.start_service("short").await;
        let first_pid = manager.services.get("short").unwrap().status.pid;
        tokio::time::sleep(Duration::from_millis(300)).await;

        manager.start_service("short").await;
        let second_pid = manager.services.get("short").unwrap().status.pid;

        assert!(first_pid.is_some(), "first spawn must record a pid");
        assert!(second_pid.is_some(), "restart must spawn a new process");
        assert_ne!(first_pid, second_pid, "restart must not reuse a dead child");
    }
}
