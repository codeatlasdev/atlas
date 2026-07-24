use std::time::{Duration, Instant};

use tokio::process::Command;

use super::health;
use super::service::ServiceState;
use crate::config::TunnelConfig;

pub struct TunnelManager {
    config: TunnelConfig,
    state: ServiceState,
    last_retry: Option<Instant>,
}

impl TunnelManager {
    pub fn new(config: TunnelConfig) -> Self {
        Self {
            config,
            state: ServiceState::Stopped,
            last_retry: None,
        }
    }

    pub fn state(&self) -> &ServiceState {
        &self.state
    }

    /// Check if the tunnel's local port is already open (from previous session)
    pub async fn is_alive(&self) -> bool {
        health::check_port("127.0.0.1", self.config.local_port, Duration::from_secs(1)).await
    }

    /// Probe SSH host reachability (ssh -o ConnectTimeout=5 -o BatchMode=yes <host> true)
    pub async fn probe_host(&self) -> bool {
        Command::new("ssh")
            .args([
                "-o",
                "ConnectTimeout=5",
                "-o",
                "BatchMode=yes",
                &self.config.ssh_host,
                "true",
            ])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Start the SSH tunnel. Returns true if started successfully.
    pub async fn start(&mut self) -> bool {
        self.state = ServiceState::Starting;

        // If port already open, tunnel from previous session
        if self.is_alive().await {
            self.state = ServiceState::Running;
            return true;
        }

        // Probe host first
        if !self.probe_host().await {
            self.state = ServiceState::Failed;
            return false;
        }

        // Kill any stale tunnel processes
        let pattern = format!(
            "ssh.*{}:{}:{}.*{}",
            self.config.local_port,
            self.config.remote_host,
            self.config.remote_port,
            self.config.ssh_host
        );
        let _ = Command::new("pkill")
            .args(["-f", &pattern])
            .output()
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Start tunnel
        let tunnel_spec = format!(
            "{}:{}:{}",
            self.config.local_port, self.config.remote_host, self.config.remote_port
        );
        let result = Command::new("ssh")
            .args([
                "-fNL",
                &tunnel_spec,
                &self.config.ssh_host,
                "-o",
                &format!("ServerAliveInterval={}", self.config.keepalive_interval),
                "-o",
                &format!("ServerAliveCountMax={}", self.config.keepalive_max),
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "ConnectTimeout=10",
            ])
            .output()
            .await;

        #[allow(clippy::collapsible_match)]
        match result {
            Ok(output) if output.status.success() => {
                if self.wait_for_port(Duration::from_secs(5)).await {
                    self.state = ServiceState::Running;
                    self.last_retry = None;
                    true
                } else {
                    self.state = ServiceState::Failed;
                    false
                }
            }
            _ => {
                self.state = ServiceState::Failed;
                false
            }
        }
    }

    /// Check health and attempt reconnection if dead (with cooldown)
    pub async fn check_and_reconnect(&mut self) {
        if self.is_alive().await {
            self.state = ServiceState::Running;
            self.last_retry = None;
            return;
        }

        let cooldown = Duration::from_secs(self.config.reconnect_cooldown as u64);
        if let Some(last) = self.last_retry {
            if last.elapsed() < cooldown {
                return;
            }
        }

        self.last_retry = Some(Instant::now());
        self.state = ServiceState::Failed;
        self.start().await;
    }

    /// Kill the tunnel process
    pub async fn kill(&self) {
        let pattern = format!(
            "ssh.*{}:{}:{}.*{}",
            self.config.local_port,
            self.config.remote_host,
            self.config.remote_port,
            self.config.ssh_host
        );
        let _ = Command::new("pkill")
            .args(["-f", &pattern])
            .output()
            .await;
    }

    async fn wait_for_port(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if health::check_port("127.0.0.1", self.config.local_port, Duration::from_secs(1))
                .await
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tunnel_config() -> TunnelConfig {
        TunnelConfig {
            enabled: Some(true),
            local_port: 54320,
            remote_host: "localhost".to_string(),
            remote_port: 5432,
            ssh_host: "test-server".to_string(),
            keepalive_interval: 15,
            keepalive_max: 3,
            reconnect_cooldown: 30,
        }
    }

    #[test]
    fn test_tunnel_new() {
        let cfg = test_tunnel_config();
        let t = TunnelManager::new(cfg);
        assert_eq!(*t.state(), ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_tunnel_is_alive_closed_port() {
        let cfg = test_tunnel_config();
        let t = TunnelManager::new(cfg);
        assert!(!t.is_alive().await);
    }

    #[test]
    fn test_tunnel_cooldown_respected() {
        let cfg = test_tunnel_config();
        let mut t = TunnelManager::new(cfg);
        t.last_retry = Some(Instant::now());
        assert!(t.last_retry.is_some());
    }
}
